//! Prints an Opus round-trip report for generated PCM.
//!
//! Usage: opus-roundtrip-spike [--frame-ms 10|20|40|60] [--bitrate BPS]
//! Without --frame-ms, all four frame durations are reported.

use opus_roundtrip_spike::{run, Config, Report};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut frames: Vec<u32> = vec![10, 20, 40, 60];
    let mut base = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let val = args.next();
        let parsed = val.as_deref().and_then(|v| v.parse::<i64>().ok());
        match (a.as_str(), parsed) {
            ("--frame-ms", Some(v)) => frames = vec![v as u32],
            ("--bitrate", Some(v)) => base.bitrate_bps = v as i32,
            _ => {
                eprintln!("usage: opus-roundtrip-spike [--frame-ms 10|20|40|60] [--bitrate BPS]");
                return ExitCode::from(2);
            }
        }
    }

    let mut ok = true;
    for frame_ms in frames {
        let cfg = Config { frame_ms, ..base };
        match run(&cfg) {
            Ok(r) => print_report(&r),
            Err(e) => {
                eprintln!("frame_ms={frame_ms}: {e}");
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_report(r: &Report) {
    let c = &r.config;
    let sr = c.sample_rate as f64;
    println!("== Opus round trip: {} ms frames ==", c.frame_ms);
    println!("libopus              {}", r.libopus_version);
    println!(
        "input                {} Hz, mono, i16 PCM, {} ms tones {:?} Hz + {} ms silence",
        c.sample_rate,
        c.tone_ms,
        opus_roundtrip_spike::TONES_HZ,
        c.silence_ms
    );
    println!(
        "encoder              {:?}, target {} bps, VBR (default), complexity default, DTX off, FEC off",
        c.application, c.bitrate_bps
    );
    println!(
        "frames               {} samples/frame, {} packets, {} decoded samples, all full: {}",
        r.frame_samples, r.packets, r.decoded_samples, r.all_frames_full
    );
    println!(
        "packet bytes         min {} / mean {:.1} / max {} (silence max {})",
        r.min_packet_bytes, r.mean_packet_bytes, r.max_packet_bytes, r.max_silence_packet_bytes
    );
    println!("tone payload         {:.0} bps", r.tone_payload_bps);
    println!(
        "codec delay          lookahead {} samples ({:.2} ms), measured {} samples ({:.2} ms)",
        r.encoder_lookahead_samples,
        r.encoder_lookahead_samples as f64 * 1000.0 / sr,
        r.measured_delay_samples,
        r.measured_delay_samples as f64 * 1000.0 / sr
    );
    println!("SNR (steady tones)   {:.1} dB", r.snr_db);
    let gains: Vec<String> = r
        .tone_gain_db
        .iter()
        .map(|(hz, db)| format!("{hz:.0} Hz {db:+.2} dB"))
        .collect();
    println!("tone gain            {}", gains.join(", "));
    println!("silence peak         {} (of 32767)", r.silence_peak);
    println!("PLC frame            {} samples", r.plc_frame_samples);
    println!(
        "deterministic        {} (packet hash {:016x})",
        r.deterministic, r.packet_hash
    );
    println!(
        "time                 encode {:.2} ms, decode {:.2} ms for {:.1} s of audio",
        r.encode_time.as_secs_f64() * 1000.0,
        r.decode_time.as_secs_f64() * 1000.0,
        r.decoded_samples as f64 / sr
    );
    println!();
}
