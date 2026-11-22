# Team handoff: current implementation baseline

Updated November 20, 2026.

## What exists

- A Rust workspace with `club-voice-protocol`, `club-voice-server`, and `club-voice-client` crates.
- Default Lobby / Team A / Team B room state, password admission, and a server member limit.
- A bounded control-plane queue with observable drops. It is not an audio queue.
- The headset example requires explicit microphone activation. The automated duplex test uses a generated signal.
- The SDK-backed device diagnostic reads a shared inventory snapshot through `shared-runtime` v0.1.0. The metadata operation has a 32 KiB cache budget and a two-second TTL. A fixture test makes concurrent picker and settings requests and verifies one enumeration and one shared result.
- CPAL 0.18.2 compiles on Windows. `club-voice-device-check --list-devices` only enumerates devices; it never creates an audio stream and must not be treated as a microphone test.

- The control prototype binds IPv4 localhost and accepts versioned join, leave and heartbeat datagrams. Run `cargo run --locked -p club-voice-server --example two_clients` to check password rejection, concurrent membership, duplicate join, capacity and slot reuse on Windows. The example uses independent sockets within one process. Keep its plaintext test protocol on loopback.
- Two Rust client processes exchange generated Opus through the relay, with one sender in the lobby. The receiver checks sequence continuity, decoded length and the level of three tones. Each comparison uses the same two seconds of 48 kHz mono audio. The timestamps cover the interval from sending an encoded packet to finishing its decode on the same host. Device buffers, frame collection, encoding and playback add further delay in a voice client.

## Pending

Physical headset acceptance, rooms with more than two participants, PTT, jitter buffering, LAN discovery, GUI, noise suppression, packaging and internet transport remain open.

## Two-seat headset check on Windows

Build the release examples using the commands below. Start `target/release/examples/local_control.exe` and copy the printed loopback address. Each client takes one seat.

- For a manual check, run `target/release/examples/headset_player.exe ADDRESS Ada --enable-microphone`. This activates the Windows default microphone and plays the other seat through the default output. The current device adapter requires 48 kHz float input and output. Use headphones and start with low playback volume. Type `mute`, `unmute`, `deafen`, `undeafen` or `quit` followed by Enter. A device stream error ends the session and reports the cause.
- For a three-second generated peer, run `target/release/examples/headset_player.exe ADDRESS Lin --tone-peer`. Speak during this interval and check whether the tone peer reports received audio. Its output meter counts 20 ms blocks that cross an amplitude threshold. A human listening at both ends is still needed to judge speech quality.
- For unattended verification, run `python crates/server/tests/media_smoke.py`. It exercises the headset capture, codec, queue and playback paths with generated audio. A listener stays connected while two successive peers join under the same name. A separate case reuses a UDP port and sends delayed audio and leave packets from the former session.
- For per-process CPU and memory on Windows, run `./crates/server/tests/windows_resources.ps1` in PowerShell 7. The script runs two synthetic peers, samples working set and private bytes once a second for 30 seconds, and reports CPU as a percentage of one logical core. Audio drivers, a GUI and the game are outside this workload.

Capture queues up to two 20 ms frames. Playback queues one batch containing a real frame and, after an isolated loss, its preceding PLC frame. An 80 ms age check discards expired frames. Each client receives a single remote stream.

The tone peer reports `send_to_render_p95_ms` and `rendered_real_frames`. The interval starts at the sender's timestamp after encoding and ends at entry to the synthetic playback callback that first consumes the frame. A fixed histogram counts each played real frame once and rounds its age up to whole milliseconds. Read the percentile alongside drops and underruns because expired, concealed and unplayed frames are outside that distribution. The endpoints share a host clock. Capture, encoding, hardware buffers and physical output require separate measurements.

Run `./crates/server/tests/windows_resources.ps1 -Seconds 30 -Runs 3` for three independent trials. The JSON output identifies the server and both clients by run number and PID. Each client row includes send-to-render p95, sent, decoded and rendered frame counts, and underrun samples from its full session, including startup and shutdown. CPU and memory come from the shorter sampling window.

When a peer rejoins, the listener resets its Opus decoder and discards queued or partially played audio from the previous session. Within one stream it immediately conceals one missing 20 ms frame through Opus PLC, skips duplicate and late frames, and resets the decoder after a longer burst instead of generating an unbounded recovery backlog. The client sends a heartbeat every five seconds and ends the session after fifteen seconds without forwarded media or a heartbeat reply.

To use two different headsets on one PC, run `cargo run --release -p club-voice-client --bin club-voice-device-check -- --list-devices`. In each player's PowerShell window, set `$env:CLUB_VOICE_INPUT_ID` and `$env:CLUB_VOICE_OUTPUT_ID` to the corresponding printed IDs before starting the headset example. Omit either variable to use the Windows default for that direction.

On November 21, a 30-second Windows release sample with two synthetic duplex clients using CVM2 and the bounded PLC path reported 8.70 MiB sampled peak working set, 1.45 and 1.46 MiB private bytes, and 1.84% and 1.38% average CPU relative to one logical core. The server used 5.79 MiB working set, 0.84 MiB private bytes and 0.36% of one core. The same build measured same-host send-to-decode p95 of 257 µs with 10 ms frames and 329 µs with 20 ms frames. Game frame time, input latency and physical mouth-to-ear delay still need hardware measurements.

## Non-negotiable constraints

- Never route audio frames through `shared-runtime`, UI state, histories, filesystem work, or generic request queues.
- Do not open capture or save/upload audio without an explicit user action.
- Keep media and control queues bounded, record drops/underruns, and discard media that misses its playout deadline.
- Do not change firewall rules, expose ports publicly, or claim LAN readiness proves internet security.
- Keep code, UI, and project documents in English; collaborate with the user in Russian.

## Next implementation sequence

1. Keep the two-seat limit until each concurrent speaker has independent decoder and playback state. CVM2 carries a server-issued session ID, frame duration, sequence, a same-host timing marker and the Opus payload. A replacement peer gets a new ID even when its display name and UDP port are reused.
2. Test bounded playout under jitter, packet loss and reordering with the same synthetic source.
3. Run the manual headset check and document device names, formats, heard speech, mute/deafen, unplug and shutdown outcomes with the tester's permission.
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
