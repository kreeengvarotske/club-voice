//! Reusable frame-level codec and synthetic source for the media path.
//!
//! Intended for the process harness / media relay tests (Issue #1): one call
//! encodes exactly one PCM frame into a caller-owned buffer, one call decodes
//! exactly one packet (or conceals a lost one) into a caller-owned buffer.
//! Nothing here allocates per frame, owns a queue, or knows about the wire
//! format; framing and sequencing stay with the transport.

use std::f64::consts::PI;

use crate::{Config, SpikeError, MAX_OPUS_PACKET, TONES_HZ};
use opus::Channels;

/// Frame durations this API accepts. 10 and 20 ms are the candidates for the
/// room default; 40 and 60 ms are kept for comparison runs.
pub const SUPPORTED_FRAME_MS: [u32; 4] = [10, 20, 40, 60];

fn validate(cfg: &Config) -> Result<usize, SpikeError> {
    if cfg.sample_rate != 48_000 {
        return Err(SpikeError::Invalid(format!(
            "sample_rate {} not supported, use 48000",
            cfg.sample_rate
        )));
    }
    if !SUPPORTED_FRAME_MS.contains(&cfg.frame_ms) {
        return Err(SpikeError::Invalid(format!(
            "frame_ms {} not one of {:?}",
            cfg.frame_ms, SUPPORTED_FRAME_MS
        )));
    }
    Ok(cfg.frame_samples())
}

/// Mono 48 kHz Opus encoder that takes exactly one frame per call.
pub struct FrameEncoder {
    enc: opus::Encoder,
    frame_samples: usize,
}

impl FrameEncoder {
    pub fn new(cfg: &Config) -> Result<Self, SpikeError> {
        let frame_samples = validate(cfg)?;
        let mut enc = opus::Encoder::new(cfg.sample_rate, Channels::Mono, cfg.application)?;
        enc.set_bitrate(opus::Bitrate::Bits(cfg.bitrate_bps))?;
        Ok(FrameEncoder { enc, frame_samples })
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    /// Encoder lookahead in samples (codec delay added on top of the frame).
    pub fn lookahead(&mut self) -> Result<usize, SpikeError> {
        let v = self.enc.get_lookahead()?;
        usize::try_from(v).map_err(|_| SpikeError::Invalid(format!("negative lookahead {v}")))
    }

    /// Encode one frame into `out` and return the payload length in bytes.
    /// `pcm` must hold exactly `frame_samples()` samples.
    pub fn encode(
        &mut self,
        pcm: &[i16],
        out: &mut [u8; MAX_OPUS_PACKET],
    ) -> Result<usize, SpikeError> {
        if pcm.len() != self.frame_samples {
            return Err(SpikeError::Invalid(format!(
                "frame has {} samples, expected {}",
                pcm.len(),
                self.frame_samples
            )));
        }
        Ok(self.enc.encode(pcm, out)?)
    }
}

/// Mono 48 kHz Opus decoder that produces exactly one frame per call.
pub struct FrameDecoder {
    dec: opus::Decoder,
    frame_samples: usize,
}

impl FrameDecoder {
    pub fn new(cfg: &Config) -> Result<Self, SpikeError> {
        let frame_samples = validate(cfg)?;
        let dec = opus::Decoder::new(cfg.sample_rate, Channels::Mono)?;
        Ok(FrameDecoder { dec, frame_samples })
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    /// Decode one packet into `out[..frame_samples()]`, or conceal a lost
    /// packet when `packet` is `None`. Returns the number of samples written,
    /// which is always `frame_samples()` on success.
    ///
    /// A packet whose duration differs from the configured frame is rejected
    /// before decoding, so the playout side never receives a short or long
    /// frame unexpectedly.
    pub fn decode(&mut self, packet: Option<&[u8]>, out: &mut [i16]) -> Result<usize, SpikeError> {
        if out.len() < self.frame_samples {
            return Err(SpikeError::Invalid(format!(
                "output has {} samples, need {}",
                out.len(),
                self.frame_samples
            )));
        }
        let out = &mut out[..self.frame_samples];
        let input: &[u8] = match packet {
            Some([]) => {
                return Err(SpikeError::Invalid(
                    "empty packet; pass None for a lost packet".into(),
                ))
            }
            Some(p) => {
                let n = self.dec.get_nb_samples(p)?;
                if n != self.frame_samples {
                    return Err(SpikeError::Invalid(format!(
                        "packet carries {n} samples, expected {}",
                        self.frame_samples
                    )));
                }
                p
            }
            None => &[],
        };
        let n = self.dec.decode(input, out, false)?;
        if n != self.frame_samples {
            return Err(SpikeError::Invalid(format!(
                "decoder returned {n} samples, expected {}",
                self.frame_samples
            )));
        }
        Ok(n)
    }

    /// Drop decoder history, e.g. when a speaker's stream restarts.
    pub fn reset(&mut self) -> Result<(), SpikeError> {
        Ok(self.dec.reset_state()?)
    }
}

/// Endless, deterministic synthetic voice-band source: the same three tones
/// as [`crate::generate_pcm`] (without fades), phase-continuous across frames.
/// Two instances created with the same config produce identical frames.
pub struct SyntheticSource {
    frame_samples: usize,
    sample_rate: f64,
    next_sample: u64,
}

impl SyntheticSource {
    pub fn new(cfg: &Config) -> Result<Self, SpikeError> {
        let frame_samples = validate(cfg)?;
        Ok(SyntheticSource {
            frame_samples,
            sample_rate: cfg.sample_rate as f64,
            next_sample: 0,
        })
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    /// Fill `out[..frame_samples()]` with the next frame. Panics if `out` is shorter.
    pub fn next_frame(&mut self, out: &mut [i16]) {
        for slot in &mut out[..self.frame_samples] {
            // Phase from the sample index modulo the 10 ms common period of
            // the tones keeps precision stable for arbitrarily long runs.
            let n = self.next_sample % (self.sample_rate as u64 / 100);
            let t = n as f64 / self.sample_rate;
            let s: f64 = TONES_HZ.iter().map(|f| (2.0 * PI * f * t).sin()).sum();
            *slot = (0.2 * s * i16::MAX as f64).round() as i16;
            self.next_sample += 1;
        }
    }
}
