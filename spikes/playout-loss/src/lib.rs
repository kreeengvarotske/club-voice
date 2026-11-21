//! Deterministic loss / reordering characterization of receiver playout
//! policies for the CVM2 media path (Issue #1).
//!
//! Everything is simulated in one thread with a seeded PRNG: no sockets, no
//! audio devices, no wall clock. The sender encodes `SyntheticSource` frames
//! with `FrameEncoder` and wraps them with the CVM2 `Header`; a network model
//! drops, delays, reorders and duplicates datagrams; receiver models parse the
//! headers, decode with `FrameDecoder` and record what each 10/20 ms output
//! slot contained.
//!
//! The `Immediate` policy with `max_plc_frames: 1` and an 80 ms age limit is a
//! model of the receive loop in `crates/client/examples/headset_player.rs` at
//! dcc7a77 (PLC for exactly one missing frame, decoder reset after a longer gap,
//! late and duplicate frames skipped). It is a re-implementation for
//! measurement, not that code itself.

use club_voice_protocol::media::{Header, MAX_DATAGRAM};
use opus_roundtrip_spike::{
    Config, FrameDecoder, FrameEncoder, MAX_OPUS_PACKET, SyntheticSource, TONES_HZ, goertzel_power,
};

/// Fixed one-way network delay added to every datagram before jitter.
pub const BASE_DELAY_US: u64 = 10_000;
/// Stream id used for the single simulated speaker.
pub const STREAM: u64 = 7;

// ---------------------------------------------------------------- PRNG

/// SplitMix64: small, fast, deterministic across platforms.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn chance(&mut self, p: f64) -> bool {
        p > 0.0 && self.unit() < p
    }
    /// Uniform integer in 0..=max.
    pub fn up_to(&mut self, max: u64) -> u64 {
        if max == 0 {
            0
        } else {
            self.next_u64() % (max + 1)
        }
    }
}

// ---------------------------------------------------------------- sender

/// One encoded datagram as it leaves the sender.
#[derive(Clone, Debug)]
pub struct Sent {
    pub sequence: u32,
    pub sent_us: u64,
    pub datagram: Vec<u8>,
}

/// Encode `frames` frames of synthetic audio. Returns the input PCM and the
/// CVM2 datagrams, sent exactly every frame duration starting at t = 0.
pub fn send(cfg: &Config, frames: usize) -> (Vec<i16>, Vec<Sent>) {
    let mut src = SyntheticSource::new(cfg).expect("config");
    let mut enc = FrameEncoder::new(cfg).expect("encoder");
    let f = src.frame_samples();
    let frame_us = cfg.frame_ms as u64 * 1000;
    let mut pcm = vec![0i16; f];
    let mut payload = [0u8; MAX_OPUS_PACKET];
    let mut input = Vec::with_capacity(frames * f);
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        src.next_frame(&mut pcm);
        input.extend_from_slice(&pcm);
        let n = enc.encode(&pcm, &mut payload).expect("encode");
        let header = Header {
            stream: STREAM,
            frame_ms: cfg.frame_ms as u8,
            sequence: i as u32,
            sent_us: i as u64 * frame_us,
        };
        let mut buf = [0u8; MAX_DATAGRAM];
        header.write(&mut buf);
        buf[club_voice_protocol::media::HEADER..][..n].copy_from_slice(&payload[..n]);
        out.push(Sent {
            sequence: header.sequence,
            sent_us: header.sent_us,
            datagram: buf[..club_voice_protocol::media::HEADER + n].to_vec(),
        });
    }
    (input, out)
}

