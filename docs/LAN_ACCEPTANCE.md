# Club Voice LAN acceptance in one visit

This run qualifies the two-seat LAN pilot for the hardware and OS pair tested. Use the same commit on both endpoints. The server runs on the Windows PC. A Mac can be the second client after its native build and device precheck pass. The pilot uses unencrypted UDP on a trusted local network.

## Before the visit

1. Choose the two PCs, headsets and trusted network. Record the Windows version and macOS version if applicable. Use a network that allows clients to reach one another over UDP.
2. Build the Windows ZIP with `./tools/package_lan_pilot.ps1`. If using a Mac as the second client, build its ZIP on that Mac with `bash tools/package_macos_lan_pilot.sh` from the same commit. The Mac build needs Rust, the native build tools and CMake. Keep the compiled ZIP for the visit.
3. Extract the ZIPs and run `Check-Devices.cmd` on Windows or `Check-Devices.command` on Mac. Save both format reports. The live probe needs a compatible 48 kHz input and output. If an endpoint fails, choose another compatible device and rerun the check before travel.
4. If game-impact measurements will be part of the visit, prepare a repeatable game replay and verify the capture tool on the gaming PC in advance. Keep the raw files and note the build and settings.

## During the visit

Use these steps in order. On Windows run the named `.cmd` files. On Mac use the equivalent `.command` launchers for the second client.

| Check | Action | Pass evidence |
| --- | --- | --- |
| 1. Start the host | Run `Host-LAN.cmd` on Windows. Enter the active private IPv4 address followed by `:40771` and a temporary password. | The server prints `LAN pilot listening on ...`. |
| 2. Automated duplex | On the host, run `Play-Test-Tone.cmd`. On the other PC, run `Check-LAN` after the host prints `READY`. Enter the address and password. | PC 2 prints `PASS: LAN check complete`. PC 1 reports `streams_heard=2` at the end of the two-minute tone run. Save both summaries. This covers password rejection, synthetic audio, confirmed departure and rejoin. |
| 3. Live speech | After the tone peer finishes, run `Join-LAN` on both PCs. Confirm microphone use explicitly and speak using both headsets. | Both people hear intelligible speech. Note any persistent gaps, clipping or echo. |
| 4. Controls | With active speech, try `mute` and `unmute` on one PC. Try `deafen` and `undeafen` on the other. | Each control takes effect in the expected direction and speech returns after undo. |
| 5. Recover from device removal | Disconnect the active headset from PC 2 during speech. Note what the client reports. Reconnect the headset and rejoin if the session ended. | The client reports the failure or recovers. The headset is usable after rejoin, and speech works both ways. |
| 6. Exit and rejoin | Enter `quit` on PC 2. Reopen `Join-LAN` with the same display name. | The seat is available, the client joins and both directions work again. |
| 7. Stop host | Stop the server while both clients are connected. After clients report the disconnect, restart the server and rejoin. | Both clients exit with a disconnect error or report a session timeout. After restart both can join and exchange voice. Close the clients and server at the end. |

If step 2 fails, check the printed address, confirm both devices use the same network and verify private-profile access on the host. Save the complete error and rerun the failed step after correcting the cause. Include the failed result alongside the retry.

## Record before leaving

- Commit, OS and architecture on both PCs, headset models, input/output formats and whether the network was wired or wireless.
- Pass, fail or blocked for each row. Save the two automatic test outputs. Include the first failing step, exact error and whether it reproduced. Leave the password and any audio recordings out of the report.
- If you ran the game comparison, include the raw frame-time captures and CPU/memory report. If you measured physical audio or input latency, record the measurement method and results.
- Call LAN acceptance complete when all seven rows pass for that OS pair. Document any checks you deferred with the reason.
