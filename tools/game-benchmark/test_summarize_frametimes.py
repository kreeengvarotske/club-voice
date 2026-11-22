"""Run: python -m unittest discover -s tools/game-benchmark"""

import unittest

from summarize_frametimes import percentile, summarize


class SummarizeTest(unittest.TestCase):
    def test_average_and_percentiles(self):
        # 98 frames at 10 ms, one at 20 ms, one at 40 ms.
        rows = [{"Application": "game.exe", "MsBetweenPresents": "10"} for _ in range(98)]
        rows += [
            {"Application": "game.exe", "MsBetweenPresents": "20"},
            {"Application": "game.exe", "MsBetweenPresents": "40"},
        ]
        s = summarize(rows)
        self.assertEqual(s["frames"], 100)
        self.assertAlmostEqual(s["seconds"], 1.04)
        self.assertAlmostEqual(s["avg_fps"], 100 / 1.04)
        self.assertEqual(s["p95_ms"], 10)
        self.assertEqual(s["p99_ms"], 20)

    def test_presentmon2_column_and_process_filter(self):
        rows = [
            {"Application": "Game.exe", "FrameTime": "16.0"},
            {"Application": "dwm.exe", "FrameTime": "5.0"},
            {"Application": "game.exe", "FrameTime": "NA"},
        ]
        s = summarize(rows, "game.exe")
        self.assertEqual(s["frames"], 1)
        self.assertAlmostEqual(s["avg_fps"], 62.5)

    def test_rejects_missing_column_and_empty_selection(self):
        with self.assertRaises(ValueError):
            summarize([{"Application": "game.exe", "Other": "1"}])
        with self.assertRaises(ValueError):
            summarize([{"Application": "a.exe", "MsBetweenPresents": "10"}], "b.exe")
        with self.assertRaises(ValueError):
            percentile([], 95)


if __name__ == "__main__":
    unittest.main()
