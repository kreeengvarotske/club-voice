"""Run from the repository root: python -m unittest discover -s tools/game-benchmark"""

import contextlib
import io
import os
import unittest

from summarize_frametimes import (
    CaptureError,
    main,
    paired_report,
    percentile,
    summarize_file,
)

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "testdata")
PILOT = os.path.join(DATA, "pilot")
PILOT_RUNS = [
    ("voice_off", "run-1-voice_off.csv"),
    ("voice_on", "run-2-voice_on.csv"),
    ("voice_on", "run-3-voice_on.csv"),
    ("voice_off", "run-4-voice_off.csv"),
    ("voice_off", "run-5-voice_off.csv"),
    ("voice_on", "run-6-voice_on.csv"),
]


def data(name):
    return os.path.join(DATA, name)


def run_cli(args):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = main(args)
    return code, out.getvalue(), err.getvalue()


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


class SelectionTest(unittest.TestCase):
    def test_counts_only_the_selected_game_process(self):
        # game.exe: 90 x 10 ms, 8 x 20 ms, 2 x 50 ms (+ one NA row) among
        # dwm.exe (60 x 16.667 ms) and Discord.exe (30 x 33.333 ms).
        s = summarize_file(data("multi_process.csv"), "GAME.EXE")
        self.assertEqual((s.process, s.pid, s.frames), ("game.exe", "4242", 100))
        self.assertAlmostEqual(s.seconds, 1.160)
        self.assertAlmostEqual(s.avg_fps, 100 / 1.160)
        self.assertEqual((s.p95_ms, s.p99_ms), (20.0, 50.0))

    def test_multiple_processes_without_selection_is_an_error(self):
        with self.assertRaisesRegex(CaptureError, "ambiguous.*Discord.exe.*dwm.exe.*game.exe"):
            summarize_file(data("multi_process.csv"))

    def test_unknown_process_lists_what_the_capture_has(self):
        with self.assertRaisesRegex(CaptureError, "no frames for process 'other.exe'.*game.exe"):
            summarize_file(data("multi_process.csv"), "other.exe")

    def test_same_name_with_two_pids_needs_pid(self):
        with self.assertRaisesRegex(CaptureError, "pass --pid.*4242.*5150"):
            summarize_file(data("two_game_pids.csv"), "game.exe")
        s = summarize_file(data("two_game_pids.csv"), "game.exe", "5150")
        self.assertEqual((s.frames, s.avg_fps, s.p99_ms), (20, 62.5, 16.0))

    def test_empty_captures_are_errors(self):
        with self.assertRaisesRegex(CaptureError, "no frames \\(header only\\)"):
            summarize_file(data("empty.csv"), "game.exe")
        with self.assertRaisesRegex(CaptureError, "empty file"):
            summarize_file(data("zero_bytes.csv"), "game.exe")

    def test_presentmon2_frame_time_column(self):
        s = summarize_file(data("presentmon2.csv"))
        self.assertEqual((s.frames, s.avg_fps, s.p95_ms), (50, 80.0, 12.5))

    def test_nearest_rank_percentile(self):
        values = list(range(1, 101))
        self.assertEqual(percentile(values, 95), 95)
        self.assertEqual(percentile(values, 99), 99)
        self.assertEqual(percentile([7.0], 99), 7.0)
        # Nearest rank rounds up: 95% of 10 values is rank 10, not 9.
        self.assertEqual(percentile(list(range(1, 11)), 95), 10)
        with self.assertRaises(CaptureError):
            percentile([], 95)


class ReportTest(unittest.TestCase):
    def test_example_report_matches_expected_output(self):
        code, out, _ = run_cli([data("multi_process.csv"), "--process", "game.exe"])
        self.assertEqual(code, 0)
        self.assertEqual(out, read(data("expected-multi_process.txt")))

    def test_pilot_chain_matches_expected_report(self):
        args = ["--process", "game.exe"]
        args += [f"{c}={os.path.join(PILOT, f)}" for c, f in PILOT_RUNS]
        code, out, _ = run_cli(args)
        self.assertEqual(code, 0)
        self.assertEqual(out, read(os.path.join(PILOT, "expected-report.md")))

    def test_pilot_known_values(self):
        runs = [(c, summarize_file(os.path.join(PILOT, f), "game.exe")) for c, f in PILOT_RUNS]
        # Run 1: 970 x 10 ms + 20 x 15 ms + 10 x 30 ms = 10.3 s for 1000 frames.
        self.assertAlmostEqual(runs[0][1].avg_fps, 1000 / 10.3)
        # Run 3: 959 x 10 + 30 x 15 + 11 x 30 ms; ranks 990-1000 are 30 ms.
        self.assertEqual(runs[2][1].p99_ms, 30.0)
        self.assertIn("| 2 | 3, 4 | -0.66 | +0.00 | +15.00 |", paired_report(runs))

    def test_cli_reports_ambiguity_with_exit_code_2(self):
        code, out, err = run_cli([data("multi_process.csv")])
        self.assertEqual((code, out), (2, ""))
        self.assertIn("error:", err)
        self.assertIn("ambiguous selection", err)

    def test_pairs_must_alternate_conditions(self):
        s = summarize_file(data("presentmon2.csv"))
        with self.assertRaisesRegex(CaptureError, "not a voice_off/voice_on pair"):
            paired_report([("voice_off", s), ("voice_off", s), ("voice_on", s), ("voice_on", s)])


if __name__ == "__main__":
    unittest.main()
