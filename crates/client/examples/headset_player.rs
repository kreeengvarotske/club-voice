//! Developer-only, two-seat loopback headset probe. Input/output at 48 kHz.
use club_voice_protocol::media::{HEADER, Header, MAX_DATAGRAM};
use cpal::{
    SampleFormat, StreamConfig, SupportedStreamConfigRange,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use opus_roundtrip_spike::{Config, FrameDecoder, FrameEncoder, MAX_OPUS_PACKET};
use std::{
    env,
    error::Error,
    io,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const N: usize = 960;
const MAX_AGE: Duration = Duration::from_millis(80);
const HEARTBEAT: Duration = Duration::from_secs(5);
type Frame = (Instant, [i16; N]);
type PlaybackFrame = (u64, Frame);

struct PlaybackBatch {
    first: PlaybackFrame,
    second: Option<PlaybackFrame>,
}

impl PlaybackBatch {
    fn one(stream: u64, frame: Frame) -> Self {
        Self {
            first: (stream, frame),
            second: None,
        }
    }

    fn recovered(stream: u64, concealed: Frame, current: Frame) -> Self {
        Self {
            first: (stream, concealed),
            second: Some((stream, current)),
        }
    }
}

#[derive(Default)]
struct State {
    running: AtomicBool,
    muted: AtomicBool,
    deafened: AtomicBool,
    dropped_capture: AtomicU64,
    dropped_playback: AtomicU64,
    concealed: AtomicU64,
    sent: AtomicU64,
    received: AtomicU64,
    remote_stream: AtomicU64,
    streams_heard: AtomicU64,
}

fn config(mut formats: impl Iterator<Item = SupportedStreamConfigRange>) -> Result<StreamConfig> {
    Ok(formats
        .find_map(|f| {
            (f.sample_format() == SampleFormat::F32 && f.channels() > 0)
                .then(|| f.try_with_sample_rate(48000))
                .flatten()
        })
        .ok_or("device needs a 48 kHz f32 shared-mode format")?
        .config())
}

fn capture(channels: usize, tx: SyncSender<Frame>, state: Arc<State>) -> impl FnMut(&[f32]) {
    let mut frame = [0; N];
    let mut pos = 0;
    move |samples| {
        if state.muted.load(Relaxed) {
            pos = 0;
            return;
        }
        for sample in samples.chunks_exact(channels) {
            frame[pos] = (sample.iter().sum::<f32>() / channels as f32 * 32767.0) as i16;
            pos += 1;
            if pos == N {
                if tx.try_send((Instant::now(), frame)).is_err() {
                    state.dropped_capture.fetch_add(1, Relaxed);
                }
                pos = 0;
            }
        }
    }
}

fn playback(
    channels: usize,
    rx: Receiver<PlaybackBatch>,
    state: Arc<State>,
) -> impl FnMut(&mut [f32]) {
    let mut current = None;
    let mut pending = None;
    let mut pos = 0;
    move |samples| {
        samples.fill(0.0);
        let now = Instant::now();
        let active = state.remote_stream.load(Relaxed);
        if state.deafened.load(Relaxed) {
            current = None;
            pending = None;
            while rx.try_recv().is_ok() {}
            return;
        }
        let mut output = samples.chunks_exact_mut(channels);
        while output.len() > 0 {
            while current.as_ref().is_none_or(|(id, (at, _)): &(u64, Frame)| {
                *id < active || now.saturating_duration_since(*at) > MAX_AGE
            }) {
                if current.take().is_some() {
                    state.dropped_playback.fetch_add(1, Relaxed);
                }
                current = pending.take().or_else(|| {
                    rx.try_recv().ok().map(|batch| {
                        pending = batch.second;
                        batch.first
                    })
                });
                pos = 0;
                if current.is_none() {
                    return;
                }
            }
            let (_, (_, pcm)) = current.as_ref().unwrap();
            let count = (N - pos).min(output.len());
            for (channels_out, sample) in output.by_ref().take(count).zip(&pcm[pos..pos + count]) {
                channels_out.fill(*sample as f32 / 32768.0);
            }
            pos += count;
            if pos == N {
                current = None;
            }
        }
    }
}

fn transmit(socket: &UdpSocket, rx: Receiver<Frame>, state: &State, stream: u64) -> Result<()> {
    let mut encoder = FrameEncoder::new(&Config::default())?;
    let mut payload = [0; MAX_OPUS_PACKET];
    let mut packet = [0; MAX_DATAGRAM];
    let mut seq = 0u32;
    let mut heartbeat = Instant::now();
    while state.running.load(Relaxed) {
        if heartbeat.elapsed() >= HEARTBEAT {
            socket.send(&club_voice_protocol::media::SessionCommand::Ping.packet(stream))?;
            heartbeat = Instant::now();
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok((at, pcm)) if at.elapsed() <= MAX_AGE && !state.muted.load(Relaxed) => {
                let size = encoder.encode(&pcm, &mut payload)?;
                let sent_us = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;
                Header {
                    stream,
                    frame_ms: 20,
                    sequence: seq,
                    sent_us,
                }
                .write(&mut packet);
                packet[HEADER..HEADER + size].copy_from_slice(&payload[..size]);
                socket.send(&packet[..HEADER + size])?;
                seq = seq.wrapping_add(1);
                state.sent.fetch_add(1, Relaxed);
            }
            Ok(_) => {
                state.dropped_capture.fetch_add(1, Relaxed);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    Ok(())
}

fn receive(
    socket: &UdpSocket,
    tx: SyncSender<PlaybackBatch>,
    state: &State,
    mut probe_drop_sequence: Option<u32>,
) -> Result<()> {
    let mut decoder = FrameDecoder::new(&Config::default())?;
    let mut packet = [0; MAX_DATAGRAM + 1];
    let mut frame = [0; N];
    let mut concealed = [0; N];
    let mut active = 0;
    let mut last_sequence = 0;
    let mut server_seen = Instant::now();
    while state.running.load(Relaxed) {
        if server_seen.elapsed() >= HEARTBEAT * 3 {
            return Err("voice server stopped responding; reconnect".into());
        }
        let len = match socket.recv(&mut packet) {
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        if &packet[..len] == b"PONG" {
            server_seen = Instant::now();
            continue;
        }
        if &packet[..len] == b"JOIN_REQUIRED" {
            return Err("server session expired; reconnect".into());
        }
        let Some((header, payload)) = Header::parse(&packet[..len]) else {
            continue;
        };
        server_seen = Instant::now();
        if header.frame_ms != 20 || state.deafened.load(Relaxed) {
            continue;
        }
        if probe_drop_sequence == Some(header.sequence) {
            probe_drop_sequence = None;
            continue;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
        let Some(age) = now
            .checked_sub(header.sent_us.into())
            .filter(|age| *age <= MAX_AGE.as_micros())
        else {
            state.dropped_playback.fetch_add(1, Relaxed);
            continue;
        };
        let at = Instant::now() - Duration::from_micros(age as u64);
        if header.stream == 0 {
            state.dropped_playback.fetch_add(1, Relaxed);
            continue;
        }
        let new_stream = header.stream > active;
        let distance = if new_stream {
            1
        } else if header.stream == active {
            let Some(distance) = forward_distance(header.sequence, last_sequence) else {
                state.dropped_playback.fetch_add(1, Relaxed);
                continue;
            };
            distance
        } else {
            state.dropped_playback.fetch_add(1, Relaxed);
            continue;
        };
        if new_stream {
            decoder.reset()?;
        }
        let recovered =
            match decode_after_gap(&mut decoder, distance, payload, &mut frame, &mut concealed) {
                Ok(recovered) => recovered,
                Err(_) => {
                    state.dropped_playback.fetch_add(1, Relaxed);
                    continue;
                }
            };
        if new_stream {
            active = header.stream;
            state.remote_stream.store(active, Relaxed);
            state.streams_heard.fetch_add(1, Relaxed);
        }
        let batch = if recovered {
            state.concealed.fetch_add(1, Relaxed);
            let concealed_at = at.checked_sub(Duration::from_millis(20)).unwrap_or(at);
            PlaybackBatch::recovered(header.stream, (concealed_at, concealed), (at, frame))
        } else {
            PlaybackBatch::one(header.stream, (at, frame))
        };
        if tx.try_send(batch).is_err() {
            state.dropped_playback.fetch_add(1, Relaxed);
        }
        state.received.fetch_add(1, Relaxed);
        last_sequence = header.sequence;
    }
    Ok(())
}

fn forward_distance(sequence: u32, previous: u32) -> Option<u32> {
    let distance = sequence.wrapping_sub(previous);
    matches!(distance, 1..=0x7fff_ffff).then_some(distance)
}

fn decode_after_gap(
    decoder: &mut FrameDecoder,
    distance: u32,
    payload: &[u8],
    current: &mut [i16; N],
    concealed: &mut [i16; N],
) -> Result<bool> {
    let recovered = distance == 2;
    if recovered {
        decoder.decode(None, concealed)?;
    } else if distance > 2 {
        decoder.reset()?;
    }
    if let Err(error) = decoder.decode(Some(payload), current) {
        decoder.reset()?;
        return Err(error.into());
    }
    Ok(recovered)
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let [address, name, consent] = args.as_slice() else {
        return Err("Usage: headset_player 127.0.0.1:PORT NAME --enable-microphone".into());
    };
    let address: SocketAddrV4 = address.parse()?;
    if !address.ip().is_loopback()
        || !matches!(consent.as_str(), "--enable-microphone" | "--tone-peer")
        || name.trim().is_empty()
        || name.len() > 48
        || name.contains('\0')
    {
        return Err("expected loopback, a display name and --enable-microphone".into());
    }
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    socket.connect(address)?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    let stream = club_voice_client::join_loopback_voice(&socket, name)?;
    let result = if consent == "--tone-peer" {
        tone_peer(&socket, stream)
    } else {
        session(&socket, stream)
    };
    let _ = socket.send(&club_voice_protocol::media::SessionCommand::Leave.packet(stream));
    result
}

fn endpoint(host: &cpal::Host, input: bool) -> Result<cpal::Device> {
    let variable = if input {
        "CLUB_VOICE_INPUT_ID"
    } else {
        "CLUB_VOICE_OUTPUT_ID"
    };
    match env::var(variable) {
        Ok(value) => {
            let id = value.parse::<cpal::DeviceId>()?;
            let device = host
                .device_by_id(&id)
                .ok_or_else(|| format!("selected endpoint in {variable} is unavailable"))?;
            if (input && !device.supports_input()) || (!input && !device.supports_output()) {
                return Err(
                    format!("selected endpoint in {variable} has the wrong direction").into(),
                );
            }
            Ok(device)
        }
        Err(env::VarError::NotPresent) => {
            let default = if input {
                host.default_input_device()
            } else {
                host.default_output_device()
            };
            default.ok_or_else(|| format!("no default endpoint for {variable}").into())
        }
        Err(e) => Err(e.into()),
    }
}

fn session(socket: &UdpSocket, stream: u64) -> Result<()> {
    let host = cpal::default_host();
    let input = endpoint(&host, true)?;
    let output = endpoint(&host, false)?;
    let i = config(input.supported_input_configs()?)?;
    let o = config(output.supported_output_configs()?)?;
    let state = Arc::new(State::default());
    let (cap_tx, cap_rx) = mpsc::sync_channel(2);
    let (play_tx, play_rx) = mpsc::sync_channel(1);
    let (stop, done) = mpsc::sync_channel::<Result<()>>(1);
    let on_error = || {
        let stop = stop.clone();
        move |e: cpal::Error| {
            let _ = stop.try_send(Err(e.into()));
        }
    };
    let mut record = capture(i.channels.into(), cap_tx, state.clone());
    let mut render = playback(o.channels.into(), play_rx, state.clone());
    let microphone = input.build_input_stream(
        i,
        move |samples, _| record(samples),
        on_error(),
        Some(Duration::from_secs(2)),
    )?;
    let headphones = output.build_output_stream(
        o,
        move |samples, _| render(samples),
        on_error(),
        Some(Duration::from_secs(2)),
    )?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    headphones.play()?;
    microphone.play()?;
    state.running.store(true, Relaxed);
    println!(
        "Microphone live: {input}. Playback: {output}. Type mute, unmute, deafen, undeafen or quit and press Enter."
    );
    let controls = state.clone();
    let user_stop = stop.clone();
    thread::spawn(move || {
        for line in io::stdin().lines() {
            match line.as_deref() {
                Ok("mute") => controls.muted.store(true, Relaxed),
                Ok("unmute") => controls.muted.store(false, Relaxed),
                Ok("deafen") => controls.deafened.store(true, Relaxed),
                Ok("undeafen") => controls.deafened.store(false, Relaxed),
                Ok("quit") | Err(_) => break,
                _ => eprintln!("Expected mute, unmute, deafen, undeafen or quit"),
            }
        }
        let _ = user_stop.try_send(Ok(()));
    });
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            let _ = stop.try_send(transmit(socket, cap_rx, &state, stream));
        });
        scope.spawn(|| {
            let _ = stop.try_send(receive(socket, play_tx, &state, None));
        });
        let result = done.recv();
        state.running.store(false, Relaxed);
        result
    })?;
    drop(microphone);
    drop(headphones);
    println!(
        "sent={} decoded={} capture_drops={} playback_drops={}",
        state.sent.load(Relaxed),
        state.received.load(Relaxed),
        state.dropped_capture.load(Relaxed),
        state.dropped_playback.load(Relaxed)
    );
    result
}

// Runs the same PCM adapters and both network workers using generated samples.
fn tone_peer(socket: &UdpSocket, stream: u64) -> Result<()> {
    let seconds = env::var("CLUB_VOICE_PROBE_SECONDS")
        .unwrap_or_else(|_| "3".to_owned())
        .parse::<u64>()?;
    if !(3..=300).contains(&seconds) {
        return Err("probe duration must be 3..300 seconds".into());
    }
    let state = Arc::new(State::default());
    let probe_drop_sequence = env::var("CLUB_VOICE_PROBE_DROP_SEQUENCE")
        .ok()
        .map(|value| value.parse::<u32>())
        .transpose()?;
    let (cap_tx, cap_rx) = mpsc::sync_channel(2);
    let (play_tx, play_rx) = mpsc::sync_channel(1);
    let mut record = capture(1, cap_tx, state.clone());
    let mut render = playback(2, play_rx, state.clone());
    let input: [f32; N] =
        std::array::from_fn(|i| (std::f32::consts::TAU * 500.0 * i as f32 / 48000.0).sin() * 0.15);
    let mut output = [0.0; N * 2];
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    state.running.store(true, Relaxed);
    println!("READY");
    let mut audible = 0;
    thread::scope(|scope| -> Result<()> {
        let sender = scope.spawn(|| transmit(socket, cap_rx, &state, stream));
        let receiver = scope.spawn(|| receive(socket, play_tx, &state, probe_drop_sequence));
        let start = Instant::now();
        for i in 1..=seconds * 50 {
            if receiver.is_finished() || sender.is_finished() {
                break;
            }
            record(&input);
            render(&mut output);
            if output.iter().any(|s| s.abs() > 0.02) {
                audible += 1;
            }
            thread::sleep(
                (start + Duration::from_millis(i * 20)).saturating_duration_since(Instant::now()),
            );
        }
        state.running.store(false, Relaxed);
        sender.join().expect("sender panicked")?;
        receiver.join().expect("receiver panicked")?;
        Ok(())
    })?;
    println!(
        "sent={} decoded={} concealed={} audible_blocks={} streams_heard={}",
        state.sent.load(Relaxed),
        state.received.load(Relaxed),
        state.concealed.load(Relaxed),
        audible,
        state.streams_heard.load(Relaxed)
    );
    if audible < 50 {
        return Err("expected at least one second of received signal".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequence_distance_accepts_loss_and_wraparound() {
        assert_eq!(forward_distance(0, u32::MAX), Some(1));
        assert_eq!(forward_distance(6, 3), Some(3));
        assert_eq!(forward_distance(3, 3), None);
        assert_eq!(forward_distance(u32::MAX, 0), None);
    }

    #[test]
    fn decoder_conceals_one_loss_and_resets_after_a_burst() {
        let cfg = Config::default();
        let mut encoder = FrameEncoder::new(&cfg).unwrap();
        let mut decoder = FrameDecoder::new(&cfg).unwrap();
        let input = [4000; N];
        let mut payload = [0; MAX_OPUS_PACKET];
        let mut current = [0; N];
        let mut concealed = [0; N];
        let size = encoder.encode(&input, &mut payload).unwrap();
        decoder
            .decode(Some(&payload[..size]), &mut current)
            .unwrap();
        encoder.encode(&input, &mut payload).unwrap();
        let size = encoder.encode(&input, &mut payload).unwrap();
        assert!(
            decode_after_gap(
                &mut decoder,
                2,
                &payload[..size],
                &mut current,
                &mut concealed
            )
            .unwrap()
        );
        assert!(concealed.iter().any(|sample| *sample != 0));
        assert!(
            !decode_after_gap(
                &mut decoder,
                4,
                &payload[..size],
                &mut current,
                &mut concealed
            )
            .unwrap()
        );
    }

    #[test]
    fn recovered_audio_is_queued_atomically_and_in_order() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(PlaybackBatch::recovered(
            1,
            (Instant::now(), [8192; N]),
            (Instant::now(), [16384; N]),
        ))
        .unwrap();
        let mut render = playback(1, rx, state);
        let mut output = [0.0; N * 2];
        render(&mut output);
        assert!(output[..N].iter().all(|sample| *sample == 0.25));
        assert!(output[N..].iter().all(|sample| *sample == 0.5));

        let (tx, _rx) = mpsc::sync_channel(1);
        tx.send(PlaybackBatch::one(1, (Instant::now(), [0; N])))
            .unwrap();
        let error = tx
            .try_send(PlaybackBatch::recovered(
                1,
                (Instant::now(), [8192; N]),
                (Instant::now(), [16384; N]),
            ))
            .unwrap_err();
        let mpsc::TrySendError::Full(batch) = error else {
            panic!("recovery batch unexpectedly disconnected");
        };
        assert!(batch.second.is_some());
    }

    #[test]
    fn replacement_discards_partial_and_queued_audio_from_previous_peer() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(2);
        let mut render = playback(1, rx, state.clone());
        let mut sample = [0.0];
        state.remote_stream.store(1, Relaxed);
        tx.send(PlaybackBatch::one(1, (Instant::now(), [16384; N])))
            .unwrap();
        render(&mut sample);
        assert_eq!(sample, [0.5]);
        tx.send(PlaybackBatch::one(1, (Instant::now(), [16384; N])))
            .unwrap();
        state.remote_stream.store(2, Relaxed);
        tx.send(PlaybackBatch::one(2, (Instant::now(), [-16384; N])))
            .unwrap();
        render(&mut sample);
        assert_eq!(sample, [-0.5]);
        assert_eq!(state.dropped_playback.load(Relaxed), 2);
    }

    #[test]
    fn stereo_callback_spans_frames_and_pads_underrun_with_silence() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send(PlaybackBatch::one(1, (Instant::now(), [16384; N])))
            .unwrap();
        tx.send(PlaybackBatch::one(1, (Instant::now(), [-16384; N])))
            .unwrap();
        let mut render = playback(2, rx, state);
        let mut first = [0.0; 2 * (N + 3)];
        render(&mut first);
        assert!(first[..2 * N].iter().all(|v| *v == 0.5));
        assert!(first[2 * N..].iter().all(|v| *v == -0.5));
        let mut rest = [1.0; 2 * N];
        render(&mut rest);
        assert!(rest[..2 * (N - 3)].iter().all(|v| *v == -0.5));
        assert!(rest[2 * (N - 3)..].iter().all(|v| *v == 0.0));
    }
    #[test]
    fn mute_discards_partial_frame_before_resuming() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(2);
        let mut record = capture(2, tx, state.clone());
        record(&[1.0; N]);
        state.muted.store(true, Relaxed);
        record(&[1.0; N]);
        state.muted.store(false, Relaxed);
        record(&[-0.5; N]);
        assert!(rx.try_recv().is_err());
        record(&[-0.5; N]);
        assert!(rx.try_recv().unwrap().1.iter().all(|v| *v == -16383));
    }
    #[test]
    fn playback_expires_old_frames_and_deafen_discards_remainder() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(2);
        tx.send(PlaybackBatch::one(
            0,
            (Instant::now() - Duration::from_secs(1), [-1000; N]),
        ))
        .unwrap();
        tx.send(PlaybackBatch::one(0, (Instant::now(), [16384; N])))
            .unwrap();
        let mut render = playback(2, rx, state.clone());
        let mut pair = [0.0; 2];
        render(&mut pair);
        assert_eq!(pair, [0.5; 2]);
        assert_eq!(state.dropped_playback.load(Relaxed), 1);
        state.deafened.store(true, Relaxed);
        render(&mut pair);
        assert_eq!(pair, [0.0; 2]);
        state.deafened.store(false, Relaxed);
        render(&mut pair);
        assert_eq!(pair, [0.0; 2]);
    }
}
