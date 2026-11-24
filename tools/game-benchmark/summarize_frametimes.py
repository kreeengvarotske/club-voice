"""Summarize PresentMon CSV captures for the Club Voice game benchmark.

Single capture:
    python tools/game-benchmark/summarize_frametimes.py CAPTURE.csv --process game.exe

Paired benchmark (conditions in run order, see docs/GAME_BENCHMARK.md):
    python tools/game-benchmark/summarize_frametimes.py --process game.exe \
        voice_off=run-1.csv voice_on=run-2.csv voice_on=run-3.csv \
        voice_off=run-4.csv voice_off=run-5.csv voice_on=run-6.csv

Only frames of the selected game process are counted. If `--process` is
omitted and the capture contains more than one process, or the selected name
matches more than one process ID, the script stops with an error that lists
the candidates instead of guessing. Works with PresentMon 1.x
(`MsBetweenPresents`) and 2.x (`FrameTime` or `MsBetweenPresents`) columns.
Standard library only.
"""

from __future__ import annotations

import argparse
import csv
import math
import statistics
import sys
from dataclasses import dataclass

FRAME_TIME_COLUMNS = ("MsBetweenPresents", "FrameTime", "msBetweenPresents")
PROCESS_COLUMNS = ("Application", "ProcessName")
PID_COLUMNS = ("ProcessID", "ProcessId")


class CaptureError(ValueError):
    """A capture that cannot be summarized unambiguously."""


@dataclass(frozen=True)
class Summary:
    process: str
    pid: str
    frames: int
    seconds: float
    avg_fps: float
    p95_ms: float
    p99_ms: float

    def line(self) -> str:
        return (
            f"process={self.process} pid={self.pid} frames={self.frames} "
            f"seconds={self.seconds:.3f} avg_fps={self.avg_fps:.2f} "
            f"p95_ms={self.p95_ms:.2f} p99_ms={self.p99_ms:.2f}"
        )


def percentile(sorted_values: list[float], pct: float) -> float:
    """Nearest-rank percentile of an ascending list."""
    if not sorted_values:
        raise CaptureError("no frames")
    rank = max(1, math.ceil(pct / 100.0 * len(sorted_values)))
    return sorted_values[rank - 1]


def _pick(header: list[str], names: tuple[str, ...]) -> str | None:
    return next((c for c in names if c in header), None)


def summarize(
    rows: list[dict[str, str]],
    header: list[str],
    process: str | None = None,
    pid: str | None = None,
) -> Summary:
    time_col = _pick(header, FRAME_TIME_COLUMNS)
    if time_col is None:
        raise CaptureError(
            f"no frame-time column; expected one of {', '.join(FRAME_TIME_COLUMNS)}"
        )
    name_col = _pick(header, PROCESS_COLUMNS)
    pid_col = _pick(header, PID_COLUMNS)
    if not rows:
        raise CaptureError("capture has no frames (header only)")

    # Group valid frame times by (name, pid).
    groups: dict[tuple[str, str], list[float]] = {}
    for r in rows:
        value = (r.get(time_col) or "").strip()
        if not value or value.upper() == "NA":
            continue
        ms = float(value)
        if ms <= 0:
            continue
        name = (r.get(name_col) or "?") if name_col else "?"
        p = (r.get(pid_col) or "?") if pid_col else "?"
        groups.setdefault((name, p), []).append(ms)
    if not groups:
        raise CaptureError("capture has no valid frame times")

    def describe(keys) -> str:
        return ", ".join(f"{n} (pid {p}, {len(groups[(n, p)])} frames)" for n, p in sorted(keys))

    candidates = list(groups)
    if process is not None:
        if name_col is None:
            raise CaptureError("capture has no process name column; cannot select --process")
        candidates = [k for k in candidates if k[0].lower() == process.lower()]
        if not candidates:
            raise CaptureError(f"no frames for process {process!r}; capture has: {describe(groups)}")
    if pid is not None:
        candidates = [k for k in candidates if k[1] == pid]
        if not candidates:
            raise CaptureError(f"no frames for pid {pid}; capture has: {describe(groups)}")
    if len(candidates) > 1:
        hint = "--pid" if process is not None else "--process (and --pid if needed)"
        raise CaptureError(
            f"ambiguous selection, pass {hint}; candidates: {describe(candidates)}"
        )

    name, p = candidates[0]
    times = sorted(groups[(name, p)])
    total_ms = sum(times)
    return Summary(
        process=name,
        pid=p,
        frames=len(times),
        seconds=total_ms / 1000.0,
        avg_fps=len(times) / (total_ms / 1000.0),
        p95_ms=percentile(times, 95),
        p99_ms=percentile(times, 99),
    )


