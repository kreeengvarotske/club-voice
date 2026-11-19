# Product requirements

Status: product definition, November 19, 2026. Proposed technical choices and performance targets require evidence before being described as verified.

## Purpose and users

The primary product is a lightweight gaming voice client whose main advantage is rapid LAN deployment in PC clubs. The club administrator hosts; players join and talk while gaming. A local server and real client-to-client communication were explicitly selected over an autonomous history-only demonstration.

The application is a separate product from the released shared-runtime SDK. The SDK's release source, compatibility policy and frozen performance research are unchanged.

## Requirements

### R1: Club deployment

An administrator can start and stop a local server through a simple interface, choose a name and join password, see the usable LAN address and connected clients, and save a working configuration.

Players find the server on the LAN and join with a local display name and the join password. Direct address entry remains available when discovery is blocked. No cloud account or working internet connection is required for the LAN path.

The first-run flow explains OS firewall permission and network reachability failures. It does not silently change firewall rules, expose internet ports, install a system service or require a terminal. Club VLANs and network policy may prevent discovery or connectivity; the product must report this accurately.

Client and server must ship with required runtime assets. Build-time dependency/model downloads do not justify requiring internet access at first use. Final binary/runtime requirements must be documented and tested on a clean supported Windows machine.

### R2: Useful gaming voice

Players can create/join permitted rooms, see membership/speaking/mute states, use push-to-talk or voice activation, mute/deafen themselves, adjust another participant's volume, and choose input/output devices.

Joining, leaving, reconnecting and a missing/unplugged device have visible, recoverable states. The first functional milestone is two clients exchanging live voice through the local server; a text-only screen is not completion.

Background hotkeys use normal supported OS facilities. Game injection and an overlay are not first-release requirements.

### R3: Noise suppression and microphone settings

Noise suppression runs locally and supports an explicit off state. Include input/output device selection, microphone and playback levels, activation sensitivity, push-to-talk binding, a microphone meter and an intentional microphone test.

Evaluate automatic gain and echo cancellation as separate optional controls, exposing only settings whose implementation and behavior are validated. A headset user should not need to understand codec internals.

Evaluate fan noise, keyboard transients, nearby voices, quiet speech and clipped consonants. Nearby human speech is not assumed to be removable by a generic noise suppressor. Assess speech preservation and processing delay/CPU alongside noise attenuation.

RNNoise is an initial candidate, not a selected implementation or a claim of Discord/Krisp-equivalent quality. Bundle any selected model and check its redistribution terms, integrity and offline behavior. Do not require a particular GPU or a cloud processing service.

Microphone capture/testing must be initiated by the user. Do not silently record, save or upload microphone audio for diagnostics.

### R4: Focused settings and interface

Provide concise settings for voice/devices, hotkeys, connection/club hosting, startup/tray behavior and essential notifications. Use English for product UI, source and documentation.

Keep keyboard navigation and clear connection/microphone states. Hidden/minimized UI must avoid unnecessary animation/redraw. Experimental SDK comparison switches belong in developer tooling, not player settings.

Soundboards, avatar shops, bots, social feeds, video/streaming, recording, cosmetic effects and a plugin marketplace are outside the first release.

### R5: Lightweight text

Provide actual text-channel messaging, bounded history, unread state and reconnect behavior alongside voice. Avoid loading an entire archive into every window. Large attachments, rich media galleries and advanced community administration are later candidates rather than prerequisites for LAN voice.

### R6: Resource behavior

Bound memory retained by histories, caches, packet queues and background tasks. Audio work must be isolated from interface work, history queries, filesystem activity and ordinary application data queues.

Old voice packets must not accumulate into an ever-growing delay. Disconnecting and closing rooms/windows must release their owned state. Startup, idle, minimized, text loading and active voice must be measured separately.

Count processes together with total owned client memory, CPU, background activity and game frame-time effects. Record host-server cost separately and include it when evaluating the hosting PC. A low process count alone is not proof of a lightweight application.

Native Rust UI is the current recommendation to avoid WebView2 helper processes. Exact process/memory/FPS/latency claims await implementation and measurement. No promise of zero lag is made for arbitrary hardware, drivers, networks or games.

### R7: Responsible local hosting

A join password controls admission but is not a substitute for transport protection. Use established security protocols/libraries; do not invent custom encryption. Bound unauthenticated work, connection attempts, requests and media queues.

Show that a server is running and which interface/address it uses. Internet-facing hosting and its authentication, abuse and deployment requirements need separate qualification; LAN readiness does not establish internet readiness.

### R8: Honest SDK evaluation

Use the SDK only where its shared-data, coalescing/cache or lifecycle services fit. Keep audio frames outside general-purpose caching.

Compare useful ordinary Rust application behavior against SDK-backed behavior using the same UI, server snapshot, voice codec, suppression settings, histories, request limits and actions. The baseline must use reasonable pagination, bounded structures and ordinary sharing; do not manufacture waste to make the SDK win.

Include a fully SDK-excluded build when needed to separate linked-code footprint from runtime behavior. Report improvements, regressions and no difference. Do not attribute a native-interface advantage to the SDK.

## Acceptance and unresolved details

- First vertical slice: local host plus two real clients exchange text and voice, can mute/PTT, reconnect and shut down cleanly.
- Club pilot: a non-developer can host and join from at least two Windows PCs without internet, accounts, toolchains or terminal commands, with clear firewall/discovery guidance.
- Noise evaluation: representative club noise and speech samples, acquired with permission, demonstrate understandable speech and acceptable processing cost.
- Gaming evaluation: record hardware, workload, active speakers and game conditions before making latency/resource/frame-time claims.
- Packaging: test offline first run and retain dependency/model licenses and hashes.
- Capacity, supported Windows versions/hardware, concrete audio/GUI/transport libraries and numerical performance budgets are not settled. Choose and measure them during technical planning and the pilot, then freeze the comparison workload before benefit claims.

## References

- [Released SDK](https://github.com/kreeengvarotske/tauri-multi-window-runtime/releases/tag/v0.1.0)
- [Tauri process model](https://v2.tauri.app/concept/process-model/)
- [Microsoft WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model)
- [Xiph RNNoise](https://github.com/xiph/rnnoise)
