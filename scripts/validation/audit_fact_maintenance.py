#!/usr/bin/env python3
"""Audit evidence-backed ownership, coverage and review deadlines; never renew facts."""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path

CATEGORIES = {"rules", "calendar", "listing_delisting", "st_suspension", "universe"}
LIMIT = 32 * 1024 * 1024


def load(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > LIMIT:
        raise ValueError("maintenance input must be a bounded regular file")
    return json.loads(path.read_bytes())


def audit(registry, directory, as_of, through=None):
    if registry.get("schema_version") != 1 or not 1 <= len(registry["entries"]) <= 1000:
        raise ValueError("unsupported maintenance registry")
    required = through or as_of
    entries, categories, ids = [], set(), set()
    directory = Path(directory).resolve()
    for row in registry["entries"]:
        identifier, category = row["id"], row["category"]
        if (not isinstance(identifier, str) or not identifier or identifier in ids
                or category not in CATEGORIES or not row.get("owner_role")
                or not row.get("reviewer_role") or row["owner_role"] == row["reviewer_role"]
                or not row.get("escalation_role")):
            raise ValueError("maintenance id/category or independent responsibility invalid")
        ids.add(identifier)
        categories.add(category)
        issues, verified = [], []
        for item in row["evidence"]:
            relative = Path(item["path"])
            if relative.is_absolute() or any(p in ("", ".", "..") for p in item["path"].split("/")):
                raise ValueError("unsafe maintenance evidence path")
            path = directory
            for part in relative.parts:
                path = path / part
                if path.is_symlink():
                    raise ValueError("maintenance evidence symlink forbidden")
            if (not isinstance(item.get("source"), str) or not item["source"].startswith("https://")
                    or len(item["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in item["sha256"])):
                raise ValueError("invalid evidence provenance")
            if not path.is_file():
                issues.append("evidence_missing:" + item["path"])
                continue
            if path.stat().st_size > LIMIT or hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
                issues.append("evidence_changed:" + item["path"])
                continue
            published, checked = map(dt.date.fromisoformat, (item["published"], item["checked"]))
            if published > checked or checked > as_of:
                issues.append("evidence_dates_invalid:" + item["path"])
            verified.append({"path": item["path"], "sha256": item["sha256"], "source": item["source"]})
        if row["qualification"] != "independently_reviewed" or not verified:
            issues.append("independent_review_missing")
        if len({item["source"] for item in verified}) < 2:
            issues.append("independent_source_missing")
        reviewed = dt.date.fromisoformat(row["reviewed_on"]) if row.get("reviewed_on") else None
        next_review = dt.date.fromisoformat(row["next_review"]) if row.get("next_review") else None
        covered = dt.date.fromisoformat(row["checked_through"]) if row.get("checked_through") else None
        if reviewed is None or reviewed > as_of or next_review is None or (reviewed and next_review < reviewed):
            issues.append("review_dates_missing_or_invalid")
        if next_review and next_review <= as_of:
            issues.append("review_due")
        if covered is None or covered < required:
            issues.append("coverage_insufficient")
        # Calendar plans may cover future dates; they still require published evidence.
        if category != "calendar" and covered and reviewed and covered > reviewed:
            issues.append("fact_coverage_exceeds_review_date")
        entries.append({"id": identifier, "category": category, "owner_role": row["owner_role"],
                        "reviewer_role": row["reviewer_role"], "escalation_role": row["escalation_role"],
                        "scope": row["scope"], "status": "attention_required" if issues else "covered",
                        "issues": issues, "evidence": verified, "checked_through": row.get("checked_through"),
                        "next_review": row.get("next_review")})
    missing = sorted(CATEGORIES - categories)
    return {"schema_version": 1, "as_of": str(as_of), "required_through": str(required),
            "status": "attention_required" if missing or any(r["issues"] for r in entries) else "covered",
            "missing_categories": missing, "entries": entries,
            "scope": "Declared responsibilities and pinned bytes only; independent semantic/point-in-time audit remains required.",
            "action": "No notification, publication, evidence rewriting or deadline renewal performed."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--as-of", type=dt.date.fromisoformat,
                        default=dt.datetime.now(dt.timezone(dt.timedelta(hours=8))).date())
    parser.add_argument("--through", type=dt.date.fromisoformat)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = audit(load(args.registry), args.evidence_root, args.as_of, args.through)
    with args.output.open("x") as stream:
        stream.write(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"status": result["status"], "missing_categories": result["missing_categories"]}))
    raise SystemExit(0 if result["status"] == "covered" else 1)


if __name__ == "__main__":
    main()
