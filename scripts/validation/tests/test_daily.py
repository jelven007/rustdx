"""Synthetic calendars and reviews test scheduling; actual runner plan is opt-in."""
import copy
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/ops"))
import daily
from audit_fact_maintenance import audit_job


def write(path, value):
    path.write_bytes(daily.encoded(value))


def fixture(root):
    now = dt.datetime(2026, 10, 28, 19, tzinfo=daily.ZONE)
    evidence = root / "evidence"
    evidence.mkdir()
    body = b"SYNTHETIC TEST ONLY; NOT MARKET FACTS"
    sha = hashlib.sha256(body).hexdigest()
    for name in (sha, "review-a", "review-b"):
        (evidence / name).write_bytes(body)
    calendar = {"version": "fixture", "source": "fixture-primary", "start": "2026-01-01",
                "end": "2026-12-31", "holidays": ["2026-10-26"]}
    write(root / "calendar.json", calendar)
    write(root / "reference.json", dict(calendar, source="fixture-independent"))
    write(root / "history.json", {"version": "fixture", "checked_through": "2026-10-28", "intervals": [
        {"code": "sh600000", "start": "2026-10-19", "end": "2026-10-28", "board": "Main",
         "st": False, "stage": "Seasoned", "state": "Trading",
         "announcement": {"url": "https://example.org/fixture", "sha256": sha,
                          "published": "2026-10-19", "checked": "2026-10-28"}}]})
    write(root / "universe.json", {"version": "fixture", "checked_through": "2026-10-28",
                                  "source_sha256": sha, "members": [
        {"code": "sh600000", "from": "2026-10-19", "through": "2026-10-28"}]})
    source = root / "day"
    source.mkdir()
    (source / "sh600000.day").write_bytes(b"\0" * 32)
    cli = os.environ.get("RUSTDX_TEST_CLI", str(root / "fake-cli"))
    if cli == str(root / "fake-cli"):
        Path(cli).write_bytes(b"fixture nonexecuted collector")
    job = {"schema_version": 1, "root": str(root / "live"), "cli": cli, "no_tcp_startup": True,
           "dataset_id": "fixture", "contract_version": daily.CONTRACT,
           "policy_revision": 1, "source_revision": 1, "source_as_of": "2026-10-28",
           "planned_date": "2026-10-28", "from": "2026-10-19", "through": "2026-10-27",
           "calendar": str(root / "calendar.json"), "security_history": str(root / "history.json"),
           "universe": str(root / "universe.json"), "facts_directory": str(evidence),
           "calendar_source_sha256": sha, "day_directories": [str(source)], "database_targets": [],
           "timeout_seconds": 60, "publish_reserve_seconds": 10, "cancel_grace_seconds": 1,
           "workers": 1, "max_source_bytes": 1024 * 1024, "max_workspace_bytes": 256 * 1024 * 1024,
           "max_rows": 100, "min_free_bytes": 0}
    write(root / "template.json", job)
    settings = {name: str(root / name) for name in daily.PATHS}
    settings.update(schema_version=1, task_type="runner_day", template=str(root / "template.json"),
                    runner=os.environ.get("RUSTDX_TEST_RUNNER", "/not-executed"),
                    calendar_reference=str(root / "reference.json"), evidence_root=str(evidence),
                    initial_from="2026-10-19", publication_lag_days=1, publish_time="18:00",
                    max_jobs_per_run=2, audit_max_bytes=1024 * 1024)
    ready = {"schema_version": 1, "status": "complete",
             "template_sha256": hashlib.sha256((root / "template.json").read_bytes()).hexdigest(),
             "source_revision": 1, "source_as_of": "2026-10-28", "complete_through": "2026-10-27",
             "observed_at": "2026-10-28T18:00:00+08:00", "valid_until": "2026-10-28T23:00:00+08:00"}
    write(Path(settings["source_ready"]), ready)
    evidence_rows = [{"path": "review-" + n, "source": "https://example.org/test-" + n,
                      "sha256": sha, "published": "2026-10-19", "checked": "2026-10-28"} for n in "ab"]
    bindings = {"calendar": ("calendar", "calendar_reference"), "listing_delisting": ("security_history",),
                "st_suspension": ("security_history",), "universe": ("universe",), "rules": ()}
    registry = {"schema_version": 1, "entries": []}
    for category, names in bindings.items():
        inputs = {n: daily.production_gate.digest(settings["calendar_reference"] if n == "calendar_reference"
                                                else job[n]) for n in names}
        registry["entries"].append({"id": category, "category": category, "scope": "synthetic",
                                    "owner_role": "test-owner", "reviewer_role": "test-reviewer",
                                    "escalation_role": "test-lead", "qualification": "independently_reviewed",
                                    "reviewed_on": "2026-10-28", "checked_from": "2026-10-19",
                                    "checked_through": "2026-10-28", "next_review": "2026-11-01",
                                    "evidence": copy.deepcopy(evidence_rows),
                                    "binding": {"dataset_id": "fixture", "inputs": inputs}})
    write(Path(settings["fact_registry"]), registry)
    return settings, job, registry, ready, now


