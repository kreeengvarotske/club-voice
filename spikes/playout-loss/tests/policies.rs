use club_voice_protocol::media::{HEADER, Header, MAX_DATAGRAM};
use opus_roundtrip_spike::Config;
use playout_loss_spike::{
    Arrival, BASE_DELAY_US, Impairment, Loss, Policy, STREAM, Sent, Slot, evaluate, network,
    policies, receive, scenarios, send,
};

fn cfg20() -> Config {
    Config {
        frame_ms: 20,
        ..Config::default()
    }
}

const WINDOW20: Policy = Policy::Window {
    window_ms: 20,
    max_plc_frames: 3,
    capacity: 16,
};

/// Arrivals for the given sequence order; each packet arrives at
/// `send time + base delay + extra_us[i]` (extra defaults to 0).
fn deliver(sent: &[Sent], order: &[u32], extra_us: &[u64]) -> Vec<Arrival> {
    let mut v: Vec<Arrival> = order
        .iter()
        .enumerate()
        .map(|(i, &seq)| {
            let s = &sent[seq as usize];
            Arrival {
                arrival_us: s.sent_us + BASE_DELAY_US + extra_us.get(i).copied().unwrap_or(0),
                datagram: s.datagram.clone(),
            }
        })
        .collect();
    v.sort_by_key(|a| a.arrival_us);
    v
}

fn slots(o: &playout_loss_spike::Outcome) -> Vec<Slot> {
    o.slots.iter().map(|s| s.unwrap_or(Slot::Silent)).collect()
}

#[test]
fn clean_network_plays_every_frame_for_every_policy() {
    for frame_ms in [10, 20] {
        let cfg = Config {
            frame_ms,
            ..Config::default()
        };
        for p in policies(frame_ms) {
            let s = evaluate(&cfg, 100, &Impairment::CLEAN, p, &[1, 2]);
            assert_eq!(s.real, 200, "{frame_ms} ms {p:?}");
            assert_eq!((s.concealed, s.silent, s.late, s.duplicate), (0, 0, 0, 0));
            assert!(s.worst_tone_db.abs() < 0.5);
        }
    }
}

#[test]
fn network_model_is_deterministic_per_seed() {
    let (_, sent) = send(&cfg20(), 200);
    let imp = scenarios()
        .into_iter()
        .find(|s| s.name.starts_with("mixed"))
        .unwrap()
        .impairment;
    let a: Vec<u64> = network(&sent, &imp, 3)
        .iter()
        .map(|x| x.arrival_us)
        .collect();
    let b: Vec<u64> = network(&sent, &imp, 3)
        .iter()
        .map(|x| x.arrival_us)
        .collect();
    let c: Vec<u64> = network(&sent, &imp, 4)
        .iter()
        .map(|x| x.arrival_us)
        .collect();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn gilbert_loss_matches_its_parameters() {
    // Dummy datagrams: the network model does not parse them.
    let sent: Vec<Sent> = (0..200_000u32)
        .map(|i| Sent {
            sequence: i,
            sent_us: i as u64 * 20_000,
            datagram: Vec::new(),
        })
        .collect();
    let imp = Impairment {
        loss: Loss::Gilbert {
            p_good_to_bad: 0.0175,
            p_bad_to_good: 1.0 / 3.0,
        },
        ..Impairment::CLEAN
    };
    let delivered = network(&sent, &imp, 9).len();
    let loss = 1.0 - delivered as f64 / sent.len() as f64;
    let expected = 0.0175 / (0.0175 + 1.0 / 3.0);
    assert!(
        (loss - expected).abs() < 0.005,
        "loss {loss:.4} vs {expected:.4}"
    );
}

#[test]
fn adjacent_swap_costs_a_real_frame_without_a_window() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 6);
    // Packet 2 is delayed by one frame and arrives just after packet 3, at
    // the same instant (the stable sort keeps 3 first).
    let arrivals = deliver(&sent, &[0, 1, 3, 2, 4, 5], &[0, 0, 0, 20_000, 0, 0]);
    let cur = receive(&cfg, 6, &arrivals, Policy::CURRENT_PR3);
    assert_eq!(slots(&cur)[2], Slot::Concealed);
    assert_eq!(cur.late, 1);
    let win = receive(&cfg, 6, &arrivals, WINDOW20);
    assert!(
        slots(&win).iter().all(|s| *s == Slot::Real),
        "{:?}",
        slots(&win)
    );
    assert_eq!(win.late, 0);

    // A 25 ms delay misses a 20 ms window but fits a 40 ms one.
    let arrivals = deliver(&sent, &[0, 1, 2, 3, 4, 5], &[0, 0, 25_000, 0, 0, 0]);
    let win = receive(&cfg, 6, &arrivals, WINDOW20);
    assert_eq!((slots(&win)[2], win.late), (Slot::Concealed, 1));
    let w40 = Policy::Window {
        window_ms: 40,
        max_plc_frames: 3,
        capacity: 16,
    };
    let win = receive(&cfg, 6, &arrivals, w40);
    assert!(slots(&win).iter().all(|s| *s == Slot::Real));
}

