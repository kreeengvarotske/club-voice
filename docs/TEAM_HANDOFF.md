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

The [Windows LAN pilot](LAN_PILOT.md) adds direct private IPv4 joining for a two-PC hardware check. Build its ZIP using `./tools/package_lan_pilot.ps1`. The localhost commands below still provide the same-host timing baseline.

Build the release examples using the commands below. Start `target/release/examples/local_control.exe` and copy the printed loopback address. Each client takes one seat.

- Run `target/release/examples/headset_player.exe --check-devices` to check the selected input and output formats without joining a server or opening an audio stream. The probe accepts 48 kHz F32, I16 and U16 endpoints and prefers F32 when available.
- For a manual check, run `target/release/examples/headset_player.exe ADDRESS Ada --enable-microphone`. This activates the Windows default microphone and plays the other seat through the default output. Use headphones and start with low playback volume. Type `mute`, `unmute`, `deafen`, `undeafen` or `quit` followed by Enter. A device stream error ends the session and reports the cause.
- For a three-second generated peer, run `target/release/examples/headset_player.exe ADDRESS Lin --tone-peer`. Speak during this interval and check whether the tone peer reports received audio. Its output meter counts 20 ms blocks that cross an amplitude threshold. A human listening at both ends is still needed to judge speech quality.
- For unattended verification, run `python crates/server/tests/media_smoke.py`. It exercises the headset capture, codec, queue and playback paths with generated audio. A listener stays connected while two successive peers join under the same name. A separate case reuses a UDP port and sends delayed audio and leave packets from the former session. The client also has a localhost test for a dropped admission reply. It retries within the existing timeout, up to three attempts in total.
- For per-process CPU and memory on Windows, run `./crates/server/tests/windows_resources.ps1` in PowerShell 7. The script runs two synthetic peers, samples working set and private bytes once a second for 30 seconds, and reports CPU as a percentage of one logical core. Audio drivers, a GUI and the game are outside this workload.

Capture queues up to two 20 ms frames. Playback queues up to two batches. A batch contains a real frame and, after an isolated loss, its preceding PLC frame. The callback starts as soon as a batch is available. An 80 ms age check discards expired frames. Each client receives a single remote stream.

The tone peer reports `send_to_render_p95_ms` and `rendered_real_frames`. The interval starts at the sender's timestamp after encoding and ends at entry to the synthetic playback callback that first consumes the frame. A fixed histogram counts each played real frame once and rounds its age up to whole milliseconds. Read the percentile alongside drops and underruns because expired, concealed and unplayed frames are outside that distribution. The endpoints share a host clock. Capture, encoding, hardware buffers and physical output require separate measurements.

Run `./crates/server/tests/windows_resources.ps1 -Seconds 30 -Runs 3` for three independent trials. The JSON output identifies the server and both clients by run number and PID. Each client row includes send-to-render p95, sent, decoded and rendered frame counts, rejected playback batches caused by a full queue, and underrun samples from its full session, including startup and shutdown. CPU and memory come from the shorter sampling window.

The tone probe also reports `send_to_receive_p95_ms` using the packet age sampled before decoding. This includes all successfully decoded real frames, including frames later dropped by playback. The render percentile covers frames that reached the output callback. Both use the same send timestamp, but their populations can differ. The resource report uses null for receive p95 when an older build emits only render p95.

The tone probe reports `probe_missed_ticks` when the scheduler leaves it at least one full 20 ms period behind. It processes one block and resumes at the normal cadence instead of generating a burst of overdue blocks. Use this counter to identify runs affected by pauses in the load generator.

On November 22, four paired Windows trials compared release builds `73fa332` and `c0f35ee` using `windows_resources.ps1 -Seconds 10`. Each trial ran two 15-second tone peers. The build order reversed between pairs. Mean CPU per client across eight clients per build was 2.33% and 2.08% of one logical core. Sampled peak private memory was 1.50 and 1.44 MiB. Sampled peak working set was 8.32 and 7.23 MiB. Mean server CPU was 0.49% and 0.68%. Client send-to-render p95 values ranged from 8 to 13 ms on the baseline and from 5 to 22 ms on the newer build. The newer build rejected one batch due to a full queue across the series. The baseline rejected zero. These results cover a single host and the combined changes between those commits.

In five paired synthetic Windows trials per queue depth, increasing capacity from one to two batches reduced full-queue rejections from 21 to zero. The largest client p95 in the series went from 19 to 21 ms. In four additional paired trials, the synthetic callback skipped one 20 ms output block per second and requested both blocks on the next invocation. Full-queue rejections fell from 177 to 15 and the largest client p95 went from 21 to 24 ms. These trials used 10-second voice sessions with a 5-second CPU and memory sampling window. Peak private memory in the ordinary trials was 1.43 MiB for both depths.

In the 30-second queue comparison follow-up, neither client rejected a batch. Client p95 values were 19 and 22 ms, average CPU was 2.54% and 2.34% of one logical core, and sampled peak working set was 7.30 MiB each. Before accounting for silence already rendered during a gap, a smoke case with one deliberately dropped packet reported a p95 of 40 ms.

When the decoder recovers one missing frame, the output callback skips the portion of that PLC frame already covered by silence during an underrun. It can still play the remainder before the next real frame. The `concealed` count reports decoder operations. The `plc_rendered_samples` counter reports samples actually written to the output buffer from those frames, independent of channel count. Divide it by 48 for milliseconds at 48 kHz. A subsequent smoke run rendered 149 real frames and reported a p95 of 21 ms after one deliberately dropped packet.

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
