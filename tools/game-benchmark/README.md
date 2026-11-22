# Game benchmark tools

Helpers for the procedure in [docs/GAME_BENCHMARK.md](../../docs/GAME_BENCHMARK.md). Standard-library Python 3 only.

- `summarize_frametimes.py` summarizes PresentMon CSV captures for one selected game process. It reports average FPS and nearest-rank p95/p99 frame time, and produces a paired voice_off/voice_on report.
- `results-template.csv` is the per-run table to fill in.
- `testdata/` holds synthetic captures with known frame intervals, used by the tests and the examples below.

Tests (from the repository root):

```
python -m unittest discover -s tools/game-benchmark
```

## Example: one capture

`testdata/multi_process.csv` mixes three processes. For `game.exe`, 90 frames take 10 ms, 8 take 20 ms and 2 take 50 ms, plus one `NA` row that is ignored. `dwm.exe` and `Discord.exe` rows must not be counted.

```
python tools/game-benchmark/summarize_frametimes.py tools/game-benchmark/testdata/multi_process.csv --process game.exe
```

Expected (`testdata/expected-multi_process.txt`):

```
process=game.exe pid=4242 frames=100 seconds=1.160 avg_fps=86.21 p95_ms=20.00 p99_ms=50.00
```

The arithmetic: 100 frames over 900 + 160 + 100 = 1160 ms gives 86.21 FPS. By nearest rank, rank 95 falls in the 20 ms frames and rank 99 in the 50 ms frames.

Without `--process`, the script refuses to guess. It exits with code 2:

```
error: .../multi_process.csv: ambiguous selection, pass --process (and --pid if needed); candidates: Discord.exe (pid 7000, 30 frames), dwm.exe (pid 1100, 60 frames), game.exe (pid 4242, 100 frames)
```

A capture with a header but no rows reports `capture has no frames (header only)`. The same name with two process IDs reports `ambiguous selection, pass --pid` and lists both.

## Example: paired pilot chain on test captures

`testdata/pilot/run-1..6-*.csv` are six synthetic 1000-frame runs in the documented order: off, on, on, off, off, on. Each run also contains `dwm.exe` rows, and the game PID changes from run to run, as it does when a game is restarted.

```
python tools/game-benchmark/summarize_frametimes.py --process game.exe \
  voice_off=tools/game-benchmark/testdata/pilot/run-1-voice_off.csv \
  voice_on=tools/game-benchmark/testdata/pilot/run-2-voice_on.csv \
  voice_on=tools/game-benchmark/testdata/pilot/run-3-voice_on.csv \
  voice_off=tools/game-benchmark/testdata/pilot/run-4-voice_off.csv \
  voice_off=tools/game-benchmark/testdata/pilot/run-5-voice_off.csv \
  voice_on=tools/game-benchmark/testdata/pilot/run-6-voice_on.csv
```

The expected output is `testdata/pilot/expected-report.md`. These numbers are synthetic and are **not** a measurement of Club Voice.
