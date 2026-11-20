# Technical plan: low-impact voice vertical slice

Status: planning baseline, November 19, 2026. This document selects no production dependency yet. Each candidate must pass the stated spike and licensing checks before it is adopted.

## Objective

Deliver the smallest Windows-first proof that a local Club Voice server can connect two real clients and carry understandable live voice, while making resource use, game frame-time impact, and end-to-end voice delay measurable. The proof is not complete when text or a mock audio UI works.

## Non-negotiable boundaries

- LAN works without accounts or internet. Discovery has a direct-address fallback.
- Audio capture and microphone tests start only after an explicit user action. The application never silently records, saves, or uploads microphone audio.
- Audio frames bypass UI state, history stores, generic caches, and shared-runtime request queues.
- The first release has no managed hosting, global accounts, video, overlays, game injection, soundboards, or social features.
- Internet mode is planned on the same protocol boundary but is not inferred from LAN success. It needs separately qualified identity, encrypted control and media transport, admission/rate limits, and two-network validation.

## Performance contract

The product goal is imperceptible game impact, not an unsupported promise of zero CPU use, zero input lag, or zero network delay. The first vertical slice must therefore capture comparable receipts for idle, minimized, and active two-way voice:

- per-process and combined working set, CPU, handles, threads, and background wakeups;
- game average FPS, p1/p0.1 frame times, and input-latency evidence with and without the client;
- capture-to-playback delay on the local test rig, plus packet loss/jitter behavior;
- audio callback underruns, packet loss, queue depth, reconnect time, and device-change recovery.

No old media packet may be played after its playout deadline. Queue caps and explicit drop counters are required from the first audio path.

## Architecture boundary

```text
Explicit user action
        |
WASAPI capture -> bounded SPSC audio queue -> processing -> Opus encode
                                                        |
                                          low-latency transport datagrams
                                                        |
Server room relay -> per-peer jitter buffer -> decode -> WASAPI playback

UI / saved servers / bounded chat history / server snapshots
        |
shared-runtime SDK only where its lifecycle, shared-data, or request
coalescing semantics fit; never on the audio-frame path
```

The initial server relays room media rather than mixing it. This keeps server CPU predictable and preserves per-participant local volume control. The client audio engine owns capture, processing, encode/decode, bounded queues, jitter/playout, device lifecycle, and metrics; it must remain independent of the GUI event loop.

## Technical spikes and decision gates

### 1. Native UI and application lifecycle

Compare a native Rust UI candidate through a small static shell: join screen, room roster, speaking/mute indicators, settings entry, keyboard navigation, minimized idle behavior, and visible connection states. Reject a candidate that requires a browser runtime or makes ownership of the audio engine unclear. Measure process count, cold start, idle memory, and redraw/wakeup behavior before choosing.

### 2. Windows audio I/O

Start with `cpal` on its Windows WASAPI backend as the spike candidate. Its current documentation lists WASAPI as the Windows default backend and device/stream configuration support. Test default and selected capture/playback devices, sample-rate conversion, unplug/replug, denied/busy devices, callback underflow/overflow, and clean stop. Do not enable optional ASIO in the first slice because it adds separate installation/toolchain requirements.

### 3. Codec and playout

Use a maintained Opus binding to the reference codec as the initial comparison target; decide the exact crate only after license, Windows packaging, frame-size, and CPU tests. Establish a fixed initial frame duration, bounded jitter policy, sequence/timestamp format, loss behavior, and per-speaker volume path in a protocol note. The test baseline must include packet loss, reordering, and jitter rather than loopback alone.

### 4. Transport and security boundary

For LAN, spike a single authenticated room-control and media transport design that can later use established TLS/QUIC primitives; Quinn is a candidate because it supplies Rust QUIC streams and unreliable datagrams. Do not invent encryption. Direct LAN testing can use an explicitly trusted local identity, but the internet design must require certificate/server-identity verification and authenticated sessions. The transport spike must cap pre-auth work, connection attempts, control messages, and media queues.

### 5. Noise suppression

Keep suppression off for the first live-voice proof. Evaluate RNNoise only after voice is working, against fan noise, keyboard transients, nearby speech, quiet speech, and clipped consonants. The upstream library is BSD-3-Clause, but the licensing status of separately distributed pretrained weights has open upstream questions; no external model is bundled until its redistribution terms and hash are documented. Echo cancellation and automatic gain remain independent options with separate acceptance evidence.

### 6. Shared-runtime SDK evaluation

Create two behaviorally identical builds of the client shell:

- a reasonable ordinary-Rust baseline with bounded state, pagination, and explicit lifecycle ownership;
- an SDK-backed build that uses the released `tauri-multi-window-runtime` v0.1.0 only for applicable shared server snapshots, saved-server data, caches/coalescing, and lifecycle work.

Both builds must use the same UI, room protocol, codec, suppression setting, chat history limits, actions, and measurement workload. The experiment records linked-code footprint separately from runtime memory and may conclude improvement, regression, or no material difference. Audio frames are excluded from both the SDK and normal UI request queues.

## Implementation order

1. Create a Cargo workspace with separate `client`, `server`, and shared protocol/audio crates; add reproducible Windows CI, dependency/license inventory, and a no-capture smoke mode.
2. Build the server room/control path and two client shells with direct-address local connection, password admission, Lobby / Team A / Team B, bounded text/control packets, reconnection, and shutdown.
3. Add intentional capture/playback, Opus media relay, mute/deafen/PTT, queue caps, device recovery, and two-client voice acceptance. Do not grow chat before this passes.
4. Add measurement harnesses and record the first resource, frame-time, input-latency, and voice-delay receipts. Tune only against repeatable workloads.
5. Add LAN discovery, host UI, offline packaging, and firewall guidance; never change firewall settings automatically.
6. Qualify internet hosting separately, then conduct the SDK comparison under the frozen workload.

## Verification matrix for the first vertical slice

| Area | Required evidence |
| --- | --- |
| Live voice | Two independently running clients join the same local server, enter rooms, exchange live voice, use mute/deafen/PTT, reconnect, and shut down cleanly. |
| Capture consent | No audio device opens until the person presses a microphone-test or connect/voice action that explains capture. |
| Real-time safety | Bounded queues, drop/underrun counters, and no UI/history/disk work on the callback path. |
| Device failures | Missing, busy, unplugged, and changed devices result in a visible recoverable state. |
| Resource impact | A reproducible before/after report covers active game workload and all owned processes, not only the GUI process. |
| SDK | Same-workload comparison includes SDK-backed, reasonable baseline, and fully SDK-excluded build where needed. |
| Offline LAN | Two Windows PCs connect without internet, accounts, Docker, Rust, or terminal commands. |

## Open implementation-time decisions

- Final native UI library, after the shell spike and Windows packaging evidence.
- Exact Opus crate/binding and versions, after compatibility and license review.
- Packet format, initial playout target, capacity target, and numerical resource/latency budgets, after instrumented measurements on representative club PCs.
- Discovery protocol and host packaging details, after the direct-address/voice path is stable.

## Sources

- [CPAL Windows/WASAPI support](https://github.com/RustAudio/cpal) and its [0.18.2 change log](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md).
- [Quinn QUIC overview](https://quinn-rs.github.io/quinn/quinn.html).
- [rustls documentation](https://docs.rs/rustls/latest/rustls/).
- [RNNoise upstream repository](https://github.com/xiph/rnnoise) and [model-weight licensing issue](https://github.com/xiph/rnnoise/issues/284).
- Product constraints: `README.md`, `docs/PRODUCT.md`, and `docs/DELIVERY.md`.
