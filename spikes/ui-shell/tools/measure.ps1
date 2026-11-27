# Measures the UI shell spike on Windows: time to window, frame counts,
# CPU and memory of the shell process and any child processes.
#
#   powershell -ExecutionPolicy Bypass -File measure.ps1 -Dir <folder with cv-shell-*.exe>
#
# Each toolkit runs in three scenarios: idle, speaking (indicators change
# every 250 ms) and idle-minimized (window minimized after warm-up). Run order
# alternates between repeats. Results go to results-<timestamp>.csv next to
# this script, together with a machine description.

param(
    [Parameter(Mandatory = $true)][string]$Dir,
    [int]$Seconds = 60,
    [int]$Warmup = 5,
    [int]$Repeats = 3
)

$ErrorActionPreference = "Stop"
# Invariant number format so the CSV uses decimal points on any locale.
[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture
Add-Type -Namespace CvMeasure -Name Win -MemberDefinition @"
[DllImport("user32.dll")] public static extern bool ShowWindow(System.IntPtr hWnd, int nCmdShow);
"@

$toolkits = @("egui", "slint", "iced")
$scenarios = @("idle", "speaking", "idle-minimized")
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outCsv = Join-Path $PSScriptRoot "results-$stamp.csv"
$outEnv = Join-Path $PSScriptRoot "machine-$stamp.txt"

$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$gpu = (Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }) -join "; "
@(
    "os=$($os.Caption) $($os.Version) build $($os.BuildNumber)",
    "cpu=$($cpu.Name) cores=$($cpu.NumberOfCores) logical=$($cpu.NumberOfLogicalProcessors)",
    "gpu=$gpu",
    "ram_gb=$([math]::Round($os.TotalVisibleMemorySize / 1MB, 1))",
    "seconds=$Seconds warmup=$Warmup repeats=$Repeats"
) | Set-Content $outEnv -Encoding utf8

function Get-Tree([int]$RootId) {
    $ids = @($RootId)
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId=$RootId" -ErrorAction SilentlyContinue
    foreach ($c in $children) { $ids += Get-Tree $c.ProcessId }
    return $ids
}

function Measure-One([string]$Toolkit, [string]$Scenario, [int]$Run) {
    $exe = Join-Path $Dir "cv-shell-$Toolkit.exe"
    $mode = if ($Scenario -eq "speaking") { "speaking" } else { "idle" }
    $stdout = Join-Path $env:TEMP "cv-$Toolkit-$Scenario-$Run.out"
    # The app must outlive the sampling window, or CPU and memory read as zero.
    $exitAfter = $Warmup + $Seconds + 15
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $exe -ArgumentList "--scenario $mode --exit-after $exitAfter" `
        -RedirectStandardOutput $stdout -PassThru
    while ($p.MainWindowHandle -eq 0 -and -not $p.HasExited -and $sw.ElapsedMilliseconds -lt 15000) {
        Start-Sleep -Milliseconds 10
        $p.Refresh()
    }
    $windowMs = $sw.ElapsedMilliseconds
    Start-Sleep -Seconds $Warmup
    if ($Scenario -eq "idle-minimized") { [void][CvMeasure.Win]::ShowWindow($p.MainWindowHandle, 6) }

    $ids = Get-Tree $p.Id
    $cpuStart = 0.0
    foreach ($id in $ids) { $cpuStart += (Get-Process -Id $id).TotalProcessorTime.TotalMilliseconds }
    $wsSum = 0.0; $privSum = 0.0; $wsPeak = 0.0; $samples = 0; $maxProcs = 0
    $t0 = $sw.ElapsedMilliseconds
    for ($i = 1; $i -le $Seconds; $i++) {
        # Deadline based: sampling overhead must not stretch the window.
        $wait = $t0 + 1000 * $i - $sw.ElapsedMilliseconds
        if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
        $ids = Get-Tree $p.Id
        $maxProcs = [math]::Max($maxProcs, $ids.Count)
        $ws = 0.0; $priv = 0.0
        foreach ($id in $ids) {
            $q = Get-Process -Id $id -ErrorAction SilentlyContinue
            if ($q) { $ws += $q.WorkingSet64; $priv += $q.PrivateMemorySize64 }
        }
        $wsSum += $ws; $privSum += $priv; $wsPeak = [math]::Max($wsPeak, $ws); $samples++
    }
    $cpuEnd = 0.0
    $alive = $true
    foreach ($id in $ids) {
        $q = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($q) { $cpuEnd += $q.TotalProcessorTime.TotalMilliseconds } else { $alive = $false }
    }
    $wallMs = $sw.ElapsedMilliseconds - $t0
    if (-not $alive) { Write-Warning "$Toolkit $Scenario run ${Run}: process exited before sampling finished; row is invalid" }
    $p.WaitForExit(30000) | Out-Null
    if (-not $p.HasExited) { $p.Kill() }
    $report = (Get-Content $stdout -ErrorAction SilentlyContinue | Select-Object -Last 1)
    $json = if ($report) { $report | ConvertFrom-Json } else { $null }

    [pscustomobject]@{
        toolkit         = $Toolkit
        scenario        = $Scenario
        run             = $Run
        window_ms       = $windowMs
        first_frame_ms  = if ($json) { $json.first_frame_ms } else { "" }
        frames_total    = if ($json) { $json.frames } else { "" }
        cpu_pct_1core   = [math]::Round(100.0 * ($cpuEnd - $cpuStart) / $wallMs, 3)
        ws_avg_mib      = [math]::Round($wsSum / $samples / 1MB, 1)
        ws_peak_mib     = [math]::Round($wsPeak / 1MB, 1)
        private_avg_mib = [math]::Round($privSum / $samples / 1MB, 1)
        processes       = $maxProcs
        valid           = $alive
    }
}

$rows = @()
for ($r = 1; $r -le $Repeats; $r++) {
    $order = if ($r % 2 -eq 1) { $toolkits } else { @($toolkits[2], $toolkits[1], $toolkits[0]) }
    foreach ($t in $order) {
        foreach ($s in $scenarios) {
            Write-Host "run $r $t $s"
            $row = Measure-One $t $s $r
            $rows += $row
            $row | Export-Csv $outCsv -NoTypeInformation -Append
            Start-Sleep -Seconds 3
        }
    }
}

Write-Host ""
Write-Host "Medians:"
$rows | Group-Object toolkit, scenario | ForEach-Object {
    $g = $_.Group
    function Med($xs) { $s = @($xs | Sort-Object); $s[[int][math]::Floor(($s.Count - 1) / 2)] }
    [pscustomobject]@{
        toolkit = $g[0].toolkit; scenario = $g[0].scenario
        window_ms = Med ($g.window_ms); cpu_pct = Med ($g.cpu_pct_1core)
        ws_avg_mib = Med ($g.ws_avg_mib); private_avg_mib = Med ($g.private_avg_mib)
        frames = Med ($g.frames_total); processes = Med ($g.processes)
    }
} | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "CSV: $outCsv"
Write-Host "Machine: $outEnv"
