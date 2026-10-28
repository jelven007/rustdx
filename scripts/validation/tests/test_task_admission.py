"""Fabricated acceptance envelopes test the validator only, never production claims."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/validation"))
from audit_task_admission import audit


class Admission(unittest.TestCase):
    def test_unknown_template_denied_and_new_tasks_do_not_inherit_daily_contract(self):
        matrix = json.loads((ROOT / "config/acceptance/task-matrix.json").read_text())
        request = json.loads((ROOT / "config/acceptance/deployment.example.json").read_text())
        result = audit(matrix, request, ROOT)
        self.assertEqual(result["status"], "admission_denied")
        self.assertIn("source_commit_required", result["issues"])
        for task in ["minute_bar", "snapshot", "subscription_0547"]:
            changed = dict(request, task_type=task)
            self.assertIn("runner_task_and_release_contract_not_implemented", audit(matrix, changed, ROOT)["issues"])

    def test_complete_scoped_records_wrong_target_unknown_fields_over_limit_and_tampering(self):
        matrix = json.loads((ROOT / "config/acceptance/task-matrix.json").read_text())
        request = json.loads((ROOT / "config/acceptance/deployment.example.json").read_text())
        request["scope"] = {"markets": [0, 1], "securities": ["sh600000"], "date_range": ["2026-10-22", "2026-10-22"],
                            "required_fields": ["date", "close"]}
        request["expected_key_set"] = {"count": 1, "sha256": "a" * 64}
        for name in ["expected_keys", "time_basis", "finality", "source_revision_rule",
                     "disconnect_policy", "overflow_policy", "version_rule"]:
            request[name] = "explicit synthetic test rule"
        request["identity"] = {"source_commit": "a" * 40, "dependency_lock_sha256": "a" * 64,
                               "config_sha256": "a" * 64, "binaries": {"rustdx": "a" * 64, "rustdx-runner": "b" * 64},
                               "target_id": "synthetic-test-only", "platform": "test",
                               "database_versions": "test", "tool_versions": "test", "facts_versions": "test"}
        request["limits"] = {name: 100 for name in [*matrix["resource_limits"], "rpo_seconds", "rto_seconds", "minimum_load_seconds"]}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "original.log").write_bytes(b"synthetic test only")
            original = {"path": "original.log", "sha256": hashlib.sha256((root / "original.log").read_bytes()).hexdigest()}
            reports = {}
            for kind in matrix["required_reports"]:
                reports[kind] = {"schema_version": 1, "kind": kind, "task_type": "runner_day",
                                 "scope": request["scope"], "identity": request["identity"], "status": "passed",
                                 "synthetic": False, "owner_role": "test-owner", "reviewer_role": "test-review",
                                 "recorded_at": "2020-01-01T00:00:00+00:00", "artifacts": [original]}
            reports["field_semantics"]["qualifications"] = {name: "independently_verified_for_scope" for name in matrix["tasks"]["runner_day"]["required_qualifications"]}
            for kind in ["fact_coverage", "full_readback"]:
                reports[kind].update(expected_key_set=request["expected_key_set"], missing_keys=0, extra_keys=0)
            reports["full_readback"]["all_required_fields_equal"] = True
            reports["fault_recovery"]["passed_cases"] = matrix["required_faults"]
            reports["resources"].update(process_tree_measured=True, hard_disk_quota_verified=True,
                                        quota_cases={name: True for name in matrix["required_quota_cases"]},
                                        maximums={name: 50 for name in matrix["resource_limits"]})
            reports["access_controls"].update({name: True for name in ["production_accounts", "acl_isolation", "allowed_directories", "scheduler", "alerts"]})
            reports["access_controls"].update({name: True for name in matrix["required_access_checks"]})
            reports["backup_recovery"].update(control_ledger_and_files=True, database_native=True, fixed_versions_equal=True,
                                              measured={"rpo_seconds": 0, "rto_seconds": 50})
            reports["backup_recovery"].update({name: True for name in matrix["required_recovery_checks"]})
            reports["sustained_load"].update(duration_seconds=100, completed_jobs=10, failed_jobs=0)

            def check(values):
                request["evidence"] = []
                for kind, report in values.items():
                    path = root / (kind + ".json")
                    path.write_text(json.dumps(report))
                    request["evidence"].append({"kind": kind, "path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
                return audit(matrix, request, root)

            self.assertEqual(check(reports)["status"], "evidence_contract_complete")
            for kind, names in [("access_controls", matrix["required_access_checks"]),
                                ("backup_recovery", matrix["required_recovery_checks"])]:
                for name in names:
                    changed = copy.deepcopy(reports)
                    changed[kind][name] = False
                    self.assertEqual(check(changed)["status"], "admission_denied", name)
            for name in matrix["required_quota_cases"]:
                changed = copy.deepcopy(reports)
                del changed["resources"]["quota_cases"][name]
                self.assertEqual(check(changed)["status"], "admission_denied", name)
            for fields in [[], ["close", "close"], ["minute"], "close"]:
                request["scope"]["required_fields"] = fields
                self.assertIn("daily_required_fields_invalid", check(reports)["issues"])
            request["scope"]["required_fields"] = ["date", "close"]
            for mutation in ["target", "unknown", "rss", "recovery", "synthetic", "old_version",
                             "extra_keys", "bool_missing", "bool_failed"]:
                changed = copy.deepcopy(reports)
                if mutation == "target":
                    changed["target_smoke"]["identity"]["target_id"] = "another"
                elif mutation == "unknown":
                    changed["field_semantics"]["qualifications"]["date"] = "unknown"
                elif mutation == "rss":
                    changed["resources"]["maximums"]["process_tree_rss_bytes"] = 101
                elif mutation == "recovery":
                    changed["backup_recovery"]["measured"]["rto_seconds"] = 101
                elif mutation == "synthetic":
                    changed["target_smoke"]["synthetic"] = True
                elif mutation == "old_version":
                    changed["fault_recovery"]["passed_cases"].remove("fixed_old_version_read")
                elif mutation == "bool_missing":
                    changed["full_readback"]["missing_keys"] = False
                elif mutation == "bool_failed":
                    changed["sustained_load"]["failed_jobs"] = False
                else:
                    changed["full_readback"]["extra_keys"] = 1
                self.assertEqual(check(changed)["status"], "admission_denied", mutation)
            check(reports)
            (root / "original.log").write_bytes(b"tampered")
            self.assertIn("original_artifact_changed:target_smoke", audit(matrix, request, root)["issues"])
