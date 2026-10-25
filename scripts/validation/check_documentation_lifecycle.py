#!/usr/bin/env python3
"""Validate critical documentation responsibility, source paths and review cadence."""
import argparse
from datetime import date, datetime, timezone
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIELDS = {"path", "quadrant", "owner", "audience", "source_of_truth", "verified_on",
          "cadence_days", "state", "trigger", "stale_signal", "check"}


def check(root, today=None):
    root = root.resolve()
    current = date.fromisoformat(today) if today else datetime.now(timezone.utc).date()
    policy = json.loads((root / "config/documentation/registry.json").read_text())
    issues = []
    if policy.get("schema_version") != 1:
        issues.append("unsupported registry schema")
    registered = set()
    for row in policy["critical_documents"]:
        name = row.get("path", "")
        if FIELDS - row.keys() or any(not row.get(key) for key in FIELDS):
            issues.append(f"{name}: missing lifecycle field")
            continue
        if name in registered:
            issues.append(f"{name}: duplicate registration")
        registered.add(name)
        if row["quadrant"] not in {"tutorial", "how-to", "reference", "explanation"}:
            issues.append(f"{name}: invalid quadrant")
        if row["state"] != "source-verified":
            issues.append(f"{name}: not source verified")
        for key in ["path", "source_of_truth"]:
            relative = Path(row[key])
            try:
                if relative.is_absolute() or ".." in relative.parts:
                    raise ValueError("unsafe path")
                path = root / relative
                path.resolve().relative_to(root)
                if not path.exists() or path.is_symlink():
                    raise ValueError("missing or symlink")
            except ValueError:
                issues.append(f"{name}: unsafe or missing {key}")
        try:
            age = (current - date.fromisoformat(row["verified_on"])).days
            cadence = row["cadence_days"]
            if type(cadence) is not int or not 1 <= cadence <= 365 or age < 0 or age > cadence:
                raise ValueError("stale or future date")
        except (ValueError, TypeError):
            issues.append(f"{name}: stale/invalid verification date or cadence")
        path = root / name
        if path.is_file() and "<!-- lifecycle:registry -->" not in path.read_text():
            issues.append(f"{name}: lifecycle registry marker missing")
    for directory in ["requirements", "architecture", "development", "reference",
                      "testing", "operations", "runbooks", "release"]:
        for path in (root / "docs" / directory).rglob("*.md"):
            if "evidence" not in path.relative_to(root).parts:
                name = path.relative_to(root).as_posix()
                if name not in registered:
                    issues.append(f"{name}: unregistered critical document")
    return dict(schema_version=1, status="failed" if issues else "passed",
                checked_on=current.isoformat(), documents=len(registered), issues=issues,
                scope="Static lifecycle and cadence; source-change semantics require PR review.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--today")
    args = parser.parse_args()
    try:
        result = check(args.root, args.today)
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f"documentation lifecycle: {error}\n")
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
