# Playout loss and reordering spike

Bounded characterization for [Issue #1](https://github.com/kreeengvarotske/club-voice/issues/1): how receiver playout policies behave under packet loss, burst loss, reordering, jitter and duplicates on the CVM2 media path. It uses the shared frame API (`spikes/opus-roundtrip`) and the CVM2 `Header` from `club-voice-protocol`.

Scope: a single-threaded simulation with a seeded PRNG. No sockets, no audio devices, no wall clock, no changes to `crates/client` or `crates/server`. Policy changes are proposals only.

## Model

- **Sender:** `SyntheticSource` frames (48 kHz mono tones) are encoded with `FrameEncoder` and written with `Header::write` (stream 7, sequence from 0, `sent_us` = sequence x frame). One datagram is sent per frame.
- **Network:** a 10 ms base delay is applied to every datagram. On top of that, a scenario can add Bernoulli loss, Gilbert-Elliott burst loss (every datagram in the bad state is lost), uniform per-datagram jitter (which reorders packets when it exceeds the frame), explicit adjacent swaps and duplicates (the copy arrives 1 ms later). Arrivals are sorted by time.
- **Receivers:** every arrival goes through `Header::parse`. Foreign streams, other frame durations and non-media packets are counted as malformed. Each output slot is recorded as Real (the frame's own packet was decoded), PLC (filled by `FrameDecoder::decode(None)`) or Silent.
  - `immediate, PLC<=1, age<=80ms` models the receive loop in `crates/client/examples/headset_player.rs` at dcc7a77: decode on arrival, conceal exactly one missing frame, reset the decoder after a longer gap, skip duplicate and late sequences, drop packets older than 80 ms. It is a re-implementation for measurement, not that code. The playback-side 80 ms check on queued frames is not modelled. The real client only accepts 20 ms, so the 10 ms rows for this policy are hypothetical.
  - `immediate, PLC<=N`: the same policy, but it conceals gaps of up to 60 ms (3 frames at 20 ms, 6 at 10 ms).
  - `window W ms, PLC<=N`: a bounded reorder buffer (capacity 16 frames, indexed by sequence) whose slot k plays at `first arrival + W + k x frame`. A packet that arrives after its slot has played counts as late. Up to N consecutive missing slots are concealed, then slots stay silent and the decoder is reset before the next real frame. This adds W ms of delay.

## Commands

From the repository root (Windows: use `\` in the path). The build compiles bundled libopus and needs CMake and a C compiler, the same as `spikes/opus-roundtrip`.

```
cargo test --release --manifest-path spikes/playout-loss/Cargo.toml
cargo run --release --manifest-path spikes/playout-loss/Cargo.toml
cargo run --release --manifest-path spikes/playout-loss/Cargo.toml -- --seconds 30 --seeds 10
```

## Tests

`tests/policies.rs` (11 tests):

- On a clean network every policy plays every frame as real.
- The network model is deterministic per seed, and Gilbert-Elliott loss matches its configured rate.
- An adjacent swap costs the current policy a real frame (PLC, then the real packet is dropped as late). A 20 ms window plays it, a 25 ms delay misses a 20 ms window and fits a 40 ms one.
- A 5-frame burst: the current policy resets once and leaves 5 slots silent. The window policy conceals 3 slots, then leaves 2 silent, then resets once.
- A single loss is concealed by every policy. Duplicates are counted and never replayed. The reorder buffer stays within its capacity and drops packets that are too far ahead.
- Foreign streams and non-media datagrams are ignored. Packets older than 80 ms are dropped by the immediate policy.
- With 0-30 ms jitter at 20 ms frames, a 40 ms window plays at least 99.5% of frames as real and beats immediate playout.

## Findings (see RESULTS.md)

- **Random loss up to 5%:** every policy recovers the same real frames. The only difference is that the current one-frame PLC leaves short gaps silent when two losses are adjacent: 0.48% silent at 20 ms and 5% loss. PLC for up to 60 ms removes that.
- **Burst loss (about 5%, mean burst 3 frames):** at 20 ms, the current policy leaves 4.68% of frames silent and resets 29 times in 2,500 frames. PLC for up to 60 ms cuts silence to 3.00% with immediate playout and to 1.56% with a window. Very long bursts still end in silence, which is intended, since Opus PLC fades out.
- **Reordering:** without a buffer, every reordered packet becomes PLC followed by a dropped real packet. A 20 ms window recovers all adjacent swaps at 20 ms and 10 ms.
- **Jitter 0-30 ms (uniform, independent per packet, pessimistic):** at 20 ms, immediate playout plays 94.3% real frames, a 20 ms window 97.6% and a 40 ms window 99.96%. At 10 ms frames, immediate playout falls to 74.7% real and, with one-frame PLC, 6.7% silent. 10 ms frames therefore need a reorder window more than 20 ms frames do.
- **Duplicates:** every policy discards them correctly.

## Proposal (not implemented in the client)

1. Keep 20 ms as the default frame. On the same impairments it is less sensitive to reordering than 10 ms without a buffer.
2. Extend the receive path's PLC from exactly one frame to a 60 ms budget before a decoder reset. This adds no latency.
3. Consider a small reorder window of 20 ms, optionally adaptive up to 40 ms, sized from measured LAN or internet jitter. It is the only option here that recovers reordered packets, and it costs its own depth in added delay. Decide with real LAN/Wi-Fi jitter traces, not with this synthetic model.

## Limits

- Synthetic tones, not speech. "Worst tone level" is a coarse indicator of silence and PLC fade, not a quality score.
- The jitter model is i.i.d. uniform. Real networks have correlated jitter and delay spikes.
- There is one speaker and one stream. Stream replacement, session IDs and the playback queue are not modelled. Sequences start at 0, so wraparound is not exercised here.
