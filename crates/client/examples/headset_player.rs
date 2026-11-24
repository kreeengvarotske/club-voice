//! Developer-only, two-seat loopback headset probe. Input/output at 48 kHz.
use club_voice_protocol::media::{HEADER, Header, MAX_DATAGRAM, MAX_PAYLOAD};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use opus_roundtrip_spike::{Config, FrameDecoder, FrameEncoder};
use std::{
    env,
    error::Error,
    io::{self, Write},
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
const PLAYBACK_BATCHES: usize = 2;
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
    playback_queue_full: AtomicU64,
    underrun_samples: AtomicU64,
    concealed: AtomicU64,
    plc_rendered_samples: AtomicU64,
    sent: AtomicU64,
    received: AtomicU64,
    rendered_real_frames: AtomicU64,
    device_xruns: AtomicU64,
    remote_stream: AtomicU64,
    streams_heard: AtomicU64,
}

fn config(
    formats: impl Iterator<Item = SupportedStreamConfigRange>,
) -> Result<SupportedStreamConfig> {
    formats
        .filter(|f| {
            f.channels() > 0
                && matches!(
                    f.sample_format(),
                    SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
                )
        })
        .filter_map(|f| f.try_with_sample_rate(48_000))
        .min_by_key(|f| f.sample_format() != SampleFormat::F32)
        .ok_or_else(|| "device needs a 48 kHz f32, i16 or u16 shared-mode format".into())
}

fn capture<T: Sample>(channels: usize, tx: SyncSender<Frame>, state: Arc<State>) -> impl FnMut(&[T])
where
    f32: FromSample<T>,
{
    let mut frame = [0; N];
    let mut pos = 0;
    move |mut samples| {
        if state.muted.load(Relaxed) {
            pos = 0;
            return;
        }
        while samples.len() >= channels {
            let count = (N - pos).min(samples.len() / channels);
            let (block, remaining) = samples.split_at(count * channels);
            let output = &mut frame[pos..pos + count];
            if channels == 1 {
                for (target, sample) in output.iter_mut().zip(block) {
                    *target = (sample.to_sample::<f32>() * 32767.0) as i16;
                }
            } else {
                for (target, sample) in output.iter_mut().zip(block.chunks_exact(channels)) {
                    let mono = sample
                        .iter()
                        .map(|sample| sample.to_sample::<f32>())
                        .sum::<f32>();
                    *target = (mono / channels as f32 * 32767.0) as i16;
                }
            }
            pos += count;
            samples = remaining;
            if pos == N {
                if tx.try_send((Instant::now(), frame)).is_err() {
                    state.dropped_capture.fetch_add(1, Relaxed);
                }
                pos = 0;
            }
        }
    }
}

#[cfg(test)]
fn playback<T: Sample + FromSample<i16>>(
    channels: usize,
    rx: Receiver<PlaybackBatch>,
    state: Arc<State>,
) -> impl FnMut(&mut [T]) {
    playback_with_observer(channels, rx, state, |_| {})
}

