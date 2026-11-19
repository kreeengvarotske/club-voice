# Club Voice

A planned lightweight gaming voice client with a local-first deployment experience for PC clubs.

**Primary outcome:** a club administrator starts a local server and players join from the same LAN without cloud accounts, an internet connection, Docker, a development toolchain or terminal commands.

Working title only. This is a separate local repository from tauri-multi-window-runtime. Current status: product requirements and delivery milestones; the client/server and audio implementation do not exist yet.

## First release scope

- Voice rooms, push-to-talk and voice activation, mute/deafen, device selection and per-participant volume.
- Local noise suppression and an explicit microphone test, with speech quality and processing cost evaluated together.
- Lightweight text channels, bounded history, unread state and reliable reconnect.
- Club hosting, LAN discovery, direct-address fallback, a join password and visible server status.
- A portable distribution and simple first-run flow for club administrators and players.

The intended native Rust client avoids a browser-runtime dependency; concrete GUI and audio libraries are not selected yet. Minimal background activity, voice reliability and measured game impact take priority over decorative features.

The released shared-runtime SDK may be consumed as a dependency for appropriate shared application data. Voice frames must not pass through data caches or UI request queues. An internal with/without-SDK comparison must retain the same useful application behavior; no memory benefit is assumed.

See [product requirements](docs/PRODUCT.md) and [delivery milestones](docs/DELIVERY.md).

No remote repository has been created or published.
