"""Synthetic commands exercise result accounting; they never certify a live source."""
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from run_online_tests import execute, inventory, ROOT


@unittest.skipUnless(os.name == "posix", "online execution requires process groups")
class OnlineTests(unittest.TestCase):
    def test_inventory_explicitly_reports_every_external_case_as_skipped(self):
        suites = inventory(ROOT)
        self.assertEqual(sum(len(suite["tests"]) for suite in suites), 19)
        self.assertTrue(all(test["status"] == "skipped" for suite in suites for test in suite["tests"]))

    def test_success_missing_zero_ignored_failed_and_timeout_results_are_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            command = root / "synthetic-cargo"
            for index, (body, exit_code, expected) in enumerate([
                ("test sample ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;", 0, True),
                ("test result: ok. 0 passed; 0 failed; 0 ignored;", 0, False),
                ("test result: ok. 0 passed; 0 failed; 1 ignored;", 0, False),
                ("test sample ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;", 1, False),
                ("test sample ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;", 1, False),
                ("no test result", 0, False),
            ]):
                output = root / str(index)
                output.mkdir()
                command.write_text("#!/bin/sh\ncat <<'RESULT'\n" + body +
                                   "\nRESULT\nexit " + str(exit_code) + "\n")
                command.chmod(0o700)
                suite = dict(package="synthetic", target="synthetic", tests=[dict(name="sample")])
                # These cases verify accounting, not process startup latency.
                # Keep the separate deadline case below at 20ms.
                outcome = execute(root, suite, str(command), 30, output)
                self.assertEqual(outcome, expected, {"body": body, "suite": suite,
                                                     "log": (output / "synthetic.log").read_text()})
                self.assertEqual(suite["status"], "passed" if expected else "failed")
            output = root / "timeout"
            output.mkdir()
            command.write_text("#!/bin/sh\nsleep 30\n")
            suite = dict(package="synthetic", target="synthetic", tests=[dict(name="sample")])
            self.assertFalse(execute(root, suite, str(command), 0.02, output))
            self.assertIn("deadline", suite["tests"][0]["reason"])
