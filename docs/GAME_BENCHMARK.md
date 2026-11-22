# Game benchmark: voice off vs two synthetic clients

Issue #1, task 2. This is a procedure for the first available Windows tester. It measures whether two active synthetic Club Voice clients plus the local server change game frame time. Nothing here opens a microphone or plays audio: the `--tone-peer` clients generate and render their signal in memory.

## What to record once

Fill these in before the first run and keep them unchanged for all runs:

- PC: CPU, GPU, RAM, Windows edition and build (`winver`), GPU driver version, display refresh rate, power plan.
- Game: title, version/build, graphics preset and any changed settings, resolution, display mode (fullscreen/borderless), V-Sync/frame cap/upscaler state.
- Scene: built-in benchmark name, or replay/route with an exact start and end point. It must be repeatable and at least 60 s long; keep it at most 120 s (the resource script limit).
- Tools: PresentMon version, Club Voice commit SHA, Rust version.
- Background: close browsers, launchers' overlays, recording/streaming tools and anything else not needed. Leave them in the same state for all runs.

## One-time setup

1. Build the release examples from the repository root (PowerShell). If CMake is not on `PATH`, set `$env:CMAKE` to the `cmake.exe` bundled with Visual Studio Build Tools.

   ```powershell
   git rev-parse HEAD
   New-Item -ItemType Directory -Force results
   cargo build --release --locked --workspace --examples --features media-smoke
   ```

2. Install [PresentMon](https://github.com/GameTechDev/PresentMon) (command-line capture) and note its version.
3. Do one warm-up run of the scene and discard it (shader compilation, file cache).

## Run order

Six runs, alternating conditions so drift (heat, background tasks) does not favour one side:

| Run | Condition |
| --- | --- |
| 1 | voice_off |
| 2 | voice_on |
| 3 | voice_on |
| 4 | voice_off |
| 5 | voice_off |
| 6 | voice_on |

Use the same scene length `S` (seconds, 60-120) for every run. Wait 30 s idle between runs.

## Each run

Open two PowerShell 7 windows in the repository root.

**voice_on only, window 1** (start it about 5 s before the scene):

```powershell
./crates/server/tests/windows_resources.ps1 -Seconds S *>&1 | Tee-Object results/run-N-voice.txt
```

It starts the local server and two synthetic clients, samples their CPU and memory once a second for `S` seconds, and prints one row per process plus each client's `sent/decoded/... playback_queue_full underrun_samples` counters.

**Both conditions, window 2:** start the scene in the game, then immediately start the frame-time capture:

```powershell
PresentMon --process_name GAME.exe --output_file results/run-N-frames.csv --timed S --terminate_after_timed
```

(Flag names may differ between PresentMon versions; record the exact command used.)

After the run, summarize the capture. `--process` is required whenever the capture holds more than one process (PresentMon usually also records `dwm.exe` and overlays). If one executable name has several process IDs, add `--pid`:

```powershell
python tools/game-benchmark/summarize_frametimes.py results/run-N-frames.csv --process GAME.exe
```

It prints `process pid frames seconds avg_fps p95_ms p99_ms` for that process only. If the selection is ambiguous or the capture is empty, it exits with code 2 and lists the candidate processes instead of guessing.

Copy the printed values and, for voice_on, the per-process CPU and memory rows and the client counters into `tools/game-benchmark/results-template.csv` (save a copy as `results/game-benchmark.csv`).

After all six runs, produce the paired report (run order matters: consecutive runs form the off/on pairs):

```powershell
python tools/game-benchmark/summarize_frametimes.py --process GAME.exe `
  voice_off=results/run-1-frames.csv voice_on=results/run-2-frames.csv `
  voice_on=results/run-3-frames.csv voice_off=results/run-4-frames.csv `
  voice_off=results/run-5-frames.csv voice_on=results/run-6-frames.csv
```

It prints a per-run table, the mean ± standard deviation per condition and the voice_on minus voice_off difference for each pair. `tools/game-benchmark/README.md` shows a worked example on test captures.

## Input latency

Report it only if it is measured directly, for example with NVIDIA Reflex Latency Analyzer, LDAT or a high-speed camera, and describe the method. Otherwise write `not measured`. Do not derive it from frame time.

## What to post in Issue #1

- The fixed configuration above.
- The filled CSV and the raw files: every `run-N-frames.csv` and `run-N-voice.txt`.
- Per condition: mean and spread of average FPS, p95 and p99 frame time over the three runs, and the paired differences (run 2 vs 1, 3 vs 4, 6 vs 5).
- Client and server CPU (% of one logical core) and sampled peak private/working-set memory.
- Anything unusual: stutters, alt-tab, thermal throttling, a client that exited early.

Three pairs is a small sample: report the numbers and their spread, and do not claim "no impact" from a difference smaller than the run-to-run variation.
