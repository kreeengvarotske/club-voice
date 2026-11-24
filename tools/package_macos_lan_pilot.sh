#!/usr/bin/env bash
# Run from the repository root on the Mac that will be used for the LAN pilot.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    printf '%s\n' 'Build the macOS LAN pilot on the target Mac.' >&2
    exit 1
fi

target="${CARGO_TARGET_DIR:-target}"
bundle="$target/club-voice-macos-lan-pilot"
cargo build --release --locked --target-dir "$target" -p club-voice-client \
    --no-default-features --features audio-device-probe,media-smoke --example headset_player

mkdir -p "$bundle"
cp "$target/release/examples/headset_player" "$bundle/headset_player"
cp docs/LAN_ACCEPTANCE.md "$bundle/Acceptance.md"

write_launcher() {
    local name="$1" flag="$2"
    printf '%s\n' '#!/bin/bash' 'cd -- "$(dirname -- "$0")" || exit 1' \
        'export CLUB_VOICE_PROBE_SECONDS=3' "exec ./headset_player $flag" > "$bundle/$name"
    chmod +x "$bundle/$name"
}

write_launcher Check-Devices.command --check-devices
write_launcher Check-LAN.command --check-lan
write_launcher Join-LAN.command --join-lan

# Archive explicit paths so stale local files cannot enter the handoff.
(
    cd "$target"
    zip -q -FS club-voice-macos-lan-pilot.zip \
        club-voice-macos-lan-pilot/{headset_player,Acceptance.md,Check-Devices.command,Check-LAN.command,Join-LAN.command}
)
printf '%s\n' "$bundle.zip"
