//! One endpoint of the two-process synthetic Opus probe.
use club_voice_protocol::media::{HEADER, Header, MAX_DATAGRAM, MAX_PAYLOAD};
use opus_roundtrip_spike::{
    Config, FrameDecoder, FrameEncoder, SyntheticSource, TONES_HZ, goertzel_power,
};
use std::{
    env,
    error::Error,
    io::{self, Write},
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const RUN_MS: u32 = 2000;

fn clock_us() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_micros()
        .try_into()?)
}

fn send(socket: &UdpSocket, cfg: &Config, stream: u64) -> Result<()> {
    let mut source = SyntheticSource::new(cfg)?;
    let mut encoder = FrameEncoder::new(cfg)?;
    let mut pcm = vec![0; cfg.frame_samples()];
    let mut packet = [0; MAX_DATAGRAM];
    let start = Instant::now();
    for sequence in 0..RUN_MS / cfg.frame_ms {
        let due = start + Duration::from_millis(u64::from(sequence * cfg.frame_ms));
        thread::sleep(due.saturating_duration_since(Instant::now()));
        source.next_frame(&mut pcm);
        let payload = packet.last_chunk_mut::<MAX_PAYLOAD>().unwrap();
        let size = encoder.encode(&pcm, payload)?;
        Header {
            stream,
            frame_ms: cfg.frame_ms as u8,
            sequence,
            sent_us: clock_us()?,
        }
        .write(&mut packet);
        socket.send(&packet[..HEADER + size])?;
    }
    Ok(())
}

fn recv_probe_packet(socket: &UdpSocket, buffer: &mut [u8], stream: u64) -> Result<usize> {
    let mut acknowledgement = *b"CV2A00000000";
    acknowledgement[4..].copy_from_slice(&stream.to_be_bytes());
    for _ in 0..3 {
        let size = socket.recv(buffer)?;
        if buffer[..size] != acknowledgement {
            return Ok(size);
        }
    }
    Err("too many duplicate admission replies".into())
}

fn receive(socket: &UdpSocket, cfg: &Config, stream: u64) -> Result<()> {
    let mut decoder = FrameDecoder::new(cfg)?;
    let mut packet = [0; MAX_DATAGRAM + 1];
    let mut frame = vec![0; cfg.frame_samples()];
    let count = RUN_MS / cfg.frame_ms;
    let mut signal = Vec::with_capacity(count as usize * frame.len());
    let mut delays = Vec::with_capacity(count as usize);
    let mut total_bytes = 0;
    for expected in 0..count {
        let size = recv_probe_packet(socket, &mut packet, stream)?;
        let (header, payload) = Header::parse(&packet[..size]).ok_or("invalid relay packet")?;
        if header.sequence != expected || u32::from(header.frame_ms) != cfg.frame_ms {
            return Err(format!("unexpected frame {header:?}, expected {expected}").into());
        }
        decoder.decode(Some(payload), &mut frame)?;
        delays.push(
            clock_us()?
                .checked_sub(header.sent_us)
                .ok_or("host clock stepped backwards")?,
        );
        total_bytes += payload.len();
        signal.extend_from_slice(&frame);
    }
    // Compare recovered tones to the generated reference, excluding codec startup.
    let tail = &signal[signal.len() - 4800..];
    let mut source = SyntheticSource::new(cfg)?;
    let mut reference = Vec::with_capacity(4800);
    while reference.len() < 4800 {
        source.next_frame(&mut frame);
        reference.extend_from_slice(&frame);
    }
    for hz in TONES_HZ {
        let gain_db = 10.0
            * (goertzel_power(tail, hz, 48000) / goertzel_power(&reference, hz, 48000)).log10();
        if !(-2.0..=2.0).contains(&gain_db) {
            return Err(format!("{hz} Hz gain {gain_db} dB").into());
        }
    }
    delays.sort_unstable();
    println!(
        "frame_ms={} packets={} samples={} payload_bytes={} send_to_decode_p50_us={} p95_us={}",
        cfg.frame_ms,
        count,
        signal.len(),
        total_bytes,
        delays[delays.len().div_ceil(2) - 1],
        delays[(delays.len() * 95).div_ceil(100) - 1]
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let [address, role, frame_ms] = args.as_slice() else {
        return Err("expected ADDRESS send|receive 10|20".into());
    };
    let address: SocketAddrV4 = address.parse()?;
    let cfg = Config {
        frame_ms: frame_ms.parse()?,
        ..Config::default()
    };
    if !address.ip().is_loopback()
        || !matches!(role.as_str(), "send" | "receive")
        || !matches!(cfg.frame_ms, 10 | 20)
    {
        return Err("invalid probe arguments".into());
    }
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    socket.connect(address)?;
    socket.set_read_timeout(Some(Duration::from_secs(4)))?;
    let mut reply = [0; 32];
    let stream = club_voice_client::join_loopback_voice(&socket, role)?;
    println!("READY");
    io::stdout().flush()?;
    if role == "send" {
        send(&socket, &cfg, stream)?;
    } else {
        receive(&socket, &cfg, stream)?;
    }
    socket.send(&club_voice_protocol::media::SessionCommand::Leave.packet(stream))?;
    let size = recv_probe_packet(&socket, &mut reply, stream)?;
    if &reply[..size] != b"LEFT" {
        return Err("leave failed".into());
    }
    Ok(())
}
