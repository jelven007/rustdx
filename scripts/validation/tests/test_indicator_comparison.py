"""Reject incomparable observations, bad statistics, changed outputs and regressions."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "bench"))
from compare_indicators import compare, load


class Comparison(unittest.TestCase):
    def report(self):
        return {
            "schema_version": 2, "statistic": "median", "synthetic": True,
            "environment": {"os": "test", "arch": "test", "os_version": "test", "hardware": "test",
                            "rustc": "test", "cargo": "test", "build": "test",
                            "input_sha256": "a", "harness_sha256": "b", "samples": 5,
                            "session": "same-host", "sampling": "AB/BA"},
            "lock_sha256": "c", "binary_sha256": "d",
            "measurements": [{"name": "sma_rsi", "rows": 10000, "period": 20,
                              "samples_ns": [10, 10, 10, 10, 10],
                              "median_ns": 10, "result_hash": "e"}],
        }

    def loaded(self, record):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "observation.json"
            path.write_text(json.dumps(record))
            return load(path)

    def test_identity_and_sets_must_match(self):
        before, old = self.loaded(self.report())
        self.assertTrue(compare(before, before, old, old, 1.5)["passed"])
        for field in before["environment"]:
            changed = copy.deepcopy(before)
            changed["environment"][field] = "different"
            with self.assertRaises(ValueError):
                compare(before, changed, old, old, 1.5)
        with self.assertRaises(ValueError):
            compare(before, {**before, "lock_sha256": "different"}, old, old, 1.5)
        with self.assertRaises(ValueError):
            compare(before, before, old, {}, 1.5)

    def test_invalid_statistics_and_schema_are_rejected(self):
        for change in [{"median_ns": 11}, {"samples_ns": [10] * 4},
                       {"samples_ns": [float("nan")] * 5}, {"samples_ns": [float("inf")] * 5},
                       {"samples_ns": [0] * 5}, {"samples_ns": [True] * 5}]:
            report = self.report()
            report["measurements"][0].update(change)
            with self.assertRaises(ValueError):
                self.loaded(report)
        report = self.report()
        report["schema_version"] = 1
        with self.assertRaises(ValueError):
            self.loaded(report)

    def test_regression_and_changed_result_fail_gate(self):
        before, old = self.loaded(self.report())
        for change in [{"result_hash": "changed"}, {"samples_ns": [16] * 5, "median_ns": 16}]:
            report = self.report()
            report["measurements"][0].update(change)
            after, new = self.loaded(report)
            self.assertFalse(compare(before, after, old, new, 1.5)["passed"])


if __name__ == "__main__":
    unittest.main()
