# Run with PowerShell 7 after building the release examples with media-smoke.
param([ValidateRange(5, 120)][int]$Seconds = 30)
$ErrorActionPreference = "Stop"
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }
$examples = Resolve-Path (Join-Path $targetDir "release/examples")
$owned = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
function Start-Probe([string]$Binary, [string[]]$Arguments) {
    $info = [System.Diagnostics.ProcessStartInfo]::new((Join-Path $examples "$Binary.exe"))
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.Environment["CLUB_VOICE_PROBE_SECONDS"] = [string]($Seconds + 5)
    foreach ($item in $Arguments) { $info.ArgumentList.Add($item) }
    $process = [System.Diagnostics.Process]::Start($info)
    $owned.Add($process)
    return $process
}
function Read-Ready($Process) {
    $line = $Process.StandardOutput.ReadLineAsync()
    if (-not $line.Wait(5000)) { throw "Probe startup timed out" }
    return $line.Result
}
try {
    $server = Start-Probe "local_control" @()
    $address = Read-Ready $server
    if ($address -notmatch '^127\.0\.0\.1:\d+$') { throw "Unexpected server address" }
    foreach ($name in "Ada","Lin") {
        $client = Start-Probe "headset_player" @($address,$name,"--tone-peer")
        if ((Read-Ready $client) -ne "READY") { throw "Client failed to join" }
    }
    Start-Sleep -Seconds 1
    $samples = foreach ($p in $owned) {
        $p.Refresh()
        [pscustomobject]@{Process=$p; StartCpu=$p.TotalProcessorTime.TotalMilliseconds; MaxPrivate=$p.PrivateMemorySize64; MaxWorking=$p.WorkingSet64}
    }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    for ($i=0; $i -lt $Seconds; $i++) {
        Start-Sleep -Seconds 1
        foreach ($s in $samples) {
            $p=$s.Process
            if ($p.HasExited) { throw "Probe exited during measurement" }
            $p.Refresh()
            $s.MaxPrivate=[Math]::Max($s.MaxPrivate,$p.PrivateMemorySize64)
            $s.MaxWorking=[Math]::Max($s.MaxWorking,$p.WorkingSet64)
        }
    }
    $elapsed=$clock.Elapsed.TotalMilliseconds
    $report=foreach ($s in $samples) {
        [pscustomobject]@{Name=$s.Process.ProcessName; Pid=$s.Process.Id;
            CpuPercentOneCore=[Math]::Round(100*($s.Process.TotalProcessorTime.TotalMilliseconds-$s.StartCpu)/$elapsed,2);
            SampledMaxPrivateMiB=[Math]::Round($s.MaxPrivate/1MB,2);
            SampledMaxWorkingSetMiB=[Math]::Round($s.MaxWorking/1MB,2)}
    }
    foreach ($p in $owned | Select-Object -Skip 1) {
        if (-not $p.WaitForExit(10000) -or $p.ExitCode -ne 0) { throw "Duplex probe failed" }
        Write-Host ($p.StandardOutput.ReadToEnd().Trim())
    }
    $report | ConvertTo-Json
} finally {
    foreach ($p in $owned) {
        if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit() }
        $p.Dispose()
    }
}
