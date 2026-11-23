# Build with PowerShell 7 from the repository root on Windows x64.
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Build the Windows LAN pilot on Windows.' }

cargo build --release --locked --workspace --examples --bins --features media-smoke
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$release = Join-Path $target 'release'
$bundle = Join-Path $target 'club-voice-lan-pilot'
New-Item -ItemType Directory -Force -Path $bundle | Out-Null
Copy-Item (Join-Path $release 'examples/local_control.exe') $bundle
Copy-Item (Join-Path $release 'examples/headset_player.exe') $bundle
Copy-Item (Join-Path $release 'club-voice-device-check.exe') $bundle
Copy-Item 'docs/LAN_PILOT.md' (Join-Path $bundle 'README.txt')
Copy-Item 'docs/LAN_ACCEPTANCE.md' (Join-Path $bundle 'Acceptance.md')

function Write-Launcher([string]$Name, [string[]]$Commands) {
    $lines = @('@echo off', 'chcp 65001 >nul') + $Commands + @('echo.', 'pause')
    $lines | Set-Content -Encoding ascii -LiteralPath (Join-Path $bundle $Name)
}

Write-Launcher 'Host-LAN.cmd' @('ipconfig', '"%~dp0local_control.exe" --lan')
Write-Launcher 'Join-LAN.cmd' @('"%~dp0headset_player.exe" --join-lan')
Write-Launcher 'Play-Test-Tone.cmd' @('set CLUB_VOICE_PROBE_SECONDS=120', '"%~dp0headset_player.exe" --join-lan --tone-peer')
Write-Launcher 'Check-LAN.cmd' @('set CLUB_VOICE_PROBE_SECONDS=3', '"%~dp0headset_player.exe" --check-lan')
Write-Launcher 'Check-Devices.cmd' @('"%~dp0headset_player.exe" --check-devices')
Write-Launcher 'List-Devices.cmd' @('"%~dp0club-voice-device-check.exe" --list-devices')

$files = @(
    'local_control.exe', 'headset_player.exe', 'club-voice-device-check.exe', 'README.txt', 'Acceptance.md',
    'Host-LAN.cmd', 'Join-LAN.cmd', 'Play-Test-Tone.cmd', 'Check-LAN.cmd',
    'Check-Devices.cmd', 'List-Devices.cmd'
) | ForEach-Object { Join-Path $bundle $_ }
$zip = "$bundle.zip"
Compress-Archive -LiteralPath $files -DestinationPath $zip -Force
(Resolve-Path -LiteralPath $zip).Path
