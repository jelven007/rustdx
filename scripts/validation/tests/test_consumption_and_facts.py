"""Offline consumption scope and point-in-time gap regressions."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from audit_point_in_time_facts import audit
from verify_consumption_fields import verify

ROOT = Path(__file__).resolve().parents[3]


class Consumption(unittest.TestCase):
    def setUp(self):
        self.matrix = json.loads((ROOT / "config/consumption/finance-sample.json").read_text())

    def test_fixed_required_fields_and_changed_scope(self):
        self.assertEqual(verify(ROOT, self.matrix)["status"], "passed")
        for key, value in [("market", 1), ("code", "000002"), ("updated_date", 20260816),
                           ("report_period", "2026-09-30")]:
            changed = copy.deepcopy(self.matrix)
            changed["entries"][0]["scope"][key] = value
            with self.assertRaises(ValueError):
                verify(ROOT, changed)
        for fields in [["slot29"], ["net_profit", "shareholder_count"], ["net_profit", "net_profit"]]:
            changed = copy.deepcopy(self.matrix)
            changed["entries"][0]["required_fields"] = fields
            with self.assertRaises(ValueError):
                verify(ROOT, changed)

    def test_issuer_bytes_must_match_the_pinned_digest(self):
        changed = copy.deepcopy(self.matrix)
        changed["entries"][0]["authority"][0]["sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            verify(ROOT, changed)


class Facts(unittest.TestCase):
    def test_unknown_dates_and_securities_and_mutated_announcement_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            body = b"SYNTHETIC TEST; NOT MARKET FACT"
            digest = hashlib.sha256(body).hexdigest()
            (root / digest).write_bytes(body)
            calendar = {"version": "test", "source": "test", "start": "2026-07-06",
                        "end": "2026-07-07", "holidays": []}
            history = {"version": "test", "checked_through": "2026-07-07", "intervals": [
                {"code": "sh600000", "start": "2026-07-06", "end": "2026-07-07",
                 "state": "Trading", "announcement": {"url": "https://example.org/test",
                     "sha256": digest, "published": "2026-07-06", "checked": "2026-07-07"}}]}
            universe = {"version": "test", "checked_through": "2026-07-07",
                        "source_sha256": digest, "members": [
                            {"code": "sh600000", "from": "2026-07-06", "through": "2026-07-07"}]}
            for name, record in [("calendar", calendar), ("history", history), ("universe", universe)]:
                (root / f"{name}.json").write_text(json.dumps(record))
            job = {"from": "2026-07-06", "through": "2026-07-07", "planned_date": "2026-07-07",
                   "source_as_of": "2026-07-07", "calendar": str(root / "calendar.json"),
                   "security_history": str(root / "history.json"), "universe": str(root / "universe.json"),
                   "calendar_source_sha256": digest, "facts_directory": str(root),
                   "max_rows": 100, "max_source_bytes": 10000}
            self.assertEqual(audit(job)["expected_keys"], 2)
            history["intervals"][0]["end"] = "2026-07-06"
            (root / "history.json").write_text(json.dumps(history))
            self.assertEqual(audit(job)["states"]["Unknown"], 1)
            history["intervals"][0]["end"] = "2026-07-07"
            history["intervals"].append(copy.deepcopy(history["intervals"][0]))
            (root / "history.json").write_text(json.dumps(history))
            with self.assertRaises(ValueError):
                audit(job)
            history["intervals"].pop()
            (root / "history.json").write_text(json.dumps(history))
            changed = {**job, "through": "2027-01-01", "planned_date": "2027-01-01",
                       "source_as_of": "2027-01-01"}
            with self.assertRaises(ValueError):
                audit(changed)
            (root / digest).write_bytes(b"CHANGED")
            with self.assertRaises(ValueError):
                audit(job)


if __name__ == "__main__":
    unittest.main()
