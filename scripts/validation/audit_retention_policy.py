#!/usr/bin/env python3
"""Validate explicit independent retention inputs; no deletion, quota or storage writes."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path

CLASSES = {"release", "consumer_audit", "database_evidence", "backup"}


def audit(policy):
    if policy.get("schema_version") != 1 or set(policy.get("classes", {})) != CLASSES:
        raise ValueError("unsupported retention policy or class set")
    issues = []
    if policy.get("protected_references_override_age") is not True:
        issues.append("reachable_release_and_evidence_must_remain_protected")
    reviewer = policy.get("reviewer")
    if not isinstance(reviewer, str) or not reviewer.strip():
        issues.append("independent_reviewer_missing")
    for kind, entry in sorted(policy["classes"].items()):
        owner = entry.get("owner")
        if not isinstance(owner, str) or not owner.strip() or owner == reviewer:
            issues.append(f"owner_or_independent_review_missing:{kind}")
        for name in ["minimum_days", "hard_quota_bytes", "backup_interval_seconds"]:
            value = entry.get(name)
            if type(value) is not int or value <= 0:
                issues.append(f"approved_input_missing:{kind}:{name}")
    if not isinstance(policy.get("immutable_evidence_store"), str) or not policy["immutable_evidence_store"].strip():
        issues.append("immutable_evidence_store_unconfigured")
    try:
        reviewed = datetime.fromisoformat(policy["reviewed_at"])
        if reviewed.tzinfo is None or reviewed > datetime.now(timezone.utc):
            raise ValueError()
    except (KeyError, ValueError, TypeError):
        issues.append("review_timestamp_missing_or_invalid")
    return {"schema_version": 1, "status": "inputs_complete" if not issues else "attention_required",
            "issues": issues,
            "scope": "Input contract only; not enforcement, immutable-store availability or production recovery verification."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.policy.stat().st_size > 1024 * 1024:
        parser.error("policy exceeds 1MiB")
    result = audit(json.loads(args.policy.read_text()))
    with args.output.open("x") as stream:
        stream.write(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))
    return int(bool(result["issues"]))


if __name__ == "__main__":
    raise SystemExit(main())