class Daily(unittest.TestCase):
    def test_calendar_backlog_expiry_late_source_and_batch_cap_preserve_all_dates(self):
        with tempfile.TemporaryDirectory() as directory:
            settings, job, _, ready, now = fixture(Path(directory))
            dates = daily.expected(settings, job, now)
            self.assertEqual(list(map(str, dates)),
                             ["2026-10-19", "2026-10-20", "2026-10-21", "2026-10-22", "2026-10-23", "2026-10-27"])
            with patch.object(daily, "qualify", side_effect=lambda s, j, n:
                              (daily.store_job(s, j), {"can_start": True}, {"status": "covered"})):
                result = daily.prepare(settings, now)
                self.assertEqual(len(result["jobs"]), 2)
                self.assertEqual(len(result["pending"]), 6)
                self.assertEqual(len(result["overdue"]), 6)
                self.assertEqual(result["jobs"], daily.prepare(settings, now)["jobs"])
                ready["complete_through"] = "2026-10-19"
                write(Path(settings["source_ready"]), ready)
                self.assertEqual(len(daily.prepare(settings, now)["jobs"]), 1)
                ready["valid_until"] = "2026-10-28T18:30:00+08:00"
                write(Path(settings["source_ready"]), ready)
                expired = daily.prepare(settings, now)
                self.assertEqual(len(expired["pending"]), 6)
                self.assertEqual(expired["jobs"], [])
                self.assertIn("expired", expired["source_issue"])

    def test_explicit_revision_and_tcp_today_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            settings, job, _, ready, now = fixture(Path(directory))
            dates = daily.expected(settings, job, now)
            old = {str(d): {"source_revision": 1, "source_as_of": "2026-10-27"} for d in dates}
            self.assertEqual(daily.queue(settings, job, dates, old, ready, now)[0], {})
            ready.update(source_revision=2, revision_range=["2026-10-20", "2026-10-21"])
            self.assertEqual(len(daily.queue(settings, job, dates, old, ready, now)[0]), 2)
            ready["source_revision"] = 1
            with self.assertRaisesRegex(ValueError, "must increase"):
                daily.queue(settings, job, dates, old, ready, now)
            job["tcp_day"] = {"fixture": True}
            ready["source_as_of"] = "2026-10-27"
            write(Path(settings["source_ready"]), ready)
            with self.assertRaises(ValueError):
                daily.source(settings, job, now)

    def test_actual_fact_binding_range_due_review_and_cli_denial(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            settings, job, registry, _, now = fixture(root)
            result = audit_job(registry, settings["evidence_root"], now.date(), job, settings["calendar_reference"])
            self.assertEqual(result["job_status"], "covered")
            registry["entries"][-1]["next_review"] = "2026-10-28"
            self.assertEqual(audit_job(registry, settings["evidence_root"], now.date(), job,
                                      settings["calendar_reference"])["job_status"], "covered")
            registry["entries"][-1]["next_review"] = "2026-11-01"
            registry["entries"][0]["binding"]["dataset_id"] = "another-dataset"
            bad = audit_job(registry, settings["evidence_root"], now.date(), job, settings["calendar_reference"])
            self.assertEqual(bad["status"], "covered")
            self.assertEqual(bad["job_status"], "attention_required")
            write(Path(settings["fact_registry"]), registry)
            output = root / "audit.json"
            process = subprocess.run([sys.executable, str(ROOT / "scripts/validation/audit_fact_maintenance.py"),
                                      "--registry", settings["fact_registry"], "--evidence-root", settings["evidence_root"],
                                      "--as-of", "2026-10-28", "--config", settings["template"],
                                      "--calendar-reference", settings["calendar_reference"], "--output", str(output)],
                                     capture_output=True, text=True)
            self.assertEqual(process.returncode, 1, process.stderr)
            self.assertEqual(json.loads(output.read_text())["job_status"], "attention_required")

    def test_independent_calendar_and_live_monitor_do_not_trust_stored_queue(self):
        with tempfile.TemporaryDirectory() as directory:
            settings, job, _, _, now = fixture(Path(directory))
            with patch.object(daily, "qualify", side_effect=ValueError("unreviewed facts")), \
                    patch.object(daily, "call", return_value="rustdx_business_metrics_timestamp_seconds 1\n"):
                first = daily.monitor(settings, now)
                self.assertEqual(len(first["overdue"]), 6)
                write(Path(settings["state_directory"]) / "current.json", {"pending": {}})
                self.assertEqual(len(daily.monitor(settings, now)["overdue"]), 6)
                self.assertEqual(Path(settings["metrics_output"]).stat().st_mode & 0o777, 0o644)
            ref = daily.load(settings["calendar_reference"])
            ref["holidays"] = []
            write(Path(settings["calendar_reference"]), ref)
            with self.assertRaisesRegex(ValueError, "disagree"):
                daily.expected(settings, job, now)

    def test_denied_admission_never_recovers_or_runs_and_returns_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            settings, job, _, _, now = fixture(Path(directory))
            path = daily.store_job(settings, job)
            result = {"jobs": [{"date": job["from"], "path": str(path)}],
                      "blocked": [], "source_issue": None}
            with patch.object(daily, "prepare", return_value=result), \
                    patch.object(daily, "qualify", return_value=(path, {}, {})), \
                    patch.object(daily.production_gate, "check", side_effect=FileNotFoundError("approval missing")), \
                    patch.object(daily, "call") as runner, \
                    patch.object(daily, "monitor", return_value=dict(result)):
                denied = daily.run(settings)
                runner.assert_not_called()
                self.assertEqual(denied["admission_denied"][0]["date"], job["from"])
                self.assertIn("approval missing", denied["admission_denied"][0]["issues"])
            with patch.object(daily, "settings_at", return_value=settings), \
                    patch.object(daily, "run", return_value=denied), \
                    patch.object(sys, "argv", ["daily.py", "run", "--settings", "/fixture"]), \
                    patch("builtins.print"):
                self.assertEqual(daily.main(), 1)

    @unittest.skipUnless(os.environ.get("RUSTDX_TEST_RUNNER"), "requires an actual built runner")
    def test_actual_runner_plan_accepts_post_rule_cutoff_state_facts(self):
        with tempfile.TemporaryDirectory() as directory:
            settings, _, _, _, now = fixture(Path(directory))
            result = daily.prepare(settings, now)
            self.assertEqual(result["blocked"], [])
            self.assertEqual(len(result["jobs"]), 2)
            self.assertEqual(result["jobs"][0]["plan"]["expected_keys"]["total"], 1)
            self.assertFalse((Path(directory) / "live").exists())

    @unittest.skipUnless(os.environ.get("RUSTDX_TEST_CLI"), "requires an actual built CLI")
    def test_actual_cli_state_only_preserves_announcement_checks_and_old_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            settings, job, _, _, _ = fixture(root)
            command = [job["cli"], "--no-tcp-startup", "data-check", "security", job["security_history"],
                       "--evidence-dir", settings["evidence_root"]]
            strict = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(strict.returncode, 0)
            output = root / "published.json"
            state = subprocess.run(command + ["--state-only", "--output", str(output)],
                                   capture_output=True, text=True)
            self.assertEqual(state.returncode, 0, state.stderr)
            original = output.read_bytes()
            history = daily.load(job["security_history"])
            announcement = Path(settings["evidence_root"]) / history["intervals"][0]["announcement"]["sha256"]
            announcement.write_bytes(b"changed original announcement")
            bad = subprocess.run(command + ["--state-only", "--output", str(output)],
                                 capture_output=True, text=True)
            self.assertNotEqual(bad.returncode, 0)
            self.assertEqual(output.read_bytes(), original)

    @unittest.skipUnless(os.environ.get("RUSTDX_TEST_RUNNER"), "requires an actual built runner")
    def test_actual_tcp_plan_and_scheduled_run_keep_source_networking_enabled(self):
        with tempfile.TemporaryDirectory() as directory:
            settings, job, _, ready, now = fixture(Path(directory))
            job.update(no_tcp_startup=False, day_directories=[], max_source_bytes=67108864,
                       max_workspace_bytes=536870912,
                       tcp_day={"endpoint": "127.0.0.1:7709",
                                "volume_evidence_sha256": job["calendar_source_sha256"],
                                "page_size": 800, "max_pages": 128, "max_raw_rows": 65536,
                                "max_payload_bytes": 16777216, "max_wire_records": 20000})
            write(Path(settings["template"]), job)
            ready["template_sha256"] = daily.production_gate.digest(settings["template"])
            write(Path(settings["source_ready"]), ready)
            result = daily.prepare(settings, now)
            self.assertEqual(result["blocked"], [])
            self.assertEqual(len(result["jobs"]), 2)
            self.assertFalse(Path(job["root"]).exists())
            item = result["jobs"][0]
            actual = daily.load(item["path"])
            single = dict(result, jobs=[item])
            with patch.object(daily, "prepare", return_value=single), \
                    patch.object(daily, "source", return_value=ready), \
                    patch.object(daily, "qualify", return_value=(Path(item["path"]), item["plan"], {})), \
                    patch.object(daily.production_gate, "check", return_value={
                        "status": "evidence_contract_complete", "issues": []}), \
                    patch.object(daily, "catalog", return_value={item["date"]: actual}), \
                    patch.object(daily, "monitor", return_value=dict(single)), \
                    patch.object(daily, "call") as runner:
                daily.run(settings)
                run_call = next(c for c in runner.call_args_list if c.args[1] == "run")
                self.assertNotIn("--no-tcp-startup", run_call.args)


if __name__ == "__main__":
    unittest.main()
