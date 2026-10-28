#!/usr/bin/env python3
"""Read a task matrix and bound target acceptance records; never deploy or collect."""
import argparse
import datetime as dt
import hashlib
import json
import math
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
from evidence_manifest import safe_path


def positive_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and value > 0


def sha_value(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def nonnegative_integer(value):
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def audit(matrix, request, root):
    if matrix.get("schema_version") != 1 or request.get("schema_version") != 1:
        raise ValueError("unsupported task contract")
    root = Path(root).resolve()
    task_type = request.get("task_type")
    if task_type not in matrix["tasks"]:
        raise ValueError("task type is absent from the evidence matrix")
    task = matrix["tasks"][task_type]
    issues = []
    if not task["runner_entry"]:
        issues.append("runner_task_and_release_contract_not_implemented")
    if task["contract"] != request.get("contract"):
        issues.append("task_contract_differs")
    scope = request.get("scope")
    if not isinstance(scope, dict) or not scope or any(value in [None, "", "unknown"] for value in scope.values()):
        issues.append("explicit_scope_required")
    if task_type == "runner_day":
        allowed = {"date", "market", "code", "instrument_class", "open", "high", "low",
                   "close", "amount_cny", "volume_shares"}
        fields = scope.get("required_fields") if isinstance(scope, dict) else None
        if (not isinstance(fields, list) or not fields or
                not all(isinstance(f, str) and f in allowed for f in fields) or
                len(set(fields)) != len(fields)):
            issues.append("daily_required_fields_invalid")
    for name in ["expected_keys", "time_basis", "finality", "source_revision_rule",
                 "disconnect_policy", "overflow_policy", "version_rule"]:
        if not isinstance(request.get(name), str) or request[name] in ["", "unknown"]:
            issues.append(f"task_rule_missing:{name}")
    identity = request.get("identity", {})
    if not re.fullmatch(r"[0-9a-f]{40}", str(identity.get("source_commit", ""))):
        issues.append("source_commit_required")
    for name in ["config_sha256", "dependency_lock_sha256"]:
        if not sha_value(identity.get(name)):
            issues.append(f"identity_missing:{name}")
    binaries = identity.get("binaries", {})
    if set(binaries) != {"rustdx", "rustdx-runner"} or not all(sha_value(value) for value in binaries.values()):
        issues.append("both_binary_digests_required")
    for name in ["target_id", "platform", "database_versions", "tool_versions", "facts_versions"]:
        if not identity.get(name) or identity[name] == "unknown":
            issues.append(f"target_identity_missing:{name}")
    limits = request.get("limits", {})
    for name in [*matrix["resource_limits"], "rpo_seconds", "rto_seconds", "minimum_load_seconds"]:
        if not positive_number(limits.get(name)):
            issues.append(f"approved_limit_missing:{name}")
    evidence = request.get("evidence", [])
    if not isinstance(evidence, list) or len(evidence) > 100:
        raise ValueError("acceptance evidence budget")
    reports = {}
    receipts = []
    declared = set()
    for record in evidence:
        kind = record.get("kind")
        if kind not in matrix["required_reports"] or kind in declared:
            raise ValueError("unrecognized or duplicate acceptance report kind")
        declared.add(kind)
        path = safe_path(root, record["path"])
        if not path.is_file():
            issues.append(f"evidence_missing:{kind}")
            continue
        if path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("acceptance report size budget")
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != record.get("sha256"):
            issues.append(f"evidence_changed:{kind}")
            continue
        report = json.loads(data)
        reports[kind] = report
        receipts.append(dict(kind=kind, path=record["path"], sha256=digest))
        if (report.get("schema_version") != 1 or report.get("kind") != kind
                or report.get("task_type") != task_type or report.get("scope") != scope
                or report.get("identity") != identity):
            issues.append(f"evidence_scope_or_identity_differs:{kind}")
        if report.get("status") != "passed" or report.get("synthetic") is not False:
            issues.append(f"target_acceptance_not_passed:{kind}")
        if (not report.get("owner_role") or not report.get("reviewer_role")
                or report["owner_role"] == report["reviewer_role"]):
            issues.append(f"independent_review_missing:{kind}")
        try:
            recorded = dt.datetime.fromisoformat(report.get("recorded_at", ""))
            if recorded.tzinfo is None or recorded > dt.datetime.now(dt.timezone.utc):
                issues.append(f"invalid_recorded_at:{kind}")
        except (ValueError, TypeError):
            issues.append(f"invalid_recorded_at:{kind}")
        # Every curated envelope also binds actual commands/logs/original artifacts.
        artifacts = report.get("artifacts", [])
        if not isinstance(artifacts, list) or not 1 <= len(artifacts) <= 1000:
            issues.append(f"original_artifacts_missing:{kind}")
        else:
            for artifact in artifacts:
                original = safe_path(root, artifact["path"])
                if not original.is_file():
                    issues.append(f"original_artifact_missing:{kind}")
                    continue
                digest = hashlib.sha256()
                with original.open("rb") as stream:
                    for block in iter(lambda: stream.read(1024 * 1024), b""):
                        digest.update(block)
                if digest.hexdigest() != artifact.get("sha256"):
                    issues.append(f"original_artifact_changed:{kind}")
    for kind in matrix["required_reports"]:
        if kind not in reports:
            issues.append(f"target_report_missing:{kind}")
    fields = reports.get("field_semantics", {}).get("qualifications", {})
    for name in task["required_qualifications"]:
        if fields.get(name) != "independently_verified_for_scope":
            issues.append(f"required_qualification_missing:{name}")
    for kind in ["fact_coverage", "full_readback"]:
        report = reports.get(kind, {})
        expected = request.get("expected_key_set", {})
        if (not sha_value(expected.get("sha256")) or not nonnegative_integer(expected.get("count"))
                or expected.get("count", 0) == 0
                or report.get("expected_key_set") != expected or report.get("missing_keys") != 0
                or report.get("extra_keys") != 0
                or not nonnegative_integer(report.get("missing_keys"))
                or not nonnegative_integer(report.get("extra_keys"))):
            issues.append(f"complete_key_set_not_verified:{kind}")
    readback = reports.get("full_readback", {})
    if readback.get("all_required_fields_equal") is not True:
        issues.append("full_field_readback_required")
    faults = reports.get("fault_recovery", {}).get("passed_cases", [])
    for case in matrix["required_faults"]:
        if case not in faults:
            issues.append(f"fault_case_missing:{case}")
    resources = reports.get("resources", {})
    for case in matrix["required_quota_cases"]:
        if resources.get("quota_cases", {}).get(case) is not True:
            issues.append("quota_case_unverified:" + case)
    if resources.get("process_tree_measured") is not True or resources.get("hard_disk_quota_verified") is not True:
        issues.append("process_tree_and_hard_quota_required")
    for name in matrix["resource_limits"]:
        actual = resources.get("maximums", {}).get(name)
        if not positive_number(actual) or not positive_number(limits.get(name)) or actual > limits[name]:
            issues.append(f"resource_limit_unverified_or_exceeded:{name}")
    access = reports.get("access_controls", {})
    for name in ["production_accounts", "acl_isolation", "allowed_directories", "scheduler", "alerts",
                 *matrix["required_access_checks"]]:
        if access.get(name) is not True:
            issues.append(f"access_control_unverified:{name}")
    recovery = reports.get("backup_recovery", {})
    for name in ["control_ledger_and_files", "database_native", "fixed_versions_equal",
                 *matrix["required_recovery_checks"]]:
        if recovery.get(name) is not True:
            issues.append(f"recovery_unverified:{name}")
    for name in ["rpo_seconds", "rto_seconds"]:
        value = recovery.get("measured", {}).get(name)
        if (isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value)
                or value < 0 or not positive_number(limits.get(name)) or value > limits[name]):
            issues.append(f"recovery_objective_unverified_or_exceeded:{name}")
    load = reports.get("sustained_load", {})
    if (not positive_number(load.get("duration_seconds"))
            or not positive_number(limits.get("minimum_load_seconds"))
            or load["duration_seconds"] < limits["minimum_load_seconds"]):
        issues.append("sustained_load_duration_not_verified")
    if (not nonnegative_integer(load.get("completed_jobs")) or load.get("completed_jobs", 0) == 0
            or not nonnegative_integer(load.get("failed_jobs")) or load.get("failed_jobs") != 0):
        issues.append("sustained_load_outcome_not_verified")
    issues = sorted(set(issues))
    return {"schema_version": 1, "task_type": task_type,
            "status": "evidence_contract_complete" if not issues else "admission_denied",
            "runner_entry": task["runner_entry"], "issues": issues, "evidence": receipts,
            "known_boundaries": task["open_boundaries"], "meaning": matrix["approval_meaning"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path, default=Path(__file__).resolve().parents[2] / "config/acceptance/task-matrix.json")
    parser.add_argument("--request", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("use a new admission report")
    matrix_data, request_data = args.matrix.read_bytes(), args.request.read_bytes()
    result = audit(json.loads(matrix_data), json.loads(request_data), args.evidence_root)
    result.update(matrix_sha256=hashlib.sha256(matrix_data).hexdigest(),
                  request_sha256=hashlib.sha256(request_data).hexdigest())
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "issues": result["issues"]}))
    return 0 if result["status"] == "evidence_contract_complete" else 1


if __name__ == "__main__":
    raise SystemExit(main())
