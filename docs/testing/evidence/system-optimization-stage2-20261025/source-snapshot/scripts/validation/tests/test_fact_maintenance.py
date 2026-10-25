"""Responsibility/deadline/provenance checks use explicitly synthetic fixtures."""
import copy
import datetime as dt
import hashlib
import sys
from pathlib import Path
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from audit_fact_maintenance import CATEGORIES, audit


class Maintenance(unittest.TestCase):
    def test_complete_due_changed_and_missing_categories(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            body = b"synthetic test evidence only"
            for name in ["a", "b"]:
                (root / name).write_bytes(body)
            evidence = [{"path": name, "source": "https://example.org/test-" + name,
                         "sha256": hashlib.sha256(body).hexdigest(),
                         "published": "2026-10-22", "checked": "2026-10-25"}
                        for name in ["a", "b"]]
            entry = {"id": "test", "category": "rules", "owner_role": "test-owner",
                     "reviewer_role": "test-review", "escalation_role": "test-escalation",
                     "scope": "synthetic", "qualification": "independently_reviewed",
                     "reviewed_on": "2026-10-25", "checked_through": "2026-10-25",
                     "next_review": "2026-11-01", "evidence": evidence}
            registry = {"schema_version": 1, "entries": [
                dict(copy.deepcopy(entry), id=c, category=c) for c in sorted(CATEGORIES)]}
            date = dt.date(2026, 10, 25)
            self.assertEqual(audit(registry, root, date)["status"], "covered")
            changed = copy.deepcopy(registry)
            changed["entries"][0]["next_review"] = "2026-10-25"
            self.assertEqual(audit(changed, root, date)["status"], "attention_required")
            changed["entries"].pop()
            self.assertTrue(audit(changed, root, date)["missing_categories"])
            (root / "a").write_bytes(b"changed")
            result = audit(registry, root, date)
            self.assertEqual(result["status"], "attention_required")
            self.assertIn("evidence_changed:a", result["entries"][0]["issues"])

    def test_owner_self_review_and_path_escape_rejected(self):
        row = {"id": "x", "category": "rules", "owner_role": "same",
               "reviewer_role": "same", "escalation_role": "lead", "evidence": []}
        with self.assertRaises(ValueError):
            audit({"schema_version": 1, "entries": [row]}, ".", dt.date(2026, 10, 25))
        row["reviewer_role"] = "independent"
        row["evidence"] = [{"path": "../outside"}]
        with self.assertRaises(ValueError):
            audit({"schema_version": 1, "entries": [row]}, ".", dt.date(2026, 10, 25))


if __name__ == "__main__":
    unittest.main()
