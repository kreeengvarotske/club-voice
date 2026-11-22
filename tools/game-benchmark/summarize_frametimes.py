"""Summarize a PresentMon CSV capture for the Club Voice game benchmark.

Usage:
    python tools/game-benchmark/summarize_frametimes.py CAPTURE.csv [--process game.exe]

Prints one line: frames, captured seconds, average FPS and p95/p99 frame time
in milliseconds. Works with PresentMon 1.x (`MsBetweenPresents`) and 2.x
(`FrameTime` or `MsBetweenPresents`) CSV output. Standard library only.
"""

from __future__ import annotations

import argparse
import csv
import math
import sys

FRAME_TIME_COLUMNS = ("MsBetweenPresents", "FrameTime", "msBetweenPresents")
PROCESS_COLUMNS = ("Application", "ProcessName")


def percentile(sorted_values: list[float], pct: float) -> float:
    """Nearest-rank percentile of an ascending list."""
    if not sorted_values:
        raise ValueError("no frames")
    rank = max(1, math.ceil(pct / 100.0 * len(sorted_values)))
    return sorted_values[rank - 1]


def summarize(rows: list[dict[str, str]], process: str | None = None) -> dict[str, float]:
    if not rows:
        raise ValueError("capture is empty")
    column = next((c for c in FRAME_TIME_COLUMNS if c in rows[0]), None)
    if column is None:
        raise ValueError(f"no frame-time column; expected one of {FRAME_TIME_COLUMNS}")
    name_column = next((c for c in PROCESS_COLUMNS if c in rows[0]), None)
    if process is not None:
        if name_column is None:
            raise ValueError("capture has no process name column to filter on")
        rows = [r for r in rows if r[name_column].lower() == process.lower()]
    times = []
    for r in rows:
        value = r.get(column, "").strip()
        if not value or value.upper() == "NA":
            continue
        ms = float(value)
        if ms > 0:
            times.append(ms)
    if not times:
        raise ValueError("no frames for the selected process")
    total_ms = sum(times)
    times.sort()
    return {
        "frames": len(times),
        "seconds": total_ms / 1000.0,
        "avg_fps": len(times) / (total_ms / 1000.0),
        "p95_ms": percentile(times, 95),
        "p99_ms": percentile(times, 99),
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("capture")
    parser.add_argument("--process", help="keep only rows for this executable name")
    args = parser.parse_args(argv)
    with open(args.capture, newline="", encoding="utf-8-sig") as f:
        rows = list(csv.DictReader(f))
    s = summarize(rows, args.process)
    print(
        f"frames={s['frames']} seconds={s['seconds']:.1f} avg_fps={s['avg_fps']:.1f} "
        f"p95_ms={s['p95_ms']:.2f} p99_ms={s['p99_ms']:.2f}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
