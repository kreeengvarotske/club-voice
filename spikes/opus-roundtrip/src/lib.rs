//! Opus encode/decode round-trip on deterministic generated PCM.
//!
//! This is a bounded technical spike for the voice path (Issue #1). It does
//! not define a wire format and does not touch devices, networking or the
//! server. It answers: with the bundled libopus, which parameters work, what
//! does a packet cost, what delay does the codec add, and does the decoded
//! signal still carry the input content.

use std::f64::consts::PI;
use std::time::{Duration, Instant};

pub use opus::{Application, Channels};

/// Largest legal Opus packet payload (RFC 6716, section 3.2.1).
pub const MAX_OPUS_PACKET: usize = 1275;

/// Test tones mixed into the generated signal (Hz). Chosen inside the
/// wideband voice range and away from each other's harmonics.
pub const TONES_HZ: [f64; 3] = [300.0, 1000.0, 2500.0];

/// Round-trip parameters. Defaults are the proposal for room voice.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub sample_rate: u32,
    pub frame_ms: u32,
    pub bitrate_bps: i32,
    pub application: Application,
    /// Length of the tone segment in milliseconds.
    pub tone_ms: u32,
    /// Length of the trailing digital-silence segment in milliseconds.
    pub silence_ms: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sample_rate: 48_000,
            frame_ms: 20,
            bitrate_bps: 32_000,
            application: Application::Voip,
            tone_ms: 2_000,
            silence_ms: 500,
        }
    }
}

impl Config {
    pub fn frame_samples(&self) -> usize {
        (self.sample_rate as usize * self.frame_ms as usize) / 1000
    }

    fn samples_for_ms(&self, ms: u32) -> usize {
        (self.sample_rate as usize * ms as usize) / 1000
    }

    /// Total generated samples, rounded up to whole frames.
    pub fn total_samples(&self) -> usize {
        let raw = self.samples_for_ms(self.tone_ms) + self.samples_for_ms(self.silence_ms);
        let f = self.frame_samples();
        raw.div_ceil(f) * f
    }

    pub fn tone_samples(&self) -> usize {
        self.samples_for_ms(self.tone_ms)
    }
}

/// Deterministic mono test signal: three equal tones (each at -14 dBFS peak,
/// sum peak about -4.5 dBFS) with 10 ms raised-cosine fades, followed by
/// digital silence, padded with silence to a whole number of frames.
pub fn generate_pcm(cfg: &Config) -> Vec<i16> {
    let total = cfg.total_samples();
    let tone_len = cfg.tone_samples();
    let fade = cfg.samples_for_ms(10).max(1);
    let amp = 0.2_f64; // per tone, in full-scale units
    let sr = cfg.sample_rate as f64;
    let mut out = vec![0i16; total];
    for (n, slot) in out.iter_mut().enumerate().take(tone_len) {
        let t = n as f64 / sr;
        let mut s: f64 = TONES_HZ.iter().map(|f| (2.0 * PI * f * t).sin()).sum();
        s *= amp;
        let env = if n < fade {
            0.5 - 0.5 * (PI * n as f64 / fade as f64).cos()
        } else if n >= tone_len - fade {
            0.5 - 0.5 * (PI * (tone_len - n) as f64 / fade as f64).cos()
        } else {
            1.0
        };
        *slot = (s * env * i16::MAX as f64).round() as i16;
    }
    out
}

/// Deterministic linear chirp, 200 Hz to 4 kHz over `len` samples at -6 dBFS,
/// with 10 ms fades. Unlike the tone mix (which repeats every 10 ms) it has a
/// unique cross-correlation peak, so it is used to measure codec delay.
pub fn generate_chirp(cfg: &Config, len: usize) -> Vec<i16> {
    let sr = cfg.sample_rate as f64;
    let (f0, f1) = (200.0_f64, 4000.0_f64);
    let dur = len as f64 / sr;
    let fade = cfg.samples_for_ms(10).max(1);
    let f = cfg.frame_samples();
    let mut out = vec![0i16; len.div_ceil(f) * f + 4 * f];
    for (n, slot) in out.iter_mut().enumerate().take(len) {
        let t = n as f64 / sr;
        let phase = 2.0 * PI * (f0 * t + 0.5 * (f1 - f0) / dur * t * t);
        let env = if n < fade {
            0.5 - 0.5 * (PI * n as f64 / fade as f64).cos()
        } else if n >= len - fade {
            0.5 - 0.5 * (PI * (len - n) as f64 / fade as f64).cos()
        } else {
            1.0
        };
        *slot = (0.5 * phase.sin() * env * i16::MAX as f64).round() as i16;
    }
    out
}

