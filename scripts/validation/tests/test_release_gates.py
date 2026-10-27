"""Private synthetic repositories/artifacts verify rejection boundaries; no remote writes."""
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/release"))
from evidence_manifest import bundle, safe_path, verify
from release_gate import check_runs, verify_packages
from source_scope import fingerprint, inventory
import preflight


class ReleaseGates(unittest.TestCase):
    def test_package_cache_selects_only_current_version_and_rejects_missing_or_link(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "jelven-rustdx-complete-2.0.0.crate").write_bytes(b"old")
            current = root / "jelven-rustdx-complete-2.0.2.crate"
            with self.assertRaises(RuntimeError):
                preflight.package_path(root, "jelven-rustdx-complete", "2.0.2")
            current.write_bytes(b"new")
            self.assertEqual(preflight.package_path(root, "jelven-rustdx-complete", "2.0.2"), current)
            current.unlink()
            current.symlink_to(root / "jelven-rustdx-complete-2.0.0.crate")
            with self.assertRaises(RuntimeError):
                preflight.package_path(root, "jelven-rustdx-complete", "2.0.2")

    def test_required_acceptance_inputs_are_not_hidden_by_runtime_ignore_rules(self):
        names = ["config/acceptance/task-matrix.json", "config/acceptance/deployment.example.json",
                 "deploy/acceptance/tool-archives.sha256"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / ".gitignore").write_bytes((ROOT / ".gitignore").read_bytes())
            for name in names:
                self.assertTrue((ROOT / name).is_file(), name)
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("synthetic engineering input")
            (root / "acceptance").mkdir()
            (root / "acceptance/runtime.json").write_text("runtime artifact")
            paths = {row["path"] for row in inventory(root)[0]}
            self.assertTrue(set(names).issubset(paths))
            self.assertNotIn("acceptance/runtime.json", paths)

    def test_engineering_identity_excludes_history_and_includes_new_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "docs/testing/evidence").mkdir(parents=True)
            (root / "docs/testing/evidence/old.bin").write_bytes(b"history")
            (root / "new.rs").write_text("source")
            records, first = inventory(root)
            self.assertEqual([row["path"] for row in records], ["new.rs"])
            (root / "docs/testing/evidence/old.bin").write_bytes(b"changed history")
            self.assertEqual(first, inventory(root)[1])
            (root / "new.rs").write_text("changed source")
            self.assertNotEqual(first, inventory(root)[1])

    def test_evidence_digest_missing_path_escape_duplicate_and_bundle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "sample.bin"
            path.write_bytes(b"frozen bytes")
            rows = [{"path": "sample.bin", "bytes": 12, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                     "availability": "local_retained"}]
            manifest = {"schema_version": 1, "files": rows, "files_sha256": fingerprint(rows)}
            self.assertEqual(verify(root, manifest)["status"], "passed")
            result = bundle(root, manifest, root / "new.tar.gz", "sample")
            self.assertEqual(result["archive"]["bytes"], (root / "new.tar.gz").stat().st_size)
            path.write_bytes(b"changed")
            self.assertEqual(verify(root, manifest)["issues"][0]["reason"], "changed")
            path.unlink()
            self.assertEqual(verify(root, manifest)["issues"][0]["availability"], "local_retained")
            with self.assertRaises(ValueError):
                safe_path(root, "../outside")
            (root / "alias").symlink_to(root)
            with self.assertRaises(ValueError):
                safe_path(root, "alias/sample.bin")
            duplicate = dict(manifest, files=rows * 2, files_sha256=fingerprint(rows * 2))
            with self.assertRaises(ValueError):
                verify(root, duplicate)

    def test_all_platforms_current_sha_latest_attempt_and_workflow_identity(self):
        policy = json.loads((ROOT / "config/release/gates.json").read_text())
        sha = "a" * 40
        workflows = {name: [{"id": index, "head_sha": sha, "event": "push",
                             "path": ".github/workflows/" + name, "status": "completed", "conclusion": "success",
                             "jobs": [{"name": job, "status": "completed", "conclusion": "success"} for job in jobs]}]
                     for index, (name, jobs) in enumerate(policy["workflows"].items(), 1)}
        self.assertFalse(check_runs(sha, policy, workflows)[1])
        workflow = policy["artifact_workflow"]
        for mutation in ["missing_windows", "cancelled", "old_sha", "pull_request", "rerun_failed", "wrong_workflow"]:
            changed = copy.deepcopy(workflows)
            run = changed[workflow][0]
            if mutation == "missing_windows":
                run["jobs"] = [job for job in run["jobs"] if "windows-latest" not in job["name"]]
            elif mutation == "rerun_failed":
                changed[workflow].append(dict(run, id=100, conclusion="failure"))
            else:
                key, value = {"cancelled": ("conclusion", "cancelled"), "old_sha": ("head_sha", "b" * 40),
                              "pull_request": ("event", "pull_request"),
                              "wrong_workflow": ("path", ".github/workflows/other.yml")}[mutation]
                run[key] = value
            self.assertTrue(check_runs(sha, policy, changed)[1], mutation)

    def test_version_receipt_binds_original_push_and_checked_sha(self):
        policy = json.loads((ROOT / "config/release/gates.json").read_text())
        sha, source = "a" * 40, "b" * 40
        event = dict(commit=sha, source_head=source, run_id="123")
        workflow = policy["artifact_workflow"]
        run = dict(id=123, head_sha=source, event="push", path=".github/workflows/" + workflow,
                   status="completed", conclusion="success", run_attempt=2,
                   jobs=[dict(name=job, status="completed", conclusion="success")
                         for job in policy["workflows"][workflow]])
        self.assertFalse(check_runs(sha, policy, {workflow: [run]}, event)[1])
        for changed in [dict(run, id=124), dict(run, head_sha=sha), dict(run, conclusion="failure")]:
            self.assertTrue(check_runs(sha, policy, {workflow: [changed]}, event)[1])
        self.assertTrue(check_runs("c" * 40, policy, {workflow: [run]}, event)[1])
        failed = dict(run, conclusion="failure", run_attempt=3)
        self.assertTrue(check_runs(sha, policy, {workflow: [run, failed]}, event)[1])

    def test_actual_package_artifact_rejects_dirty_mismatched_or_changed_bytes(self):
        policy = json.loads((ROOT / "config/release/gates.json").read_text())
        report = {"schema_version": 2, "head": "a" * 40, "dirty": False,
                  "source_sha256": "b" * 64, "result": "passed",
                  "checks": [{"exit_code": 0}], "packages": [
                      {"name": name, "crate": name + ".crate", "files": ["src/lib.rs"],
                       "sha256": hashlib.sha256(name.encode()).hexdigest()} for name in policy["package_names"]]}

        def artifact(value, corrupt=False):
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, "w") as archive:
                archive.writestr("preflight.json", json.dumps(value))
                for row in value["packages"]:
                    archive.writestr(row["crate"], b"changed" if corrupt else row["name"].encode())
            return stream.getvalue()

        self.assertEqual(len(verify_packages(artifact(report), "a" * 40, "b" * 64, policy)["packages"]), 2)
        for changed in [dict(report, dirty=True), dict(report, head="c" * 40), dict(report, source_sha256="c" * 64)]:
            with self.assertRaises(ValueError):
                verify_packages(artifact(changed), "a" * 40, "b" * 64, policy)
        with self.assertRaises(ValueError):
            verify_packages(artifact(report, True), "a" * 40, "b" * 64, policy)

    def test_offline_fixture_identity_is_available_without_historical_directories(self):
        manifest = json.loads((ROOT / "config/release/offline-fixtures.json").read_text())
        for row in manifest["files"]:
            path = ROOT / row["path"]
            self.assertNotIn("docs/testing/evidence/", row["path"])
            self.assertEqual(path.stat().st_size, row["bytes"])
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), row["sha256"])
