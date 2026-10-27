import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class FuzzSeedsTests(unittest.TestCase):
    def test_real_seed_identity_boundaries_and_new_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / "seeds"
            command = [sys.executable, str(ROOT / "scripts/validation/build_fuzz_seeds.py"),
                       "--output-directory", str(out)]
            result = subprocess.run(command, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            manifest = json.loads((out / "manifest.json").read_text())
            self.assertGreater(manifest["total_bytes"], 0)
            self.assertLessEqual(manifest["total_bytes"], 64 * 1024 * 1024)
            variants = {row["variant"] for row in manifest["seeds"]}
            self.assertTrue({"original", "truncate-last", "count-max",
                             "wire-truncated", "frame-length-max"} <= variants)
            import hashlib
            for row in manifest["seeds"]:
                path = out / row["path"]
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), row["sha256"])
                source = ROOT / row["source"]
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), row["source_sha256"])
            again = subprocess.run(command, capture_output=True)
            self.assertNotEqual(again.returncode, 0)
            second = Path(temporary) / "second"
            subprocess.run(command[:-1] + [str(second)], check=True, capture_output=True)
            self.assertEqual((out / "manifest.json").read_bytes(), (second / "manifest.json").read_bytes())
