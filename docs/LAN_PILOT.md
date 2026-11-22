# Club Voice LAN pilot for Windows x64

Extract the same ZIP on both PCs and connect them to the same trusted local network. The pilot uses unencrypted UDP and a temporary room password. Use it only on a trusted LAN.

## Start the server on PC 1

1. Connect your headset and select it as the default input and output in Windows.
2. Double-click `Host-LAN.cmd`. Find the IPv4 address of the active Ethernet or Wi-Fi adapter in the printed list. Enter that address followed by `:40771`, for example `192.168.1.42:40771`.
3. Enter a temporary room password of at least eight characters. Keep the server window open. If Windows prompts for network access, use the private network profile only.

## Join on both PCs

1. Connect and select the headset on PC 2 as well. `Check-Devices.cmd` checks format support without starting capture. `List-Devices.cmd` lists devices through the shared-runtime diagnostic.
2. Double-click `Join-LAN.cmd` on each PC. Enter the same server address and password and use different display names. Enter `y` to enable the microphone.
3. Check speech in both directions. Type `mute`, `unmute`, `deafen`, `undeafen` or `quit` and press Enter in either client window. Quit and rejoin once to verify that the seat is released.
4. Quit both clients and close the server window. Report any error text and the step at which it appeared.

If joining times out, check that both clients use the server's printed address and that the network allows access between the PCs. A password rejection appears as `DENIED`.

The pilot supports two seats. A device stream error ends the session. Reconnect by opening `Join-LAN.cmd` again. The LAN client uses the receiver's monotonic clock for playback queue age. Send-to-render measurements from the localhost probe require a shared clock.
