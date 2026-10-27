"""Constructed contract fixtures; these are not market completeness evidence."""
import copy
import datetime as dt
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from audit_minute_archive import audit


class MinuteArchive(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.now = dt.datetime.fromisoformat("2026-10-27T08:00:00+00:00")
        self.rows = [
            {"market": "sz", "code": "000001", "date": "2026-10-26",
             "slot": slot, "price_minor": 1000, "volume_shares": volume,
             "amount_minor": volume * 1000, "cumulative_volume_shares": cumulative,
             "cumulative_amount_minor": cumulative * 1000}
            for slot, volume, cumulative in [("09:31", 2, 2), ("09:32", 3, 5)]
        ]
        self.evidence = self.pin("reference", b"synthetic independently reviewed control")
        self.evidence.update(source="https://example.org/test-only",
                             qualification="independently_reviewed")
        self.manifest = {
            "schema_version": 1, "archive_id": "test", "revision": 1,
            "source_as_of": "2026-10-27T07:00:00+00:00",
            "retention": {"days": 30, "max_bytes": 100000, "max_files": 20,
                          "max_freshness_seconds": 7200},
            "expected": [{"market": "sz", "code": "000001", "date": "2026-10-26",
                          "state": "Trading", "slots": ["09:31", "09:32"],
                          "evidence": self.evidence}],
            "partitions": [self.partition()],
        }

    def pin(self, name, body):
        (self.root / name).write_bytes(body)
        return {"path": name, "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}

    def partition(self):
        return {
            "market": "sz", "code": "000001", "date": "2026-10-26",
            "rows": self.pin("rows.jsonl", b"".join(
                (json.dumps(r) + "\n").encode() for r in self.rows)),
            "source": {"kind": "RequestedRefresh", "endpoint": "test:7709",
                       "request": self.pin("request.bin", b"synthetic request"),
                       "response": self.pin("response.bin", b"synthetic response"),
                       "captured_at": "2026-10-27T07:00:00+00:00"},
            "qualification": {axis: copy.deepcopy(self.evidence)
                              for axis in ("date", "slots", "price", "volume", "amount")},
            "day_control": {"volume_shares": 5, "amount_minor": 5000,
                            "evidence": copy.deepcopy(self.evidence)},
        }

    def run_audit(self, manifest=None, previous=None):
        return audit(manifest or self.manifest, self.root, self.now, previous)

    def test_full_slots_and_day_closure_preserve_source_and_replay_digest(self):
        result = self.run_audit()
        self.assertEqual(result["status"], "covered")
        self.assertEqual(result["sources"], {"RequestedRefresh": 1})
        self.assertEqual(result["rows"], 2)
        self.assertEqual(result["passive_push_verified"], False)

    def test_gap_extra_slot_or_unqualified_volume_cannot_complete_archive(self):
        for fault in ("gap", "extra", "volume", "closure", "freshness", "retention"):
            manifest = copy.deepcopy(self.manifest)
            if fault in ("gap", "extra"):
                rows = copy.deepcopy(self.rows)
                if fault == "gap":
                    rows.pop()
                else:
                    rows[1]["slot"] = "09:33"
                manifest["partitions"][0]["rows"] = self.pin(
                    "changed.jsonl", b"".join((json.dumps(r) + "\n").encode() for r in rows))
            elif fault == "volume":
                manifest["partitions"][0]["qualification"]["volume"]["qualification"] = "unverified"
            elif fault == "closure":
                manifest["partitions"][0]["day_control"]["volume_shares"] = 6
            elif fault == "freshness":
                manifest["retention"]["max_freshness_seconds"] = 1
            else:
                manifest["retention"]["days"] = None
            with self.subTest(fault=fault):
                self.assertEqual(self.run_audit(manifest)["status"], "incomplete")

    def test_raw_change_symlink_duplicate_and_passive_request_rejected(self):
        (self.root / "response.bin").write_bytes(b"changed")
        with self.assertRaises(ValueError):
            self.run_audit()
        self.manifest["partitions"][0] = self.partition()
        (self.root / "response.bin").unlink()
        (self.root / "response.bin").symlink_to(self.root / "reference")
        with self.assertRaises(ValueError):
            self.run_audit()
        (self.root / "response.bin").unlink()
        self.manifest["partitions"] = [self.partition(), self.partition()]
        with self.assertRaises(ValueError):
            self.run_audit()
        self.manifest["partitions"] = [self.partition()]
        self.manifest["partitions"][0]["source"]["kind"] = "PassivePush"
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_revision_rollback_and_lost_retained_partition_rejected(self):
        previous = copy.deepcopy(self.manifest)
        self.manifest["revision"] = 2
        self.assertEqual(self.run_audit(previous=previous)["status"], "covered")
        self.manifest["source_as_of"] = "2026-10-26T07:00:00+00:00"
        with self.assertRaises(ValueError):
            self.run_audit(previous=previous)
        self.manifest["source_as_of"] = previous["source_as_of"]
        self.manifest["partitions"][0]["source"]["captured_at"] = "2026-10-26T07:00:00+00:00"
        with self.assertRaises(ValueError):
            self.run_audit(previous=previous)
        self.manifest["partitions"] = []
        self.assertEqual(self.run_audit(previous=previous)["status"], "incomplete")

    @unittest.skipIf(sys.platform == "win32", "output alias regression requires POSIX symlinks")
    def test_cli_output_alias_cannot_write_into_input_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            outside = Path(directory)
            manifest = outside / "manifest.json"
            manifest.write_text(json.dumps(self.manifest))
            alias = outside / "archive-alias"
            alias.symlink_to(self.root, target_is_directory=True)
            before = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                      for path in self.root.iterdir()}

            def run(output):
                return subprocess.run(
                    [sys.executable, str(Path(__file__).resolve().parents[1] / "audit_minute_archive.py"),
                     "--manifest", str(manifest), "--root", str(self.root),
                     "--as-of", self.now.isoformat(), "--output", str(output)],
                    capture_output=True, timeout=30,
                )

            safe_output = outside / "safe-result.json"
            control = run(safe_output)
            self.assertEqual(control.returncode, 0, control.stderr.decode())
            self.assertEqual(json.loads(safe_output.read_text())["status"], "covered")
            for output in (alias / "aliased-result.json", self.root / "direct-result.json"):
                with self.subTest(output=output):
                    result = run(output)
                    self.assertNotEqual(result.returncode, 0, result.stdout.decode())
                    self.assertIn(b"output must be outside the archive", result.stderr)
                    self.assertFalse(output.exists())
                    self.assertEqual(
                        before,
                        {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                         for path in self.root.iterdir()},
                    )


if __name__ == "__main__":
    unittest.main()