/// Encode and decode a chirp with a fresh encoder/decoder and return the lag
/// (samples) at which the decoded output best matches the input.
pub fn measure_codec_delay(cfg: &Config, max_lag: usize) -> Result<usize, SpikeError> {
    let len = cfg.sample_rate as usize / 2;
    let chirp = generate_chirp(cfg, len);
    let packets = encode_all(cfg, &chirp)?;
    let f = cfg.frame_samples();
    let mut dec = opus::Decoder::new(cfg.sample_rate, Channels::Mono)?;
    let mut frame = vec![0i16; f];
    let mut out = Vec::with_capacity(chirp.len());
    for p in &packets {
        let n = dec.decode(p, &mut frame, false)?;
        out.extend_from_slice(&frame[..n]);
    }
    Ok(estimate_delay(&chirp, &out, 0..len, max_lag))
}

/// Measurements from one round trip.
#[derive(Clone, Debug)]
pub struct Report {
    pub libopus_version: String,
    pub config: Config,
    pub frame_samples: usize,
    pub packets: usize,
    pub decoded_samples: usize,
    pub min_packet_bytes: usize,
    pub max_packet_bytes: usize,
    pub mean_packet_bytes: f64,
    /// Packet sizes during the trailing silence (DTX is off, so packets still flow).
    pub max_silence_packet_bytes: usize,
    /// Average payload bitrate over the tone segment, bits per second.
    pub tone_payload_bps: f64,
    pub encoder_lookahead_samples: usize,
    /// Delay of the decoded signal against the input, from cross-correlation
    /// on a separate chirp probe (see [`measure_codec_delay`]).
    pub measured_delay_samples: usize,
    /// Signal-to-noise ratio over the steady tone segment after delay alignment.
    pub snr_db: f64,
    /// Output/input power per test tone, in dB (0 = preserved exactly).
    pub tone_gain_db: Vec<(f64, f64)>,
    /// Peak absolute sample in the decoded steady-silence region.
    pub silence_peak: i16,
    /// Every decode call returned exactly one frame of samples.
    pub all_frames_full: bool,
    /// Packet-loss concealment: decoding an empty packet produced a full frame.
    pub plc_frame_samples: usize,
    /// Two fresh encoders produced identical packets for the same input.
    pub deterministic: bool,
    /// FNV-1a over all packet bytes; compare across machines/builds.
    pub packet_hash: u64,
    pub encode_time: Duration,
    pub decode_time: Duration,
}

#[derive(Debug)]
pub enum SpikeError {
    Opus(opus::Error),
    Invalid(String),
}