def summarize_file(path: str, process: str | None = None, pid: str | None = None) -> Summary:
    with open(path, newline="", encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        rows = list(reader)
        header = reader.fieldnames
    if not header:
        raise CaptureError(f"{path}: empty file")
    try:
        return summarize(rows, list(header), process, pid)
    except CaptureError as e:
        raise CaptureError(f"{path}: {e}") from None


def _stats(values: list[float]) -> str:
    mean = statistics.fmean(values)
    sd = statistics.stdev(values) if len(values) > 1 else 0.0
    return f"{mean:.2f} ± {sd:.2f}"


def paired_report(runs: list[tuple[str, Summary]]) -> str:
    """Markdown report: per-run table, per-condition mean ± sd, and
    consecutive off/on pair differences (on minus off)."""
    out = ["| Run | Condition | Frames | Seconds | Avg FPS | p95 ms | p99 ms |",
           "| --- | --- | --- | --- | --- | --- | --- |"]
    for i, (cond, s) in enumerate(runs, 1):
        out.append(
            f"| {i} | {cond} | {s.frames} | {s.seconds:.1f} | {s.avg_fps:.2f} | "
            f"{s.p95_ms:.2f} | {s.p99_ms:.2f} |"
        )
    out += ["", "| Condition | Runs | Avg FPS | p95 ms | p99 ms |", "| --- | --- | --- | --- | --- |"]
    for cond in dict.fromkeys(c for c, _ in runs):
        sel = [s for c, s in runs if c == cond]
        out.append(
            f"| {cond} | {len(sel)} | {_stats([s.avg_fps for s in sel])} | "
            f"{_stats([s.p95_ms for s in sel])} | {_stats([s.p99_ms for s in sel])} |"
        )
    conds = list(dict.fromkeys(c for c, _ in runs))
    if len(conds) == 2 and len(runs) % 2 == 0:
        base, other = ("voice_off", "voice_on") if set(conds) == {"voice_off", "voice_on"} else conds
        out += ["", f"| Pair | Runs | Δ Avg FPS ({other} − {base}) | Δ p95 ms | Δ p99 ms |",
                "| --- | --- | --- | --- | --- |"]
        for k in range(0, len(runs), 2):
            (c1, s1), (c2, s2) = runs[k], runs[k + 1]
            if {c1, c2} != {base, other}:
                raise CaptureError(f"runs {k + 1} and {k + 2} are not a {base}/{other} pair")
            off, on = (s1, s2) if c1 == base else (s2, s1)
            out.append(
                f"| {k // 2 + 1} | {k + 1}, {k + 2} | {on.avg_fps - off.avg_fps:+.2f} | "
                f"{on.p95_ms - off.p95_ms:+.2f} | {on.p99_ms - off.p99_ms:+.2f} |"
            )
    return "\n".join(out)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("captures", nargs="+", help="CAPTURE.csv, or CONDITION=CAPTURE.csv for a paired report")
    parser.add_argument("--process", help="game executable name, e.g. game.exe (case-insensitive)")
    parser.add_argument("--pid", help="process ID, when one name has several processes")
    args = parser.parse_args(argv)
    try:
        if len(args.captures) == 1 and "=" not in args.captures[0]:
            print(summarize_file(args.captures[0], args.process, args.pid).line())
            return 0
        runs = []
        for item in args.captures:
            cond, sep, path = item.partition("=")
            if not sep or not cond or not path:
                raise CaptureError(f"expected CONDITION=CAPTURE.csv, got {item!r}")
            runs.append((cond, summarize_file(path, args.process, args.pid)))
        print(paired_report(runs))
        return 0
    except CaptureError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
