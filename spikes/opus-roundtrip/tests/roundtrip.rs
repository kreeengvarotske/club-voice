use opus_roundtrip_spike::{
    encode_all, generate_pcm, run, Channels, Config, Report, MAX_OPUS_PACKET,
};

/// Checks that must hold for every supported frame duration.
fn assert_sound(r: &Report) {
    let c = &r.config;
    let input_len = c.total_samples();
    let f = c.frame_samples();

    assert!(
        r.libopus_version.starts_with("libopus "),
        "{}",
        r.libopus_version
    );
    assert_eq!(r.packets * f, input_len, "one packet per input frame");
    assert_eq!(
        r.decoded_samples, input_len,
        "decoded length equals input length"
    );
    assert!(
        r.all_frames_full,
        "every packet decoded to exactly one frame"
    );
    assert!(r.min_packet_bytes >= 1);
    assert!(r.max_packet_bytes <= MAX_OPUS_PACKET);

    // VBR may overshoot the target briefly but the average should be close.
    let target = c.bitrate_bps as f64;
    assert!(
        (0.6 * target..=1.4 * target).contains(&r.tone_payload_bps),
        "tone payload {:.0} bps vs target {target}",
        r.tone_payload_bps
    );

    // The decoder output lags the input by the encoder lookahead.
    let d = r.measured_delay_samples as i64 - r.encoder_lookahead_samples as i64;
    assert!(
        d.abs() <= 8,
        "measured delay {} vs lookahead {}",
        r.measured_delay_samples,
        r.encoder_lookahead_samples
    );

    // Every test tone survives with its level intact.
    for (hz, db) in &r.tone_gain_db {
        assert!(db.abs() <= 1.5, "{hz} Hz changed by {db:.2} dB");
    }

    // Opus is perceptual, not waveform-preserving, so sample SNR is only a
    // coarse sanity floor (observed about 8-9 dB at 32 kbps).
    assert!(r.snr_db >= 4.0, "SNR {:.1} dB", r.snr_db);

    // Digital silence in, (near) silence out.
    assert!(r.silence_peak <= 16, "silence peak {}", r.silence_peak);

    assert_eq!(r.plc_frame_samples, f, "PLC fills a whole frame");
    assert!(
        r.deterministic,
        "same input and settings give identical packets"
    );
}

#[test]
fn twenty_ms_default_round_trip() {
    let cfg = Config::default();
    assert_eq!(cfg.sample_rate, 48_000);
    assert_eq!(cfg.frame_samples(), 960);
    let r = run(&cfg).expect("round trip");
    assert_sound(&r);
    // Silence packets with DTX off are tiny (a few bytes) at 20 ms.
    assert!(
        r.max_silence_packet_bytes <= 8,
        "{}",
        r.max_silence_packet_bytes
    );
}

#[test]
fn all_supported_frame_durations_round_trip() {
    for frame_ms in [10, 20, 40, 60] {
        let cfg = Config {
            frame_ms,
            ..Config::default()
        };
        let r = run(&cfg).unwrap_or_else(|e| panic!("{frame_ms} ms: {e}"));
        assert_sound(&r);
    }
}

#[test]
fn unsupported_frame_duration_is_rejected() {
    let cfg = Config {
        frame_ms: 15,
        ..Config::default()
    };
    assert!(run(&cfg).is_err());
}

#[test]
fn partial_frame_input_is_rejected() {
    let cfg = Config::default();
    let pcm = vec![0i16; cfg.frame_samples() + 1];
    assert!(encode_all(&cfg, &pcm).is_err());
}

#[test]
fn generated_pcm_is_deterministic_and_frame_aligned() {
    let cfg = Config::default();
    let a = generate_pcm(&cfg);
    assert_eq!(a, generate_pcm(&cfg));
    assert_eq!(a.len() % cfg.frame_samples(), 0);
    assert!(a.iter().any(|&s| s != 0));
    assert!(a[cfg.tone_samples()..].iter().all(|&s| s == 0));
}

#[test]
fn higher_bitrate_costs_more_bytes() {
    let low = run(&Config {
        bitrate_bps: 16_000,
        ..Config::default()
    })
    .unwrap();
    let high = run(&Config {
        bitrate_bps: 64_000,
        ..Config::default()
    })
    .unwrap();
    assert!(
        high.mean_packet_bytes > low.mean_packet_bytes * 1.5,
        "16k {:.1} B vs 64k {:.1} B",
        low.mean_packet_bytes,
        high.mean_packet_bytes
    );
}

#[test]
fn decoding_recovers_after_a_lost_packet() {
    let cfg = Config::default();
    let f = cfg.frame_samples();
    let packets = encode_all(&cfg, &generate_pcm(&cfg)).unwrap();
    let mut dec = opus::Decoder::new(cfg.sample_rate, Channels::Mono).unwrap();
    let mut frame = vec![0i16; f];
    for (i, p) in packets.iter().enumerate() {
        // Drop packet 40 (inside the tone segment): conceal instead of decode.
        let input: &[u8] = if i == 40 { &[] } else { p };
        let n = dec.decode(input, &mut frame, false).unwrap();
        assert_eq!(n, f, "packet {i}");
    }
}
