#!/usr/bin/env python3
"""Read-only announcement integrity and expected-date/security coverage audit."""
import argparse
import bisect
import datetime as dt
import hashlib
import json
from pathlib import Path

LIMIT = 32 * 1024 * 1024


def load(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > LIMIT:
        raise ValueError("facts must be bounded regular files")
    return json.loads(path.read_bytes())


def date(value):
    return dt.date.fromisoformat(value)


def trading_days(calendar, start, end):
    lower, upper = date(calendar["start"]), date(calendar["end"])
    if lower > start or upper < end:
        raise ValueError("calendar does not cover requested range")
    holidays = [date(value) for value in calendar["holidays"]]
    if len(holidays) != len(set(holidays)) or any(not lower <= d <= upper for d in holidays):
        raise ValueError("duplicate/out-of-range holiday")
    holidays = set(holidays)
    result = []
    while start <= end:
        if start.weekday() < 5 and start not in holidays:
            result.append(start)
        start += dt.timedelta(days=1)
    return result


def audit(job, reference_path=None):
    calendar = load(Path(job["calendar"]))
    history = load(Path(job["security_history"]))
    universe = load(Path(job["universe"]))
    start, end, planned = map(date, (job["from"], job["through"], job["planned_date"]))
    if start > end or (end - start).days > 3660:
        raise ValueError("invalid point-in-time range/cutoff")
    if date(job["source_as_of"]) < end or date(job["source_as_of"]) > planned:
        raise ValueError("source cutoff")
    days = trading_days(calendar, start, end)
    if reference_path:
        reference = load(reference_path)
        if calendar["source"] == reference["source"] or (calendar["start"], calendar["end"]) != (reference["start"], reference["end"]):
            raise ValueError("calendar reference must have a separate source and identical coverage")
        if trading_days(calendar, date(calendar["start"]), date(calendar["end"])) != trading_days(reference, date(reference["start"]), date(reference["end"])):
            raise ValueError("independent calendar dates disagree")
    digests = {job["calendar_source_sha256"], universe["source_sha256"]}
    intervals = {}
    for row in history["intervals"]:
        code = row["code"]
        announcement = row["announcement"]
        if row["state"] not in {"Trading", "Suspended", "Delisted"} or date(row["start"]) > date(row["end"]):
            raise ValueError("invalid security state/interval")
        if date(row["end"]) > date(history["checked_through"]) or date(announcement["checked"]) > date(history["checked_through"]):
            raise ValueError("interval exceeds verification cutoff")
        if not announcement["url"].startswith("https://") or date(announcement["published"]) > date(announcement["checked"]):
            raise ValueError("invalid announcement provenance")
        digests.add(announcement["sha256"])
        intervals.setdefault(code, []).append(row)
    for rows in intervals.values():
        rows.sort(key=lambda row: row["start"])
        if any(a["end"] >= b["start"] for a, b in zip(rows, rows[1:])):
            raise ValueError("overlapping security intervals")
    for dataset in (history, universe):
        if not dataset["version"] or not end <= date(dataset["checked_through"]) <= planned:
            raise ValueError("dataset verification cutoff does not cover plan")
    evidence = []
    for digest in sorted(digests):
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise ValueError("invalid SHA-256")
        path = Path(job["facts_directory"]) / digest
        if path.is_symlink() or not path.is_file() or path.stat().st_size > job["max_source_bytes"]:
            raise ValueError("announcement file missing or over budget")
        hasher = hashlib.sha256()
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(65536), b""):
                hasher.update(chunk)
        if hasher.hexdigest() != digest:
            raise ValueError("announcement original bytes changed")
        evidence.append({"sha256": digest, "bytes": path.stat().st_size})
    count, unknown, states, previous = 0, [], {}, None
    for member in universe["members"]:
        code = member["code"]
        if previous is not None and previous >= code:
            raise ValueError("universe must be sorted and unique")
        if len(code) != 8 or code[:2] not in {"sh", "sz", "bj"} or not code[2:].isdigit():
            raise ValueError("full security identity required")
        if date(member["from"]) > date(member["through"]) or date(member["through"]) > date(universe["checked_through"]):
            raise ValueError("universe interval exceeds verification cutoff")
        previous = code
        rows = intervals.get(code, [])
        starts = [row["start"] for row in rows]
        for day in days:
            if not date(member["from"]) <= day <= date(member["through"]):
                continue
            count += 1
            if count > job["max_rows"]:
                raise ValueError("expected row budget exceeded")
            index = bisect.bisect_right(starts, day.isoformat()) - 1
            state = rows[index]["state"] if index >= 0 and day.isoformat() <= rows[index]["end"] else "Unknown"
            states[state] = states.get(state, 0) + 1
            if state == "Unknown" and len(unknown) < 1000:
                unknown.append({"date": day.isoformat(), "code": code})
    return {"schema_version": 1, "status": "complete" if count and not states.get("Unknown") else "incomplete",
            "expected_keys": count, "states": states, "unknown_examples": unknown,
            "unknown_examples_truncated": states.get("Unknown", 0) > len(unknown),
            "calendar_version": calendar["version"], "calendar_through": calendar["end"],
            "calendar_reference_checked": reference_path is not None,
            "history_version": history["version"], "universe_version": universe["version"],
            "source_as_of": job["source_as_of"], "evidence": evidence,
            "scope": "Only declared point-in-time universe and range. Integrity/coverage do not prove announcement semantics or a full-market universe. No future coverage is inferred."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--calendar-reference", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("use a new audit output")
    result = audit(load(args.config), args.calendar_reference)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "expected_keys": result["expected_keys"]}))
    if result["status"] != "complete":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
