# Club Voice LAN pilot for Windows x64

Extract the same ZIP on both PCs and connect them to the same trusted local network. The pilot uses unencrypted UDP and a temporary room password. Use it only on a trusted LAN.

Use [the LAN acceptance checklist](LAN_ACCEPTANCE.md) to prepare both machines and run every check in one visit. The ZIP includes the checklist as `Acceptance.md`.

## Start the server on PC 1

1. Connect your headset and select it as the default input and output in Windows.
2. Double-click `Host-LAN.cmd`. Find the IPv4 address of the active Ethernet or Wi-Fi adapter in the printed list and enter it, for example `192.168.1.42`. The host and clients use port 40771 when you omit it. If you choose a different port, use the full address printed by the host on both clients.
3. Enter a temporary room password of at least eight characters. Keep the server window open. If Windows prompts for network access, use the private network profile only.

## Run the automatic check

1. On PC 1, start the server and double-click `Play-Test-Tone.cmd`. Enter the server address, a display name and the room password. This peer runs for two minutes.
2. On PC 2, connect the headset and double-click `Check-LAN.cmd`. Enter the server address and password once. The check reports device formats, rejects a wrong password, runs two synthetic voice sessions and verifies leave acknowledgements and a new session ID.
3. Check for `PASS: LAN check complete` on PC 2. The test peer on PC 1 prints its received audio summary when it finishes. Both commands use generated audio and keep audio devices closed.

## Join on both PCs for a live call

1. Connect and select the headset on PC 2 as well. `Check-Devices.cmd` checks format support without starting capture. `List-Devices.cmd` lists devices through the shared-runtime diagnostic.
2. Double-click `Join-LAN.cmd` on each PC. Enter the same server address and password and use different display names. Enter `y` to enable the microphone.
3. Check speech in both directions. Type `mute`, `unmute`, `deafen`, `undeafen` or `quit` and press Enter in either client window. Quit and rejoin once to verify that the seat is released.
4. Quit both clients and close the server window. Report any error text and the step at which it appeared.

If joining times out, check that both clients use the server's printed address and that the network allows access between the PCs. A password rejection appears as `DENIED`.

The pilot supports two seats. A device stream error ends the session. Reconnect by opening `Join-LAN.cmd` again. The LAN client uses the receiver's monotonic clock for playback queue age. Send-to-render measurements from the localhost probe require a shared clock.
