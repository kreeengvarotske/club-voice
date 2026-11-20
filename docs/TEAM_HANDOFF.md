# Team handoff: current implementation baseline

Updated November 20, 2026.

## What exists

- A Rust workspace with `club-voice-protocol`, `club-voice-server`, and `club-voice-client` crates.
- Default Lobby / Team A / Team B room state, password admission, and a server member limit.
- A bounded control-plane queue with observable drops. It is not an audio queue.
- A client capture gate that starts in `Idle`; no microphone stream exists or opens by default.
- `shared-runtime` v0.1.0, pinned to commit `6b87081b2da02560eb8edf098c3654e34b40b34c`, used only for the initial bounded non-audio runtime experiment.
- CPAL 0.18.2 compiles on Windows. `club-voice-device-check --list-devices` only enumerates devices; it never creates an audio stream and must not be treated as a microphone test.

## What does not exist yet

- The control prototype binds IPv4 localhost and accepts versioned join, leave and heartbeat datagrams. Run `cargo run --locked -p club-voice-server --example two_clients` to check password rejection, concurrent membership, duplicate join, capacity and slot reuse on Windows. The example uses independent sockets within one process. Keep its plaintext test protocol on loopback.
- GUI, LAN discovery, text transport, Opus, capture/playback streams, PTT, jitter buffer, RNNoise, packaging and internet transport are pending.
- A real two-client voice path. Do not represent the room-state tests as voice functionality.

## Non-negotiable constraints

- Never route audio frames through `shared-runtime`, UI state, histories, filesystem work, or generic request queues.
- Do not open capture or save/upload audio without an explicit user action.
- Keep media and control queues bounded, record drops/underruns, and discard media that misses its playout deadline.
- Do not change firewall rules, expose ports publicly, or claim LAN readiness proves internet security.
- Keep code, UI, and project documents in English; collaborate with the user in Russian.

## Next implementation sequence

1. Agree on the media datagram format with the Opus spike authors in GitHub issue #1. The Windows process smoke test starts one loopback server and two independent players and checks overlapping lobby membership, heartbeat replies and departure.
2. Add an Opus spike and a deterministic synthetic-audio test before real microphone capture.
3. Add an explicitly initiated CPAL capture/playback smoke path, with device-error and clean-shutdown handling.
4. Connect two independent clients through the local server and collect queue, underrun, delay, CPU/RAM, and game-impact receipts.
5. Add LAN discovery and packaging only after the direct-address voice path works.

## Commands run successfully

```powershell
cargo build --locked --workspace --examples
python crates/server/tests/process_smoke.py
cargo test --workspace --all-features
cargo test --workspace --no-default-features
cargo fmt --all --check
git diff --check
```

The two Cargo test configurations are intentional: `--all-features` validates the SDK and CPAL integrations, while `--no-default-features` is the initial SDK-excluded baseline.
