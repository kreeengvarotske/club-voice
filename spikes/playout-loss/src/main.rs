//! Prints the loss/reordering characterization as Markdown tables.
//!
//! Usage: playout-loss-spike [--seconds N] [--seeds N]

use opus_roundtrip_spike::Config;
use playout_loss_spike::{evaluate, policies, scenarios};
use std::process::ExitCode;

fn main() -> ExitCode {
    let (mut seconds, mut seeds) = (10u32, 5u64);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = args.next().and_then(|v| v.parse::<u64>().ok());
        match (a.as_str(), v) {
            ("--seconds", Some(v)) if v > 0 => seconds = v as u32,
            ("--seeds", Some(v)) if v > 0 => seeds = v,
            _ => {
                eprintln!("usage: playout-loss-spike [--seconds N] [--seeds N]");
                return ExitCode::from(2);
            }
        }
    }
    let seed_list: Vec<u64> = (1..=seeds).collect();
    println!(
        "Synthetic 48 kHz mono, {seconds} s per run, seeds 1..={seeds}, base one-way delay 10 ms.\n\
         Counts are totals over all seeds. 'Silent' includes frames before the first and after the last received packet.\n"
    );
    for frame_ms in [20u32, 10] {
        let cfg = Config {
            frame_ms,
            ..Config::default()
        };
        let frames = (seconds * 1000 / frame_ms) as usize;
        println!("### {frame_ms} ms frames ({frames} frames per run)\n");
        println!(
            "| Scenario | Policy | Added delay | Real | PLC | Silent | Longest silence | Late | Dup | Expired | Resets | Worst tone level |"
        );
        println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
        for sc in scenarios() {
            for p in policies(frame_ms) {
                let s = evaluate(&cfg, frames, &sc.impairment, p, &seed_list);
                let pct = |n: usize| 100.0 * n as f64 / s.frames as f64;
                println!(
                    "| {} | {} | {} ms | {:.2}% | {:.2}% | {:.2}% | {} ms | {} | {} | {} | {} | {:+.2} dB |",
                    sc.name,
                    p.label(),
                    p.added_delay_ms(),
                    pct(s.real),
                    pct(s.concealed),
                    pct(s.silent),
                    s.longest_silence_ms,
                    s.late,
                    s.duplicate,
                    s.expired,
                    s.resets,
                    s.worst_tone_db
                );
            }
        }
        println!();
    }
    ExitCode::SUCCESS
}
