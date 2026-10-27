import copy
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/validation"))
from audit_retention_policy import audit


class RetentionPolicyTests(unittest.TestCase):
    def test_missing_inputs_and_independent_class_budgets(self):
        example = json.loads((ROOT / "config/maintenance/retention.example.json").read_text())
        self.assertEqual(audit(example)["status"], "attention_required")
        policy = copy.deepcopy(example)
        policy.update(immutable_evidence_store="synthetic-test-store", reviewer="test-reviewer",
                      reviewed_at="2026-01-01T00:00:00+00:00")
        for entry in policy["classes"].values():
            entry.update(owner="test-owner", minimum_days=1, hard_quota_bytes=1024,
                         backup_interval_seconds=60)
        self.assertEqual(audit(policy)["status"], "inputs_complete")
        for kind in policy["classes"]:
            for field in ["minimum_days", "hard_quota_bytes", "backup_interval_seconds"]:
                changed = copy.deepcopy(policy)
                changed["classes"][kind][field] = True
                self.assertEqual(audit(changed)["status"], "attention_required")
        policy["protected_references_override_age"] = False
        self.assertIn("reachable_release_and_evidence_must_remain_protected", audit(policy)["issues"])