#[test]
fn burst_longer_than_plc_limit_resets_once() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 12);
    let order = [0, 1, 2, 8, 9, 10, 11]; // 3..=7 lost (5 frames)
    let arrivals = deliver(&sent, &order, &[]);

    let cur = receive(&cfg, 12, &arrivals, Policy::CURRENT_PR3);
    let s = slots(&cur);
    assert!(s[3..8].iter().all(|x| *x == Slot::Silent), "{s:?}");
    assert!(s[8..].iter().all(|x| *x == Slot::Real));
    assert_eq!(cur.resets, 1);

    let win = receive(&cfg, 12, &arrivals, WINDOW20);
    let s = slots(&win);
    assert_eq!(&s[3..6], &[Slot::Concealed; 3]);
    assert_eq!(&s[6..8], &[Slot::Silent; 2]);
    assert!(s[8..].iter().all(|x| *x == Slot::Real));
    assert_eq!(win.resets, 1);
}

#[test]
fn single_loss_is_concealed_by_all_policies() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 8);
    let arrivals = deliver(&sent, &[0, 1, 2, 4, 5, 6, 7], &[]);
    for p in policies(20) {
        let o = receive(&cfg, 8, &arrivals, p);
        assert_eq!(slots(&o)[3], Slot::Concealed, "{p:?}");
        assert_eq!(o.count(Slot::Real), 7);
        assert_eq!(o.resets, 0);
    }
}

#[test]
fn duplicates_are_counted_and_never_replayed() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 5);
    let arrivals = deliver(&sent, &[0, 1, 1, 2, 3, 3, 4], &[0, 0, 500, 0, 0, 500, 0]);
    for p in policies(20) {
        let o = receive(&cfg, 5, &arrivals, p);
        assert_eq!(o.duplicate, 2, "{p:?}");
        assert_eq!(o.count(Slot::Real), 5);
    }
}

#[test]
fn window_buffer_is_bounded() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 40);
    // Packet 30 arrives right after packet 0, far ahead of playout.
    let mut arrivals = deliver(&sent, &[0], &[]);
    arrivals.push(Arrival {
        arrival_us: arrivals[0].arrival_us + 1,
        datagram: sent[30].datagram.clone(),
    });
    let policy = Policy::Window {
        window_ms: 20,
        max_plc_frames: 3,
        capacity: 4,
    };
    let o = receive(&cfg, 40, &arrivals, policy);
    assert_eq!(o.too_early, 1);
    assert!(o.peak_buffered <= 4);
}

#[test]
fn foreign_and_malformed_datagrams_are_ignored() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 3);
    let mut arrivals = deliver(&sent, &[0, 1, 2], &[]);
    let mut other = [0u8; MAX_DATAGRAM];
    Header {
        stream: STREAM + 1,
        frame_ms: 20,
        sequence: 1,
        sent_us: 20_000,
    }
    .write(&mut other);
    let t = arrivals[0].arrival_us;
    arrivals.insert(
        1,
        Arrival {
            arrival_us: t,
            datagram: other[..HEADER + 3].to_vec(),
        },
    );
    arrivals.insert(
        1,
        Arrival {
            arrival_us: t,
            datagram: b"PONG".to_vec(),
        },
    );
    for p in policies(20) {
        let o = receive(&cfg, 3, &arrivals, p);
        assert_eq!(o.malformed, 2, "{p:?}");
        assert_eq!(o.count(Slot::Real), 3);
    }
}

#[test]
fn immediate_policy_drops_packets_older_than_max_age() {
    let cfg = cfg20();
    let (_, sent) = send(&cfg, 4);
    let arrivals = deliver(&sent, &[0, 1, 2, 3], &[0, 0, 0, 90_000]);
    let o = receive(&cfg, 4, &arrivals, Policy::CURRENT_PR3);
    assert_eq!(o.expired, 1);
    assert_eq!(slots(&o)[3], Slot::Silent);
}

#[test]
fn window_absorbs_jitter_within_its_depth() {
    // Uniform 0-30 ms jitter at 20 ms frames: a 40 ms window should play
    // nearly every frame as real; immediate playout loses reordered frames.
    let cfg = cfg20();
    let imp = Impairment {
        jitter_ms: 30,
        ..Impairment::CLEAN
    };
    let w40 = Policy::Window {
        window_ms: 40,
        max_plc_frames: 3,
        capacity: 16,
    };
    let win = evaluate(&cfg, 500, &imp, w40, &[1, 2, 3]);
    let cur = evaluate(&cfg, 500, &imp, Policy::CURRENT_PR3, &[1, 2, 3]);
    assert!(win.real as f64 >= 0.995 * win.frames as f64, "{win:?}");
    assert!(cur.late > 0 && cur.real < win.real, "{cur:?}");
}