fn playback_with_observer<T: Sample + FromSample<i16>>(
    channels: usize,
    rx: Receiver<PlaybackBatch>,
    state: Arc<State>,
    mut on_real_frame: impl FnMut(Duration),
) -> impl FnMut(&mut [T]) {
    let mut current = None;
    let mut pending = None;
    let mut pos = 0;
    let mut silent_samples = 0;
    move |samples| {
        let now = Instant::now();
        let active = state.remote_stream.load(Relaxed);
        if state.deafened.load(Relaxed) {
            samples.fill(T::EQUILIBRIUM);
            current = None;
            pending = None;
            silent_samples = 0;
            while rx.try_recv().is_ok() {}
            return;
        }
        let aligned_len = samples.len() - samples.len() % channels;
        let (mut output, remainder) = samples.split_at_mut(aligned_len);
        remainder.fill(T::EQUILIBRIUM);
        while !output.is_empty() {
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
                    // Count mono sample periods, independent of output channel count.
                    let missing = output.len() / channels;
                    silent_samples = (silent_samples + missing.min(N)).min(N);
                    state.underrun_samples.fetch_add(missing as u64, Relaxed);
                    output.fill(T::EQUILIBRIUM);
                    return;
                }
            }
            if pos == 0 && pending.is_some() {
                // Those output periods already passed as silence during the gap.
                pos = std::mem::take(&mut silent_samples);
                if pos == N {
                    current = None;
                    continue;
                }
            }
            silent_samples = 0;
            let (_, (at, pcm)) = current.as_ref().unwrap();
            // A pending second frame marks the first one as synthetic PLC.
            if pos == 0 && pending.is_none() {
                on_real_frame(now.saturating_duration_since(*at));
            }
            let count = (N - pos).min(output.len() / channels);
            if pending.is_some() {
                state.plc_rendered_samples.fetch_add(count as u64, Relaxed);
            }
            let (block, rest) = output.split_at_mut(count * channels);
            for (channels_out, sample) in
                block.chunks_exact_mut(channels).zip(&pcm[pos..pos + count])
            {
                channels_out.fill(T::from_sample(*sample));
            }
            output = rest;
            pos += count;
            if pos == N {
                current = None;
            }
        }
    }
}