impl std::fmt::Display for SpikeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpikeError::Opus(e) => write!(f, "libopus: {e}"),
            SpikeError::Invalid(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for SpikeError {}

impl From<opus::Error> for SpikeError {
    fn from(e: opus::Error) -> Self {
        SpikeError::Opus(e)
    }
}

fn new_encoder(cfg: &Config) -> Result<opus::Encoder, SpikeError> {
    let mut enc = opus::Encoder::new(cfg.sample_rate, Channels::Mono, cfg.application)?;
    enc.set_bitrate(opus::Bitrate::Bits(cfg.bitrate_bps))?;
    Ok(enc)
}

/// Encode `pcm` frame by frame. `pcm.len()` must be a multiple of the frame size.
pub fn encode_all(cfg: &Config, pcm: &[i16]) -> Result<Vec<Vec<u8>>, SpikeError> {
    let f = cfg.frame_samples();
    if f == 0 || !pcm.len().is_multiple_of(f) {
        return Err(SpikeError::Invalid(format!(
            "pcm length {} is not a multiple of frame size {f}",
            pcm.len()
        )));
    }
    let mut enc = new_encoder(cfg)?;
    let mut buf = [0u8; MAX_OPUS_PACKET];
    let mut packets = Vec::with_capacity(pcm.len() / f);
    for frame in pcm.chunks_exact(f) {
        let n = enc.encode(frame, &mut buf)?;
        packets.push(buf[..n].to_vec());
    }
    Ok(packets)
}

/// Run the full round trip and measure it.
pub fn run(cfg: &Config) -> Result<Report, SpikeError> {
    let f = cfg.frame_samples();
    // libopus accepts 2.5/5/10/20/40/60 ms; this spike only uses the ones that
    // map to whole sample counts at the configured rate.
    if !matches!(cfg.frame_ms, 10 | 20 | 40 | 60) {
        return Err(SpikeError::Invalid(format!(
            "frame_ms {} not one of 10, 20, 40, 60",
            cfg.frame_ms
        )));
    }
    let pcm = generate_pcm(cfg);

    let lookahead = new_encoder(cfg)?.get_lookahead()?;
    let lookahead = usize::try_from(lookahead)
        .map_err(|_| SpikeError::Invalid(format!("negative lookahead {lookahead}")))?;

    let t0 = Instant::now();
    let packets = encode_all(cfg, &pcm)?;
    let encode_time = t0.elapsed();

    let deterministic = encode_all(cfg, &pcm)? == packets;

    let mut dec = opus::Decoder::new(cfg.sample_rate, Channels::Mono)?;
    let mut out = Vec::with_capacity(pcm.len());
    let mut frame = vec![0i16; f];
    let mut all_frames_full = true;
    let t1 = Instant::now();
    for p in &packets {
        let n = dec.decode(p, &mut frame, false)?;
        if n != f || dec.get_nb_samples(p)? != f {
            all_frames_full = false;
        }
        out.extend_from_slice(&frame[..n]);
    }
    let decode_time = t1.elapsed();

    // Packet-loss concealment on a separate decoder primed with the first packets.
    let plc_frame_samples = {
        let mut d = opus::Decoder::new(cfg.sample_rate, Channels::Mono)?;
        let mut fr = vec![0i16; f];
        for p in packets.iter().take(5) {
            d.decode(p, &mut fr, false)?;
        }
        d.decode(&[], &mut fr, false)?
    };

    let sizes: Vec<usize> = packets.iter().map(Vec::len).collect();
    let tone_frames = cfg.tone_samples() / f;
    let tone_bytes: usize = sizes[..tone_frames].iter().sum();
    let tone_secs = (tone_frames * f) as f64 / cfg.sample_rate as f64;
    // Silence packets: frames starting a full lookahead + one frame after the tone end.
    let silence_first_frame = (cfg.tone_samples() + lookahead) / f + 1;
    let max_silence_packet_bytes = sizes
        .get(silence_first_frame..)
        .and_then(|s| s.iter().copied().max())
        .unwrap_or(0);

    // Steady tone region: skip 100 ms after start and stop 100 ms before the fade-out.
    let guard = cfg.sample_rate as usize / 10;
    let steady = guard..cfg.tone_samples().saturating_sub(guard);
    let measured_delay = measure_codec_delay(cfg, 2 * lookahead + f)?;

    let aligned: Vec<i16> = out[measured_delay..].to_vec();
    let snr_db = snr(&pcm[steady.clone()], &aligned[steady.clone()]);
    let tone_gain_db = TONES_HZ
        .iter()
        .map(|&hz| {
            let pin = goertzel_power(&pcm[steady.clone()], hz, cfg.sample_rate);
            let pout = goertzel_power(&aligned[steady.clone()], hz, cfg.sample_rate);
            (hz, 10.0 * (pout / pin).log10())
        })
        .collect();

    // Steady silence: from 200 ms after the tone ends (in aligned time) to the end.
    let silence_start = cfg.tone_samples() + cfg.sample_rate as usize / 5;
    let silence_peak = aligned
        .get(silence_start..)
        .map(|s| s.iter().map(|x| x.saturating_abs()).max().unwrap_or(0))
        .unwrap_or(0);

    Ok(Report {
        libopus_version: opus::version().to_string(),
        config: *cfg,
        frame_samples: f,
        packets: packets.len(),
        decoded_samples: out.len(),
        min_packet_bytes: sizes.iter().copied().min().unwrap_or(0),
        max_packet_bytes: sizes.iter().copied().max().unwrap_or(0),
        mean_packet_bytes: sizes.iter().sum::<usize>() as f64 / sizes.len().max(1) as f64,
        max_silence_packet_bytes,
        tone_payload_bps: tone_bytes as f64 * 8.0 / tone_secs,
        encoder_lookahead_samples: lookahead,
        measured_delay_samples: measured_delay,
        snr_db,
        tone_gain_db,
        silence_peak,
        all_frames_full,
        plc_frame_samples,
        deterministic,
        packet_hash: fnv1a(packets.iter().flat_map(|p| {
            // Length-prefix each packet so boundaries are part of the hash.
            (p.len() as u16)
                .to_le_bytes()
                .into_iter()
                .chain(p.iter().copied())
        })),
        encode_time,
        decode_time,
    })
}

/// Lag in `0..=max_lag` that maximises the normalised cross-correlation of
/// `input[range]` with `output[range + lag]`.
pub fn estimate_delay(
    input: &[i16],
    output: &[i16],
    range: std::ops::Range<usize>,
    max_lag: usize,
) -> usize {
    let mut best = (0usize, f64::MIN);
    for lag in 0..=max_lag {
        if range.end + lag > output.len() {
            break;
        }
        let (mut xy, mut yy) = (0.0f64, 0.0f64);
        for i in range.clone() {
            let x = input[i] as f64;
            let y = output[i + lag] as f64;
            xy += x * y;
            yy += y * y;
        }
        // Input energy is constant over lags, so dividing by sqrt(yy) is enough.
        let score = if yy > 0.0 { xy / yy.sqrt() } else { f64::MIN };
        if score > best.1 {
            best = (lag, score);
        }
    }
    best.0
}

/// SNR in dB of `decoded` against `reference` (same length, already aligned).
pub fn snr(reference: &[i16], decoded: &[i16]) -> f64 {
    let (mut sig, mut err) = (0.0f64, 0.0f64);
    for (&r, &d) in reference.iter().zip(decoded) {
        let r = r as f64;
        let e = d as f64 - r;
        sig += r * r;
        err += e * e;
    }
    if err == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (sig / err).log10()
    }
}

/// Power of the `hz` component in `x` (Goertzel).
pub fn goertzel_power(x: &[i16], hz: f64, sample_rate: u32) -> f64 {
    let w = 2.0 * PI * hz / sample_rate as f64;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s0 = v as f64 + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - c * s1 * s2) / (x.len() as f64).powi(2)
}

fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
