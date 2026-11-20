# Team handoff: current implementation baseline

Updated November 20, 2026.

## What exists

- A Rust workspace with `club-voice-protocol`, `club-voice-server`, and `club-voice-client` crates.
- Default Lobby / Team A / Team B room state, password admission, and a server member limit.
- A bounded control-plane queue with observable drops. It is not an audio queue.
- A client capture gate that starts in `Idle`; no microphone stream exists or opens by default.
- `shared-runtime` v0.1.0, pinned to commit `6b87081b2da02560eb8edf098c3654e34b40b34c`, used only for the initial bounded non-audio runtime experiment.
- CPAL 0.18.2 compiles on Windows. `club-voice-device-check --list-devices` only enumerates devices; it never creates an audio stream and must not be treated as a microphone test.

- The control prototype binds IPv4 localhost and accepts versioned join, leave and heartbeat datagrams. Run `cargo run --locked -p club-voice-server --example two_clients` to check password rejection, concurrent membership, duplicate join, capacity and slot reuse on Windows. The example uses independent sockets within one process. Keep its plaintext test protocol on loopback.
- Two Rust client processes exchange generated Opus through the relay, with one sender in the lobby. The receiver checks sequence continuity, decoded length and the level of three tones. Each comparison uses the same two seconds of 48 kHz mono audio. The timestamps cover the interval from sending an encoded packet to finishing its decode on the same host. Device buffers, frame collection, encoding and playback add further delay in a voice client.

## Pending

Live capture and playback, multiple concurrent speakers, PTT, jitter buffering, LAN discovery, GUI, noise suppression, packaging and internet transport still need implementation and qualification.

## Non-negotiable constraints

- Never route audio frames through `shared-runtime`, UI state, histories, filesystem work, or generic request queues.
- Do not open capture or save/upload audio without an explicit user action.
- Keep media and control queues bounded, record drops/underruns, and discard media that misses its playout deadline.
- Do not change firewall rules, expose ports publicly, or claim LAN readiness proves internet security.
- Keep code, UI, and project documents in English; collaborate with the user in Russian.

## Next implementation sequence

1. Add speaker identity before enabling multiple senders. The experimental CVM1 datagram carries frame duration, sequence, a same-host timing marker and the Opus payload.
2. Test bounded playout under jitter, packet loss and reordering with the same synthetic source.
3. Add an explicitly initiated CPAL capture/playback smoke path, with device-error and clean-shutdown handling.
4. Connect two independent clients through the local server and collect queue, underrun, delay, CPU/RAM, and game-impact receipts.
5. Add LAN discovery and packaging only after the direct-address voice path works.

## Commands run successfully

The media example builds bundled libopus and requires CMake and the MSVC C++ build tools. If CMake is bundled with Visual Studio but missing from PATH, set the CMAKE environment variable to its executable for this shell.

```powershell
cargo build --locked --workspace --examples
python crates/server/tests/process_smoke.py
cargo build --release --locked --workspace --examples --features media-smoke
python crates/server/tests/media_smoke.py
cargo test --workspace --all-features
cargo test --workspace --no-default-features
cargo fmt --all --check
git diff --check
```

The full feature build checks the SDK, CPAL and codec integrations. For SDK performance comparisons, hold all other features fixed. The minimal build also removes audio features.
