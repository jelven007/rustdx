"""Installed binding regression; report qualification is tested separately in test_task_admission."""
import copy
import datetime as dt
import json
from pathlib import Path
import platform
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/ops"))
import production_gate as gate


class InstalledGate(unittest.TestCase):
    def test_actual_job_installed_hashes_scope_keys_expiry_and_missing_admission(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            settings = {name: str(root / name) for name in
                        ("runner", "dependency_lock", "deployment_identity", "task_matrix",
                         "admissions_directory", "admission_evidence_root")}
            Path(settings["admissions_directory"]).mkdir()
            for name in ("runner", "dependency_lock", "cli", "calendar", "security_history"):
                (root / name).write_bytes(b"synthetic installed bytes:" + name.encode())
            (root / "universe").write_text(json.dumps({"members": [
                {"code": "sh600000", "from": "2026-10-01", "through": "2026-10-28"},
                {"code": "sz000001", "from": "2026-11-01", "through": "2026-11-02"}]}))
            job = {name: str(root / name) for name in ("cli", "calendar", "security_history", "universe")}
            job.update(dataset_id="fixture", contract_version="a-share/day/raw/cny/shares/v1",
                       **{"from": "2026-10-27", "through": "2026-10-27"})
            job_path = root / "job.json"
            job_path.write_text(json.dumps(job))
            Path(settings["task_matrix"]).write_text('{"schema_version":1}')
            installed = {"target_id": "fixture", "source_commit": "a" * 40, "database_versions": "fixture",
                         "tool_versions": "fixture", "facts_versions": "fixture"}
            Path(settings["deployment_identity"]).write_text(json.dumps(installed))
            identity = dict(installed, config_sha256=gate.digest(job_path),
                            dependency_lock_sha256=gate.digest(settings["dependency_lock"]),
                            binaries={"rustdx": gate.digest(job["cli"]), "rustdx-runner": gate.digest(settings["runner"])},
                            platform=platform.platform(),
                            facts_sha256={n: gate.digest(job[n]) for n in ("calendar", "security_history", "universe")})
            plan = {"expected_keys": {"total": 1, "all_keys_jsonl_sha256": "b" * 64}}
            request = {"identity": identity, "schema_version": 1, "task_type": "runner_day",
                       "contract": job["contract_version"], "matrix_sha256": gate.digest(settings["task_matrix"]),
                       "valid_until": "2026-10-28T23:00:00+08:00",
                       "scope": {"dataset_id": "fixture", "source_kind": "local_day", "markets": ["sh"],
                                 "securities": ["sh600000"], "date_range": ["2026-10-27", "2026-10-27"]},
                       "expected_key_set": {"count": 1, "sha256": "b" * 64}}
            path = Path(settings["admissions_directory"]) / (gate.digest(job_path) + ".json")
            now = dt.datetime.fromisoformat("2026-10-28T19:00:00+08:00")

            def check(value):
                path.write_text(json.dumps(value))
                # Isolate exact installed binding; synthetic report envelope validation has its own suite.
                with patch.object(gate, "audit", return_value={"status": "evidence_contract_complete", "issues": []}):
                    return gate.check(settings, job_path, plan, now)

            self.assertEqual(check(request)["status"], "evidence_contract_complete")
            for field in ("config_sha256", "dependency_lock_sha256", "platform", "facts_sha256", "binaries",
                          "source_commit", "target_id", "database_versions", "tool_versions", "facts_versions"):
                bad = copy.deepcopy(request)
                bad["identity"][field] = "changed"
                self.assertEqual(check(bad)["status"], "admission_denied", field)
            for field in ("dataset_id", "source_kind", "markets", "securities", "date_range"):
                bad = copy.deepcopy(request)
                bad["scope"][field] = "changed"
                self.assertEqual(check(bad)["status"], "admission_denied", field)
            for field, value in (("valid_until", "2026-10-28T18:00:00+08:00"),
                                 ("valid_until", "2026-10-28T23:00:00"),
                                 ("matrix_sha256", "a" * 64),
                                 ("expected_key_set", {"count": 2, "sha256": "b" * 64}),
                                 ("task_type", "snapshot")):
                bad = copy.deepcopy(request)
                bad[field] = value
                self.assertEqual(check(bad)["status"], "admission_denied", field)
            path.unlink()
            with self.assertRaises(ValueError):
                gate.check(settings, job_path, plan, now)


if __name__ == "__main__":
    unittest.main()
