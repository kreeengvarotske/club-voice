# Product requirements

Status: product definition, November 19, 2026. Proposed technical choices and performance targets require evidence before being described as verified.

## Purpose and users

The primary product is a lightweight gaming voice client whose main advantage is rapid LAN deployment in PC clubs. The club administrator hosts; players join and talk while gaming. A local server and real client-to-client communication were explicitly selected over an autonomous history-only demonstration.

The application is a separate product from the released shared-runtime SDK. The SDK's release source, compatibility policy and frozen performance research are unchanged.

## Product identity and references

TeamSpeak 3 is a reference for compact, voice-first gaming workflows. Discord is a reference for modern visual hierarchy and familiar channel navigation. These are design inputs, not verified comparative performance claims or a requirement to reproduce either product's entire feature set.

Players should quickly see their server, team room, participants, speaking/mute state and connection health. Keep microphone, mute/deafen and disconnect controls consistently accessible. Provide a modern original interface with clear text/channel hierarchy and restrained background activity.

Rapid local deployment is the primary differentiator. The first club flow offers a ready-made Lobby / Team A / Team B layout that an administrator can adjust. Advanced community/social features must not delay a usable voice client and local host flow.

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

### R7: Responsible network hosting

A join password controls admission but is not a substitute for transport protection. Use established security protocols/libraries; do not invent custom encryption. Bound unauthenticated work, connection attempts, requests and media queues.

Show that a server is running and which interface/address it uses. Internet-facing hosting is required by R9, with separately verified authentication, transport protection, abuse limits and deployment. LAN readiness does not establish internet readiness. Local hosting must not silently become publicly accessible.

### R8: Honest SDK evaluation

Use the SDK only where its shared-data, coalescing/cache or lifecycle services fit. Keep audio frames outside general-purpose caching.

Compare useful ordinary Rust application behavior against SDK-backed behavior using the same UI, server snapshot, voice codec, suppression settings, histories, request limits and actions. The baseline must use reasonable pagination, bounded structures and ordinary sharing; do not manufacture waste to make the SDK win.

Include a fully SDK-excluded build when needed to separate linked-code footprint from runtime behavior. Report improvements, regressions and no difference. Do not attribute a native-interface advantage to the SDK.

### R9: Internet communication in the same client

Players can use voice rooms and text channels with people on other networks, not only on the same LAN. Keep the same client, room semantics, essential voice settings and reconnect behavior in both modes.

A server operator can run the same server software on a reachable remote machine such as a VPS or dedicated host. Players join by a shareable invitation or address, save servers and switch between local and remote servers. Invitations must carry enough server location information to work without an unspecified global invite-code lookup service.

Ordinary joining players should not need to host a server, edit a router or configure port forwarding. Remote operators still need a reachable host and documented endpoint/certificate/network setup. Do not claim that any home PC becomes internet-reachable automatically.

Internet mode requires authenticated sessions, encrypted control and media transport, server identity verification, bounded unauthenticated work, admission/rate limits and usable connection errors. Validate invitation expiry/revocation and basic operator removal/admission controls. Choose established protocols and libraries during technical planning; do not invent crypto.

LAN discovery remains local and does not publish a club server to an internet directory. Managed hosting, centralized accounts, a public server directory and their operational costs/privacy rules are separate future decisions. Self-hosted internet communication must not depend on those services.

## Acceptance and unresolved details

- First vertical slice: local host plus two real clients exchange text and voice, can mute/PTT, reconnect and shut down cleanly.
- Club pilot: a non-developer can host and join from at least two Windows PCs without internet, accounts, toolchains or terminal commands, with clear firewall/discovery guidance.
- Internet pilot: two clients on independent networks exchange voice and text through a reachable server, reconnect, validate admission/server identity and transport protection, and shut down cleanly. A loopback or LAN-only test does not satisfy this requirement.
- Noise evaluation: representative club noise and speech samples, acquired with permission, demonstrate understandable speech and acceptable processing cost.
- Gaming evaluation: record hardware, workload, active speakers and game conditions before making latency/resource/frame-time claims.
- Packaging: test offline first run and retain dependency/model licenses and hashes.
- Capacity, supported Windows versions/hardware, concrete audio/GUI/transport libraries and numerical performance budgets are not settled. Choose and measure them during technical planning and the pilot, then freeze the comparison workload before benefit claims.

## References

- [Released SDK](https://github.com/kreeengvarotske/tauri-multi-window-runtime/releases/tag/v0.1.0)
- [Tauri process model](https://v2.tauri.app/concept/process-model/)
- [Microsoft WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model)
- [Xiph RNNoise](https://github.com/xiph/rnnoise)
