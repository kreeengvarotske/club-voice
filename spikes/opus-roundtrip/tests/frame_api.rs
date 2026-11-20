use opus_roundtrip_spike::{
    encode_all, goertzel_power, Config, FrameDecoder, FrameEncoder, SyntheticSource,
    MAX_OPUS_PACKET, TONES_HZ,
};

fn cfg(frame_ms: u32) -> Config {
    Config {
        frame_ms,
        ..Config::default()
    }
}

/// Stream `frames` synthetic frames through encoder and decoder, returning
/// (input, output) concatenated.
fn stream(frame_ms: u32, frames: usize) -> (Vec<i16>, Vec<i16>) {
    let c = cfg(frame_ms);
    let mut src = SyntheticSource::new(&c).unwrap();
    let mut enc = FrameEncoder::new(&c).unwrap();
    let mut dec = FrameDecoder::new(&c).unwrap();
    let f = src.frame_samples();
    assert_eq!(enc.frame_samples(), f);
    assert_eq!(dec.frame_samples(), f);
    let mut pcm = vec![0i16; f];
    let mut pkt = [0u8; MAX_OPUS_PACKET];
    let mut out = vec![0i16; f];
    let (mut all_in, mut all_out) = (Vec::new(), Vec::new());
    for _ in 0..frames {
        src.next_frame(&mut pcm);
        let n = enc.encode(&pcm, &mut pkt).unwrap();
        assert!((1..=MAX_OPUS_PACKET).contains(&n));
        assert_eq!(dec.decode(Some(&pkt[..n]), &mut out).unwrap(), f);
        all_in.extend_from_slice(&pcm);
        all_out.extend_from_slice(&out);
    }
    (all_in, all_out)
}

#[test]
fn ten_and_twenty_ms_stream_round_trip() {
    for (frame_ms, samples) in [(10, 480), (20, 960)] {
        let frames = 1000 / frame_ms as usize; // one second
        let (input, output) = stream(frame_ms, frames);
        assert_eq!(input.len(), frames * samples);
        assert_eq!(output.len(), input.len());
        // Skip the first 100 ms (codec start-up) and compare tone levels.
        let r = 4800..input.len();
        for hz in TONES_HZ {
            let gin = goertzel_power(&input[r.clone()], hz, 48_000);
            let gout = goertzel_power(&output[r.clone()], hz, 48_000);
            let db = 10.0 * (gout / gin).log10();
            assert!(db.abs() <= 1.5, "{frame_ms} ms, {hz} Hz: {db:.2} dB");
        }
    }
}

#[test]
fn frame_encoder_matches_batch_encoder() {
    let c = cfg(20);
    let mut src = SyntheticSource::new(&c).unwrap();
    let mut pcm = vec![0i16; 960 * 25];
    for chunk in pcm.chunks_exact_mut(960) {
        src.next_frame(chunk);
    }
    let batch = encode_all(&c, &pcm).unwrap();
    let mut enc = FrameEncoder::new(&c).unwrap();
    let mut buf = [0u8; MAX_OPUS_PACKET];
    for (frame, expected) in pcm.chunks_exact(960).zip(&batch) {
        let n = enc.encode(frame, &mut buf).unwrap();
        assert_eq!(&buf[..n], expected.as_slice());
    }
}

#[test]
fn synthetic_source_is_deterministic_and_phase_continuous() {
    // Frame size must not change the sample sequence.
    let mut a = SyntheticSource::new(&cfg(10)).unwrap();
    let mut b = SyntheticSource::new(&cfg(20)).unwrap();
    let mut sa = vec![0i16; 480 * 200];
    let mut sb = vec![0i16; 960 * 100];
    for c in sa.chunks_exact_mut(480) {
        a.next_frame(c);
    }
    for c in sb.chunks_exact_mut(960) {
        b.next_frame(c);
    }
    assert_eq!(sa, sb);
    assert!(sa.iter().any(|&s| s != 0));
    // The phase shortcut relies on every tone being a multiple of 100 Hz.
    for hz in TONES_HZ {
        assert_eq!(hz % 100.0, 0.0);
    }
}

#[test]
fn wrong_frame_length_is_rejected() {
    let mut enc = FrameEncoder::new(&cfg(20)).unwrap();
    let mut buf = [0u8; MAX_OPUS_PACKET];
    assert!(enc.encode(&[0i16; 480], &mut buf).is_err());
    assert!(enc.encode(&[0i16; 961], &mut buf).is_err());
}

#[test]
fn packet_of_other_duration_is_rejected() {
    let mut enc10 = FrameEncoder::new(&cfg(10)).unwrap();
    let mut buf = [0u8; MAX_OPUS_PACKET];
    let n = enc10.encode(&[0i16; 480], &mut buf).unwrap();
    let mut dec20 = FrameDecoder::new(&cfg(20)).unwrap();
    let mut out = [0i16; 960];
    assert!(dec20.decode(Some(&buf[..n]), &mut out).is_err());
    assert!(dec20.decode(Some(&[]), &mut out).is_err());
    assert!(FrameDecoder::new(&cfg(10))
        .unwrap()
        .decode(Some(&buf[..n]), &mut out[..479])
        .is_err());
}

#[test]
fn lost_packet_is_concealed_with_a_full_frame() {
    for frame_ms in [10, 20] {
        let c = cfg(frame_ms);
        let mut src = SyntheticSource::new(&c).unwrap();
        let mut enc = FrameEncoder::new(&c).unwrap();
        let mut dec = FrameDecoder::new(&c).unwrap();
        let f = enc.frame_samples();
        let (mut pcm, mut out) = (vec![0i16; f], vec![0i16; f]);
        let mut buf = [0u8; MAX_OPUS_PACKET];
        for i in 0..20 {
            src.next_frame(&mut pcm);
            let n = enc.encode(&pcm, &mut buf).unwrap();
            let pkt = if i == 10 { None } else { Some(&buf[..n]) };
            assert_eq!(dec.decode(pkt, &mut out).unwrap(), f);
        }
        dec.reset().unwrap();
        assert_eq!(dec.decode(None, &mut out).unwrap(), f);
    }
}

#[test]
fn unsupported_configs_are_rejected() {
    assert!(FrameEncoder::new(&cfg(15)).is_err());
    assert!(FrameDecoder::new(&cfg(5)).is_err());
    let c = Config {
        sample_rate: 16_000,
        ..Config::default()
    };
    assert!(FrameEncoder::new(&c).is_err());
    assert!(SyntheticSource::new(&c).is_err());
}