fn transmit(socket: &UdpSocket, rx: Receiver<Frame>, state: &State, stream: u64) -> Result<()> {
    let mut encoder = FrameEncoder::new(&Config::default())?;
    let mut packet = [0; MAX_DATAGRAM];
    let mut seq = 0u32;
    let mut heartbeat = Instant::now();
    while state.running.load(Relaxed) {
        let elapsed = heartbeat.elapsed();
        let wait = if elapsed >= HEARTBEAT {
            socket.send(&club_voice_protocol::media::SessionCommand::Ping.packet(stream))?;
            heartbeat = Instant::now();
            HEARTBEAT
        } else {
            HEARTBEAT - elapsed
        };
        match rx.recv_timeout(wait) {
            Ok(_) if !state.running.load(Relaxed) => break,
            Ok((at, pcm)) if at.elapsed() <= MAX_AGE && !state.muted.load(Relaxed) => {
                let payload = packet.last_chunk_mut::<MAX_PAYLOAD>().unwrap();
                let size = encoder.encode(&pcm, payload)?;
                let sent_us = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;
                Header {
                    stream,
                    frame_ms: 20,
                    sequence: seq,
                    sent_us,
                }
                .write(&mut packet);
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
    probe_drop_sequence: Option<u32>,
) -> Result<()> {
    receive_with_observer(socket, tx, state, probe_drop_sequence, |_| {})
}

fn receive_with_observer(
    socket: &UdpSocket,
    tx: SyncSender<PlaybackBatch>,
    state: &State,
    mut probe_drop_sequence: Option<u32>,
    mut on_received_frame: impl FnMut(Duration),
) -> Result<()> {
    let mut decoder = FrameDecoder::new(&Config::default())?;
    let same_host_clock = socket.peer_addr()?.ip().is_loopback();
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
        let age = if same_host_clock {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
            let Some(age) = now
                .checked_sub(header.sent_us.into())
                .filter(|age| *age <= MAX_AGE.as_micros())
            else {
                state.dropped_playback.fetch_add(1, Relaxed);
                continue;
            };
            Some(Duration::from_micros(age as u64))
        } else {
            None
        };
        let at = Instant::now() - age.unwrap_or_default();
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
        if let Err(error) = tx.try_send(batch) {
            if matches!(error, mpsc::TrySendError::Full(_)) {
                state.playback_queue_full.fetch_add(1, Relaxed);
            }
            state.dropped_playback.fetch_add(1, Relaxed);
        }
        if let Some(age) = age {
            on_received_frame(age);
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
    if args == ["--check-devices"] {
        return check_devices();
    }
    if args == ["--check-lan"] {
        return check_lan();
    }
    if args == ["--join-lan"] || args == ["--join-lan", "--tone-peer"] {
        return join_lan(args.len() == 2);
    }
    let [address, name, consent] = args.as_slice() else {
        return Err("Usage: headset_player --check-devices or headset_player 127.0.0.1:PORT NAME --enable-microphone".into());
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

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input closed"));
    }
    Ok(answer.trim().to_owned())
}

fn prompt_lan_address() -> Result<SocketAddrV4> {
    let port = club_voice_protocol::LAN_PORT;
    let input = prompt(&format!(
        "Server address (IPv4 or IPv4:port, default {port})"
    ))?;
    Ok(club_voice_protocol::parse_lan_address(&input)?)
}

fn connect_lan(address: SocketAddrV4) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.connect(address)?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    Ok(socket)
}

fn join_lan(tone: bool) -> Result<()> {
    let address = prompt_lan_address()?;
    let name = prompt("Display name")?;
    let password = prompt("Room password")?;
    if !tone && !prompt("Enable microphone? [y/N]")?.eq_ignore_ascii_case("y") {
        return Ok(());
    }
    let socket = connect_lan(address)?;
    let stream = club_voice_client::join_lan_voice(&socket, &name, &password)?;
    let result = if tone {
        tone_peer(&socket, stream)
    } else {
        session(&socket, stream)
    };
    let _ = socket.send(&club_voice_protocol::media::SessionCommand::Leave.packet(stream));
    result.map_err(|e| {
        if e.downcast_ref::<io::Error>().is_some_and(|e| {
            matches!(e.kind(), io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionRefused)
        }) {
            format!("Lost connection to voice server at {address}. Restart or check the server, then rejoin.").into()
        } else {
            e
        }
    })
}

fn check_lan() -> Result<()> {
    let address = prompt_lan_address()?;
    let password = prompt("Room password")?;
    let devices = check_devices();
    match &devices {
        Ok(()) => println!("PASS: device formats"),
        Err(e) => eprintln!("FAIL: device formats: {e}"),
    }

    let socket = connect_lan(address)?;
    let wrong_password = if password == "invalid-check-password" {
        "another-invalid-password"
    } else {
        "invalid-check-password"
    };
    match club_voice_client::join_lan_voice(&socket, "LAN check", wrong_password) {
        Err(e) if e.to_string() == "join failed: DENIED" => {
            println!("PASS: wrong password rejected")
        }
        Err(e) => return Err(format!("password rejection check failed: {e}").into()),
        Ok(stream) => {
            let _ = socket.send(&club_voice_protocol::media::SessionCommand::Leave.packet(stream));
            return Err("server accepted a wrong password".into());
        }
    }

    let mut previous = None;
    for step in ["initial connection", "rejoin"] {
        let stream = club_voice_client::join_lan_voice(&socket, "LAN check", &password)?;
        if previous == Some(stream) {
            return Err("rejoin did not start a new voice session".into());
        }
        let audio = tone_peer(&socket, stream);
        let left = leave_lan_confirmed(&socket, stream);
        audio?;
        left?;
        println!("PASS: {step}, synthetic audio received and leave acknowledged");
        previous = Some(stream);
    }
    devices?;
    println!("PASS: LAN check complete");
    Ok(())
}

fn leave_lan_confirmed(socket: &UdpSocket, stream: u64) -> Result<()> {
    socket.send(&club_voice_protocol::media::SessionCommand::Leave.packet(stream))?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut packet = [0; MAX_DATAGRAM + 1];
    while Instant::now() < deadline {
        match socket.recv(&mut packet) {
            Ok(size) if &packet[..size] == b"LEFT" => {
                socket.set_read_timeout(Some(Duration::from_secs(2)))?;
                return Ok(());
            }
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err("server did not confirm departure".into())
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

fn check_devices() -> Result<()> {
    let host = cpal::default_host();
    for (label, input) in [("Input", true), ("Output", false)] {
        let device = endpoint(&host, input)?;
        let selected = if input {
            config(device.supported_input_configs()?)
        } else {
            config(device.supported_output_configs()?)
        }
        .map_err(|error| format!("{label} {device}: {error}"))?;
        println!(
            "{label}: {device}, {} Hz, {} channel(s), {:?}",
            selected.sample_rate(),
            selected.channels(),
            selected.sample_format()
        );
    }
    Ok(())
}

fn build_capture<T: SizedSample + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    tx: SyncSender<Frame>,
    state: Arc<State>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream>
where
    f32: FromSample<T>,
{
    let mut record = capture::<T>(config.channels.into(), tx, state);
    Ok(device.build_input_stream(
        config,
        move |samples, _| record(samples),
        on_error,
        Some(Duration::from_secs(2)),
    )?)
}

fn build_playback<T: SizedSample + FromSample<i16> + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    rx: Receiver<PlaybackBatch>,
    state: Arc<State>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream> {
    let counters = state.clone();
    let mut render = playback_with_observer::<T>(config.channels.into(), rx, state, move |_| {
        counters.rendered_real_frames.fetch_add(1, Relaxed);
    });
    Ok(device.build_output_stream(
        config,
        move |samples, _| render(samples),
        on_error,
        Some(Duration::from_secs(2)),
    )?)
}

fn audio_error_handler(state: Arc<State>, stop: SyncSender<Result<()>>) -> impl FnMut(cpal::Error) {
    move |error| {
        // WASAPI continues delivering buffers after reporting a discontinuity.
        if cfg!(windows) && error.kind() == cpal::ErrorKind::Xrun {
            state.device_xruns.fetch_add(1, Relaxed);
        } else {
            let _ = stop.try_send(Err(error.into()));
        }
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
    let (play_tx, play_rx) = mpsc::sync_channel(PLAYBACK_BATCHES);
    let (stop, done) = mpsc::sync_channel::<Result<()>>(1);
    let on_error = || audio_error_handler(state.clone(), stop.clone());
    let microphone = match i.sample_format() {
        SampleFormat::F32 => {
            build_capture::<f32>(&input, i.config(), cap_tx, state.clone(), on_error())
        }
        SampleFormat::I16 => {
            build_capture::<i16>(&input, i.config(), cap_tx, state.clone(), on_error())
        }
        SampleFormat::U16 => {
            build_capture::<u16>(&input, i.config(), cap_tx, state.clone(), on_error())
        }
        _ => return Err("unsupported capture sample format".into()),
    }?;
    let headphones = match o.sample_format() {
        SampleFormat::F32 => {
            build_playback::<f32>(&output, o.config(), play_rx, state.clone(), on_error())
        }
        SampleFormat::I16 => {
            build_playback::<i16>(&output, o.config(), play_rx, state.clone(), on_error())
        }
        SampleFormat::U16 => {
            build_playback::<u16>(&output, o.config(), play_rx, state.clone(), on_error())
        }
        _ => return Err("unsupported playback sample format".into()),
    }?;
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
        // Closing the capture channel wakes the sender even while muted.
        drop(microphone);
        result
    })?;
    drop(headphones);
    println!(
        "sent={} decoded={} rendered_real_frames={} capture_drops={} playback_drops={} concealed={} plc_rendered_samples={} playback_queue_full={} underrun_samples={} device_xruns={}",
        state.sent.load(Relaxed),
        state.received.load(Relaxed),
        state.rendered_real_frames.load(Relaxed),
        state.dropped_capture.load(Relaxed),
        state.dropped_playback.load(Relaxed),
        state.concealed.load(Relaxed),
        state.plc_rendered_samples.load(Relaxed),
        state.playback_queue_full.load(Relaxed),
        state.underrun_samples.load(Relaxed),
        state.device_xruns.load(Relaxed)
    );
    result
}

const PROBE_PERIOD: Duration = Duration::from_millis(20);

struct ProbeLatency([u32; MAX_AGE.as_millis() as usize + 1]);

impl ProbeLatency {
    fn new() -> Self {
        Self([0; MAX_AGE.as_millis() as usize + 1])
    }

    fn record(&mut self, age: Duration) {
        let bucket = age.as_nanos().div_ceil(1_000_000) as usize;
        self.0[bucket] += 1;
    }

    fn p95(&self) -> Option<(u32, usize)> {
        let frames: u32 = self.0.iter().sum();
        if frames == 0 {
            return None;
        }
        let rank = (frames * 95).div_ceil(100);
        let mut cumulative = 0;
        let bucket = self.0.iter().position(|count| {
            cumulative += count;
            cumulative >= rank
        })?;
        Some((frames, bucket))
    }
}

fn next_probe_tick(due: Instant, now: Instant) -> (Instant, u64) {
    let missed = now.saturating_duration_since(due).as_millis() / PROBE_PERIOD.as_millis();
    if missed > 0 {
        (now + PROBE_PERIOD, missed as u64)
    } else {
        (due + PROBE_PERIOD, 0)
    }
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
    let (play_tx, play_rx) = mpsc::sync_channel(PLAYBACK_BATCHES);
    let mut record = capture(1, cap_tx, state.clone());
    let mut received_ages = ProbeLatency::new();
    let mut rendered_ages = ProbeLatency::new();
    let mut render = playback_with_observer(2, play_rx, state.clone(), |age| {
        rendered_ages.record(age);
    });
    let input: [f32; N] =
        std::array::from_fn(|i| (std::f32::consts::TAU * 500.0 * i as f32 / 48000.0).sin() * 0.15);
    let mut output = [0.0f32; N * 2];
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    state.running.store(true, Relaxed);
    println!("READY");
    let mut audible = 0;
    let mut probe_missed_ticks = 0;
    thread::scope(|scope| -> Result<()> {
        let sender = scope.spawn(|| transmit(socket, cap_rx, &state, stream));
        let receiver = scope.spawn(|| {
            receive_with_observer(socket, play_tx, &state, probe_drop_sequence, |age| {
                received_ages.record(age);
            })
        });
        let mut due = Instant::now();
        let end = due + Duration::from_secs(seconds);
        loop {
            let now = Instant::now();
            if now >= end || receiver.is_finished() || sender.is_finished() {
                break;
            }
            if now < due {
                thread::sleep(due.min(end).duration_since(now));
                continue;
            }
            let (next, missed) = next_probe_tick(due, now);
            due = next;
            probe_missed_ticks += missed;
            record(&input);
            render(&mut output);
            if output.iter().any(|s| s.abs() > 0.02) {
                audible += 1;
            }
        }
        state.running.store(false, Relaxed);
        drop(record);
        sender.join().expect("sender panicked")?;
        receiver.join().expect("receiver panicked")?;
        Ok(())
    })?;
    println!(
        "sent={} decoded={} concealed={} audible_blocks={} streams_heard={} probe_missed_ticks={probe_missed_ticks} plc_rendered_samples={} playback_queue_full={} underrun_samples={}",
        state.sent.load(Relaxed),
        state.received.load(Relaxed),
        state.concealed.load(Relaxed),
        audible,
        state.streams_heard.load(Relaxed),
        state.plc_rendered_samples.load(Relaxed),
        state.playback_queue_full.load(Relaxed),
        state.underrun_samples.load(Relaxed)
    );
    if audible < 50 {
        return Err("expected at least one second of received signal".into());
    }
    drop(render);
    let (frames, render_p95) = rendered_ages.p95().ok_or("no real frames rendered")?;
    if !socket.peer_addr()?.ip().is_loopback() {
        println!("rendered_real_frames={frames} arrival_to_render_p95_ms={render_p95}");
        return Ok(());
    }
    let (_, receive_p95) = received_ages.p95().ok_or("no real frames received")?;
    println!(
        "rendered_real_frames={frames} send_to_render_p95_ms={render_p95} send_to_receive_p95_ms={receive_p95}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opus_roundtrip_spike::MAX_OPUS_PACKET;

    #[cfg(windows)]
    #[test]
    fn wasapi_discontinuity_counts_without_stopping_but_device_loss_stops() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(1);
        let mut on_error = audio_error_handler(state.clone(), tx);
        on_error(cpal::ErrorKind::Xrun.into());
        assert_eq!(state.device_xruns.load(Relaxed), 1);
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        on_error(cpal::ErrorKind::DeviceNotAvailable.into());
        assert!(rx.try_recv().unwrap().is_err());
    }

    #[test]
    fn selects_integer_fallback_at_48_khz_and_prefers_float() {
        let format = |rate, sample_format| {
            SupportedStreamConfigRange::new(
                2,
                rate,
                rate,
                cpal::SupportedBufferSize::Unknown,
                sample_format,
            )
        };
        let integer = format(48_000, SampleFormat::I16);
        let float_44 = format(44_100, SampleFormat::F32);
        assert_eq!(
            config([float_44, integer].into_iter())
                .unwrap()
                .sample_format(),
            SampleFormat::I16
        );
        let float_48 = format(48_000, SampleFormat::F32);
        assert_eq!(
            config([integer, float_48].into_iter())
                .unwrap()
                .sample_format(),
            SampleFormat::F32
        );
        assert!(config([float_44].into_iter()).is_err());
    }

    #[test]
    fn integer_callbacks_convert_pcm_and_emit_unsigned_silence() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(1);
        let mut record = capture::<i16>(2, tx, state.clone());
        let stereo: [i16; 2 * N] = std::array::from_fn(|i| if i % 2 == 0 { 16384 } else { 0 });
        record(&stereo);
        let frame = rx.try_recv().unwrap();
        assert!(frame.1.iter().all(|&sample| sample == 8191));

        let (tx, rx) = mpsc::sync_channel(1);
        tx.try_send(PlaybackBatch::one(1, frame)).unwrap();
        let mut render = playback::<u16>(2, rx, state.clone());
        let mut out = [0; 2 * N + 3];
        render(&mut out);
        assert!(out[..2 * N].iter().all(|&sample| sample == 40959));
        assert_eq!(out[2 * N..], [32768; 3]);
        state.deafened.store(true, Relaxed);
        render(&mut out);
        assert!(out.iter().all(|&sample| sample == 32768));
    }

    #[test]
    fn probe_latency_rounds_up_and_uses_the_nearest_rank() {
        let mut latency = ProbeLatency::new();
        assert_eq!(latency.p95(), None);
        for _ in 0..19 {
            latency.record(Duration::from_micros(1));
        }
        latency.record(MAX_AGE);
        assert_eq!(latency.p95(), Some((20, 1)));
    }
    #[test]
    fn idle_sender_heartbeats_and_stops_when_capture_closes() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        server.set_read_timeout(Some(HEARTBEAT * 2)).unwrap();
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket.connect(server.local_addr().unwrap()).unwrap();
        let state = State::default();
        state.running.store(true, Relaxed);
        state.muted.store(true, Relaxed);
        thread::scope(|scope| {
            let (capture, frames) = mpsc::sync_channel(2);
            let (finished, done) = mpsc::channel();
            let (socket, state) = (&socket, &state);
            scope.spawn(move || {
                finished.send(transmit(socket, frames, state, 1)).unwrap();
            });
            let mut packet = [0; 32];
            let (len, _) = server.recv_from(&mut packet).unwrap();
            assert_eq!(
                &packet[..len],
                &club_voice_protocol::media::SessionCommand::Ping.packet(1)
            );
            drop(capture);
            done.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        });
    }

    #[test]
    fn probe_skips_elapsed_ticks_after_a_scheduler_stall() {
        let due = Instant::now();
        let slightly_late = due + Duration::from_millis(3);
        assert_eq!(next_probe_tick(due, slightly_late), (due + PROBE_PERIOD, 0));
        let stalled = due + Duration::from_millis(65);
        assert_eq!(next_probe_tick(due, stalled), (stalled + PROBE_PERIOD, 3));
    }

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
        let mut observed = 0;
        let mut render = playback_with_observer(1, rx, state, |_| observed += 1);
        let mut output = [0.0; N * 2];
        let (start, rest) = output.split_at_mut(N + 1);
        render(start);
        render(rest);
        drop(render);
        assert_eq!(observed, 1);
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
    fn concealment_only_fills_the_part_of_the_gap_not_already_rendered() {
        for silence in [N / 2, N] {
            let state = Arc::new(State::default());
            let (tx, rx) = mpsc::sync_channel(PLAYBACK_BATCHES);
            let mut render = playback(1, rx, state.clone());
            let mut gap = [1.0; N];
            render(&mut gap[..silence]);
            assert!(gap[..silence].iter().all(|sample| *sample == 0.0));

            tx.try_send(PlaybackBatch::recovered(
                1,
                (Instant::now(), [8192; N]),
                (Instant::now(), [16384; N]),
            ))
            .unwrap();
            let mut output = [0.0; 2 * N];
            render(&mut output[..2 * N - silence]);
            assert!(output[..N - silence].iter().all(|sample| *sample == 0.25));
            assert!(
                output[N - silence..2 * N - silence]
                    .iter()
                    .all(|sample| *sample == 0.5)
            );
            assert_eq!(state.underrun_samples.load(Relaxed), silence as u64);
            assert_eq!(
                state.plc_rendered_samples.load(Relaxed),
                (N - silence) as u64
            );
        }
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
        let (tx, rx) = mpsc::sync_channel(PLAYBACK_BATCHES);
        tx.try_send(PlaybackBatch::one(1, (Instant::now(), [16384; N])))
            .unwrap();
        tx.try_send(PlaybackBatch::one(1, (Instant::now(), [-16384; N])))
            .unwrap();
        let mut render = playback(2, rx, state.clone());
        let mut first = [1.0; 2 * (N + 3) + 1];
        render(&mut first);
        assert!(first[..2 * N].iter().all(|v| *v == 0.5));
        assert!(first[2 * N..2 * (N + 3)].iter().all(|v| *v == -0.5));
        assert_eq!(first[2 * (N + 3)], 0.0);
        let mut rest = [1.0; 2 * N];
        render(&mut rest);
        assert!(rest[..2 * (N - 3)].iter().all(|v| *v == -0.5));
        assert!(rest[2 * (N - 3)..].iter().all(|v| *v == 0.0));
        assert_eq!(state.underrun_samples.load(Relaxed), 3);
        state.deafened.store(true, Relaxed);
        render(&mut rest);
        assert_eq!(state.underrun_samples.load(Relaxed), 3);
    }
    #[test]
    fn mono_capture_preserves_frames_across_callback_boundaries() {
        let state = Arc::new(State::default());
        let (tx, rx) = mpsc::sync_channel(2);
        let mut record = capture(1, tx, state);
        record(&[0.5; N / 2]);
        assert!(rx.try_recv().is_err());
        record(&[-0.5; N + N / 2]);
        let (_, first) = rx.try_recv().unwrap();
        let (_, second) = rx.try_recv().unwrap();
        assert!(first[..N / 2].iter().all(|&sample| sample == 16383));
        assert!(first[N / 2..].iter().all(|&sample| sample == -16383));
        assert!(second.iter().all(|&sample| sample == -16383));
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
