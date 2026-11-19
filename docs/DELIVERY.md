# Delivery milestones

This is a delivery order, not a claim that features are implemented or a promise of completion dates.

| Stage | Deliverable | Completion evidence | Status |
| --- | --- | --- | --- |
| 0 | Separate product boundary and LAN-first requirements | Independent repository and explicit scope/settings/noise/measurement contract | Recorded |
| 1 | Technical plan and bounded spikes | Select and verify Windows UI, device I/O, codec, suppression, transport and persistence; define capacity/budgets and licensing | Next |
| 2 | Useful server and text client | Two clients exchange actual messages, rooms/membership persist as intended, reconnect and limits work | Planned |
| 3 | First live voice path | Intentional device capture/playback, room transport, mute/deafen/PTT and clean shutdown on two clients | Planned |
| 4 | Voice quality and focused settings | Noise suppression, microphone test, device changes and validated optional processing; synthetic and permitted real-audio evidence | Planned |
| 5 | Club deployment experience | Host/start/stop UI, discovery, direct-address fallback, password admission, actionable network errors, offline portable packages | Planned |
| 6 | SDK integration and comparison | Same application behavior with and without SDK, reasonable baseline, owned-process resource/latency receipts | Planned |
| 7 | Club pilot and release | Two actual PCs, non-developer setup, voice usability and game-impact checks, packaging/security/licenses and reproducible results | Planned |

Implement coherent vertical slices and verify each before expanding scope. Automated codec/network tests do not establish microphone quality or a two-PC deployment. A human/device acceptance check remains necessary.

The shared-runtime release is already published. Do not move its tags or mix this product's release status with the SDK's. Publish this new product remotely only after its repository destination and visibility are chosen.
