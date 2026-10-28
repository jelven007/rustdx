#!/usr/bin/env python3
"""Bind target admission to installed bytes and the complete keys of one actual job."""
import datetime as dt
import hashlib
import platform
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "validation"))
from audit_task_admission import audit
from audit_fact_maintenance import load


def digest(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError("installed input must be a regular file")
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def check(settings, job_path, plan, now):
    job = load(job_path)
    job_sha = digest(job_path)
    request = load(Path(settings["admissions_directory"]) / (job_sha + ".json"))
    matrix = load(settings["task_matrix"])
    result = audit(matrix, request, settings["admission_evidence_root"])
    issues = list(result["issues"])
    identity = request.get("identity", {})
    installed = load(settings["deployment_identity"])
    actual = {
        "config_sha256": job_sha, "dependency_lock_sha256": digest(settings["dependency_lock"]),
        "binaries": {"rustdx": digest(job["cli"]), "rustdx-runner": digest(settings["runner"])},
        "platform": platform.platform(),
        "facts_sha256": {name: digest(job[name]) for name in ("calendar", "security_history", "universe")},
    }
    for name, value in actual.items():
        if identity.get(name) != value:
            issues.append("installed_identity_differs:" + name)
    for name in ("target_id", "source_commit", "database_versions", "tool_versions", "facts_versions"):
        if not installed.get(name) or identity.get(name) != installed[name]:
            issues.append("installed_provenance_differs:" + name)
    if request.get("matrix_sha256") != digest(settings["task_matrix"]):
        issues.append("installed_matrix_differs")
    try:
        valid = dt.datetime.fromisoformat(request["valid_until"])
        if valid.tzinfo is None or valid <= now:
            issues.append("admission_expired")
    except (KeyError, ValueError, TypeError):
        issues.append("admission_validity_missing")
    members = load(job["universe"])["members"]
    securities = sorted(m["code"] for m in members
                        if m["from"] <= job["through"] and m["through"] >= job["from"])
    scope = request.get("scope", {})
    expected_scope = {"dataset_id": job["dataset_id"],
                      "source_kind": "tcp_day" if job.get("tcp_day") else "local_day",
                      "date_range": [job["from"], job["through"]],
                      "securities": securities, "markets": sorted({s[:2] for s in securities})}
    for name, value in expected_scope.items():
        if scope.get(name) != value:
            issues.append("actual_job_scope_differs:" + name)
    expected = {"count": plan["expected_keys"]["total"],
                "sha256": plan["expected_keys"]["all_keys_jsonl_sha256"]}
    if request.get("expected_key_set") != expected:
        issues.append("actual_complete_keys_differ")
    if request.get("task_type") != "runner_day" or request.get("contract") != job["contract_version"]:
        issues.append("actual_task_contract_differs")
    result.update(status="admission_denied" if issues else "evidence_contract_complete",
                  issues=sorted(set(issues)), actual_identity=actual, actual_scope=expected_scope)
    return result