// ---------------------------------------------------------------- network

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Loss {
    None,
    /// Independent loss with probability `p` per datagram.
    Bernoulli(f64),
    /// Two-state Gilbert-Elliott: every datagram sent in the bad state is lost.
    /// Mean burst length is `1 / p_bad_to_good`; long-run loss rate is
    /// `p_good_to_bad / (p_good_to_bad + p_bad_to_good)`.
    Gilbert {
        p_good_to_bad: f64,
        p_bad_to_good: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impairment {
    pub loss: Loss,
    /// Extra delay per datagram, uniform in 0..=jitter_ms (causes reordering
    /// when it exceeds the frame duration).
    pub jitter_ms: u32,
    /// Probability that a delivered datagram is swapped with the next one.
    pub swap: f64,
    /// Probability that a delivered datagram arrives twice (second copy 1 ms later).
    pub duplicate: f64,
}

impl Impairment {
    pub const CLEAN: Impairment = Impairment {
        loss: Loss::None,
        jitter_ms: 0,
        swap: 0.0,
        duplicate: 0.0,
    };
}

/// A datagram as the receiver sees it.
#[derive(Clone, Debug)]
pub struct Arrival {
    pub arrival_us: u64,
    pub datagram: Vec<u8>,
}

/// Apply the impairment and return arrivals in arrival order (stable).
pub fn network(sent: &[Sent], imp: &Impairment, seed: u64) -> Vec<Arrival> {
    let mut rng = Rng::new(seed);
    let mut bad = false;
    let mut kept: Vec<(u64, &Sent)> = Vec::with_capacity(sent.len());
    for s in sent {
        let lost = match imp.loss {
            Loss::None => false,
            Loss::Bernoulli(p) => rng.chance(p),
            Loss::Gilbert {
                p_good_to_bad,
                p_bad_to_good,
            } => {
                bad = if bad {
                    !rng.chance(p_bad_to_good)
                } else {
                    rng.chance(p_good_to_bad)
                };
                bad
            }
        };
        let jitter = rng.up_to(imp.jitter_ms as u64 * 1000);
        if !lost {
            kept.push((s.sent_us + BASE_DELAY_US + jitter, s));
        }
    }
    // Adjacent swaps among delivered datagrams: exchange arrival times.
    let mut i = 0;
    while i + 1 < kept.len() {
        if rng.chance(imp.swap) {
            let (a, b) = (kept[i].0, kept[i + 1].0);
            kept[i].0 = b.max(a);
            kept[i + 1].0 = a.min(b);
            i += 2;
        } else {
            i += 1;
        }
    }
    let mut out: Vec<Arrival> = Vec::with_capacity(kept.len() * 2);
    for (t, s) in kept {
        out.push(Arrival {
            arrival_us: t,
            datagram: s.datagram.clone(),
        });
        if rng.chance(imp.duplicate) {
            out.push(Arrival {
                arrival_us: t + 1_000,
                datagram: s.datagram.clone(),
            });
        }
    }
    out.sort_by_key(|a| a.arrival_us);
    out
}

// ---------------------------------------------------------------- receivers

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Decoded from the frame's own packet.
    Real,
    /// Filled by Opus packet-loss concealment.
    Concealed,
    /// Nothing played (gap after a reset, or before the first packet).
    Silent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Policy {
    /// Decode each packet as soon as it arrives. A gap of up to
    /// `max_plc_frames` missing frames is concealed; a longer gap resets the
    /// decoder and leaves the missing slots silent. Packets at or behind the
    /// last played sequence are skipped. Packets older than `max_age_ms`
    /// (by the sender timestamp) are dropped.
    Immediate {
        max_plc_frames: u32,
        max_age_ms: u32,
    },
    /// Hold packets in a bounded reorder buffer and play slot k at
    /// `first_arrival + window_ms + k * frame`. A missing slot is concealed
    /// for up to `max_plc_frames` consecutive slots, then silent; the decoder
    /// is reset before the next real frame after silence. Packets that arrive
    /// after their slot was played are late; packets more than `capacity`
    /// frames ahead are dropped.
    Window {
        window_ms: u32,
        max_plc_frames: u32,
        capacity: u32,
    },
}

impl Policy {
    /// Model of the receive loop in PR #3 at dcc7a77.
    pub const CURRENT_PR3: Policy = Policy::Immediate {
        max_plc_frames: 1,
        max_age_ms: 80,
    };

    pub fn added_delay_ms(&self) -> u32 {
        match *self {
            Policy::Immediate { .. } => 0,
            Policy::Window { window_ms, .. } => window_ms,
        }
    }

    pub fn label(&self) -> String {
        match *self {
            Policy::Immediate {
                max_plc_frames,
                max_age_ms,
            } => format!("immediate, PLC<={max_plc_frames}, age<={max_age_ms}ms"),
            Policy::Window {
                window_ms,
                max_plc_frames,
                ..
            } => format!("window {window_ms}ms, PLC<={max_plc_frames}"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub slots: Vec<Option<Slot>>,
    pub output: Vec<i16>,
    pub late: u64,
    pub duplicate: u64,
    pub expired: u64,
    pub too_early: u64,
    pub malformed: u64,
    pub resets: u64,
    /// Largest number of packets held at once (window policies).
    pub peak_buffered: usize,
}

impl Outcome {
    fn new(frames: usize, f: usize) -> Self {
        Outcome {
            slots: vec![None; frames],
            output: vec![0; frames * f],
            ..Default::default()
        }
    }
    fn put(&mut self, k: usize, slot: Slot, pcm: &[i16]) {
        if k < self.slots.len() {
            self.slots[k] = Some(slot);
            let f = pcm.len();
            self.output[k * f..(k + 1) * f].copy_from_slice(pcm);
        }
    }
    pub fn count(&self, s: Slot) -> usize {
        self.slots
            .iter()
            .filter(|x| x.unwrap_or(Slot::Silent) == s)
            .count()
    }
    /// Longest run of silent slots, in frames.
    pub fn longest_silence(&self) -> usize {
        let (mut best, mut run) = (0, 0);
        for s in &self.slots {
            if s.unwrap_or(Slot::Silent) == Slot::Silent {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
        best
    }
}

fn parse(datagram: &[u8], frame_ms: u32) -> Option<(Header, &[u8])> {
    let (h, p) = Header::parse(datagram)?;
    (h.stream == STREAM && h.frame_ms as u32 == frame_ms).then_some((h, p))
}

pub fn receive(cfg: &Config, frames: usize, arrivals: &[Arrival], policy: Policy) -> Outcome {
    match policy {
        Policy::Immediate {
            max_plc_frames,
            max_age_ms,
        } => immediate(cfg, frames, arrivals, max_plc_frames, max_age_ms),
        Policy::Window {
            window_ms,
            max_plc_frames,
            capacity,
        } => window(cfg, frames, arrivals, window_ms, max_plc_frames, capacity),
    }
}

fn immediate(
    cfg: &Config,
    frames: usize,
    arrivals: &[Arrival],
    max_plc: u32,
    max_age_ms: u32,
) -> Outcome {
    let mut dec = FrameDecoder::new(cfg).expect("decoder");
    let f = dec.frame_samples();
    let mut out = Outcome::new(frames, f);
    let mut pcm = vec![0i16; f];
    let mut last: Option<u32> = None;
    for a in arrivals {
        let Some((h, payload)) = parse(&a.datagram, cfg.frame_ms) else {
            out.malformed += 1;
            continue;
        };
        if a.arrival_us.saturating_sub(h.sent_us) > max_age_ms as u64 * 1000 {
            out.expired += 1;
            continue;
        }
        let distance = match last {
            None => 1,
            Some(prev) => {
                let d = h.sequence.wrapping_sub(prev);
                if d == 0 {
                    out.duplicate += 1;
                    continue;
                }
                if d > 0x7fff_ffff {
                    out.late += 1;
                    continue;
                }
                d
            }
        };
        let missing = distance - 1;
        if last.is_some() && missing > 0 {
            if missing <= max_plc {
                for m in 0..missing {
                    dec.decode(None, &mut pcm).expect("plc");
                    let k = h.sequence - missing + m;
                    out.put(k as usize, Slot::Concealed, &pcm);
                }
            } else {
                dec.reset().expect("reset");
                out.resets += 1;
            }
        }
        dec.decode(Some(payload), &mut pcm).expect("decode");
        out.put(h.sequence as usize, Slot::Real, &pcm);
        last = Some(h.sequence);
    }
    out
}

fn window(
    cfg: &Config,
    frames: usize,
    arrivals: &[Arrival],
    window_ms: u32,
    max_plc: u32,
    capacity: u32,
) -> Outcome {
    let mut dec = FrameDecoder::new(cfg).expect("decoder");
    let f = dec.frame_samples();
    let frame_us = cfg.frame_ms as u64 * 1000;
    let mut out = Outcome::new(frames, f);
    let mut pcm = vec![0i16; f];
    // Ring buffer indexed by sequence % capacity; holds (sequence, payload).
    let cap = capacity.max(1) as usize;
    let mut ring: Vec<Option<(u32, Vec<u8>)>> = vec![None; cap];
    let mut held = 0usize;

    let Some(first) = arrivals
        .iter()
        .find_map(|a| parse(&a.datagram, cfg.frame_ms).map(|(h, _)| (a.arrival_us, h.sequence)))
    else {
        return out;
    };
    let (anchor_us, base_seq) = first;
    let start_us = anchor_us + window_ms as u64 * 1000;

    let mut next_arrival = 0usize;
    let mut missing_run = 0u32;
    let mut need_reset = false;
    for k in 0..frames.saturating_sub(base_seq as usize) {
        let seq = base_seq + k as u32;
        let deadline = start_us + k as u64 * frame_us;
        while next_arrival < arrivals.len() && arrivals[next_arrival].arrival_us <= deadline {
            let a = &arrivals[next_arrival];
            next_arrival += 1;
            let Some((h, payload)) = parse(&a.datagram, cfg.frame_ms) else {
                out.malformed += 1;
                continue;
            };
            let ahead = h.sequence.wrapping_sub(seq);
            if ahead > 0x7fff_ffff {
                out.late += 1;
                continue;
            }
            if ahead >= capacity {
                out.too_early += 1;
                continue;
            }
            let idx = h.sequence as usize % cap;
            match &ring[idx] {
                Some((s, _)) if *s == h.sequence => out.duplicate += 1,
                _ => {
                    if ring[idx].is_none() {
                        held += 1;
                    }
                    ring[idx] = Some((h.sequence, payload.to_vec()));
                    out.peak_buffered = out.peak_buffered.max(held);
                }
            }
        }
        let idx = seq as usize % cap;
        let entry = match ring[idx].take() {
            Some((s, p)) if s == seq => {
                held -= 1;
                Some(p)
            }
            other => {
                ring[idx] = other;
                None
            }
        };
        match entry {
            Some(payload) => {
                if need_reset {
                    dec.reset().expect("reset");
                    out.resets += 1;
                    need_reset = false;
                }
                dec.decode(Some(&payload), &mut pcm).expect("decode");
                out.put(seq as usize, Slot::Real, &pcm);
                missing_run = 0;
            }
            None if missing_run < max_plc && !need_reset => {
                dec.decode(None, &mut pcm).expect("plc");
                out.put(seq as usize, Slot::Concealed, &pcm);
                missing_run += 1;
            }
            None => {
                out.put(seq as usize, Slot::Silent, &vec![0; f]);
                need_reset = true;
            }
        }
    }
    // Packets still arriving after the last slot are late by definition.
    out.late += arrivals[next_arrival..]
        .iter()
        .filter(|a| parse(&a.datagram, cfg.frame_ms).is_some())
        .count() as u64;
    out
}

// ---------------------------------------------------------------- scenarios

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: &'static str,
    pub impairment: Impairment,
}

pub fn scenarios() -> Vec<Scenario> {
    let c = Impairment::CLEAN;
    vec![
        Scenario {
            name: "clean",
            impairment: c,
        },
        Scenario {
            name: "random loss 2%",
            impairment: Impairment {
                loss: Loss::Bernoulli(0.02),
                ..c
            },
        },
        Scenario {
            name: "random loss 5%",
            impairment: Impairment {
                loss: Loss::Bernoulli(0.05),
                ..c
            },
        },
        Scenario {
            name: "burst loss ~5%, mean burst 3",
            impairment: Impairment {
                loss: Loss::Gilbert {
                    p_good_to_bad: 0.0175,
                    p_bad_to_good: 1.0 / 3.0,
                },
                ..c
            },
        },
        Scenario {
            name: "adjacent swaps 5%",
            impairment: Impairment { swap: 0.05, ..c },
        },
        Scenario {
            name: "jitter 0-30 ms",
            impairment: Impairment { jitter_ms: 30, ..c },
        },
        Scenario {
            name: "duplicates 5%",
            impairment: Impairment {
                duplicate: 0.05,
                ..c
            },
        },
        Scenario {
            name: "mixed: loss 2%, jitter 0-20 ms, dup 1%",
            impairment: Impairment {
                loss: Loss::Bernoulli(0.02),
                jitter_ms: 20,
                swap: 0.0,
                duplicate: 0.01,
            },
        },
    ]
}

pub fn policies(frame_ms: u32) -> Vec<Policy> {
    let plc60 = 60 / frame_ms;
    vec![
        Policy::CURRENT_PR3,
        Policy::Immediate {
            max_plc_frames: plc60,
            max_age_ms: 80,
        },
        Policy::Window {
            window_ms: 20,
            max_plc_frames: plc60,
            capacity: 16,
        },
        Policy::Window {
            window_ms: 40,
            max_plc_frames: plc60,
            capacity: 16,
        },
    ]
}

/// Totals over several seeds for one (scenario, policy, frame size).
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub frames: usize,
    pub real: usize,
    pub concealed: usize,
    pub silent: usize,
    pub longest_silence_ms: u32,
    pub late: u64,
    pub duplicate: u64,
    pub expired: u64,
    pub too_early: u64,
    pub resets: u64,
    pub peak_buffered: usize,
    /// Worst tone level change (dB, most negative) over all runs.
    pub worst_tone_db: f64,
}

pub fn tone_level_db(cfg: &Config, input: &[i16], output: &[i16]) -> f64 {
    // Skip the first 100 ms (codec start-up); level comparison is
    // insensitive to the constant codec delay.
    let skip = cfg.sample_rate as usize / 10;
    let (i, o) = (&input[skip..], &output[skip..]);
    TONES_HZ
        .iter()
        .map(|&hz| {
            let pi = goertzel_power(i, hz, cfg.sample_rate);
            let po = goertzel_power(o, hz, cfg.sample_rate).max(1e-12);
            10.0 * (po / pi).log10()
        })
        .fold(f64::INFINITY, f64::min)
}

pub fn evaluate(
    cfg: &Config,
    frames: usize,
    imp: &Impairment,
    policy: Policy,
    seeds: &[u64],
) -> Summary {
    let (input, sent) = send(cfg, frames);
    let mut s = Summary {
        worst_tone_db: f64::INFINITY,
        ..Default::default()
    };
    for &seed in seeds {
        let arrivals = network(&sent, imp, seed);
        let o = receive(cfg, frames, &arrivals, policy);
        s.frames += frames;
        s.real += o.count(Slot::Real);
        s.concealed += o.count(Slot::Concealed);
        s.silent += o.count(Slot::Silent);
        s.longest_silence_ms = s
            .longest_silence_ms
            .max(o.longest_silence() as u32 * cfg.frame_ms);
        s.late += o.late;
        s.duplicate += o.duplicate;
        s.expired += o.expired;
        s.too_early += o.too_early;
        s.resets += o.resets;
        s.peak_buffered = s.peak_buffered.max(o.peak_buffered);
        s.worst_tone_db = s.worst_tone_db.min(tone_level_db(cfg, &input, &o.output));
    }
    s
}
