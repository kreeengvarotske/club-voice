# Club Voice

A planned modern, voice-first gaming client with simple LAN hosting for PC clubs.

**Primary outcome:** a club administrator starts a local server and players join from the same LAN without cloud accounts, an internet connection, Docker, a development toolchain or terminal commands.

Working title only. This is a separate local repository from tauri-multi-window-runtime. Current status: product requirements and delivery milestones; the client/server and audio implementation do not exist yet.

## Product direction

Keep the compact voice-first workflow that motivates the TeamSpeak 3 reference, with a modern, channel-oriented interface inspired by Discord. Build an original visual identity and retain only functions that help players communicate while gaming.

The main differentiator is club deployment: start one local server, let players find it, and join a team room with minimal setup. Initial room templates include Lobby, Team A and Team B.

## Connection modes

- **Club / LAN:** start a local server, discover it on the same network, and join without internet or a cloud account.
- **Internet:** join a remotely hosted server by invitation or address and keep saved servers in the same client. The same server software can run on a reachable VPS or dedicated machine.

Simple club deployment remains the main differentiator; internet voice/text is also a first-release requirement. A managed cloud hosting service and global account system are separate future decisions, not prerequisites for self-hosted internet use.

## First release scope

- Voice rooms, push-to-talk and voice activation, mute/deafen, device selection and per-participant volume.
- Local noise suppression and an explicit microphone test, with speech quality and processing cost evaluated together.
- Lightweight text channels, bounded history, unread state and reliable reconnect.
- Club hosting, LAN discovery, direct-address fallback, a join password and visible server status.
- Internet server connections, invitations, saved servers and reconnect, with independently qualified transport security and authentication.
- A portable distribution and simple first-run flow for club administrators and players.

The intended native Rust client avoids a browser-runtime dependency; concrete GUI and audio libraries are not selected yet. Minimal background activity, voice reliability and measured game impact take priority over decorative features.

The released shared-runtime SDK may be consumed as a dependency for appropriate shared application data. Voice frames must not pass through data caches or UI request queues. An internal with/without-SDK comparison must retain the same useful application behavior; no memory benefit is assumed.

See [product requirements](docs/PRODUCT.md) and [delivery milestones](docs/DELIVERY.md).

Public repository: https://github.com/kreeengvarotske/club-voice . The repository currently contains requirements and delivery milestones; no application release is available.
