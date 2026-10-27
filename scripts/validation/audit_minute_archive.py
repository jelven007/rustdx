#!/usr/bin/env python3
"""Audit a bounded, independent minute archive candidate; never publish or infer dates."""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re

LIMIT = 32 * 1024 * 1024
AXES = ("date", "slots", "price", "volume", "amount")
ROW_FIELDS = {"market", "code", "date", "slot", "price_minor", "volume_shares",
              "amount_minor", "cumulative_volume_shares", "cumulative_amount_minor"}


def integer(value, positive=False):
    return type(value) is int and (value > 0 if positive else value >= 0)


def timestamp(value):
    result = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.tzinfo is None:
        raise ValueError("archive timestamp requires an explicit timezone")
    return result


def key(row):
    if (row["market"] not in ("sz", "sh", "bj")
            or not re.fullmatch(r"[0-9]{6}", row["code"])):
        raise ValueError("archive key must preserve market and six digit code")
    date = dt.date.fromisoformat(row["date"])
    return row["market"], row["code"], date.isoformat()


def read(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > LIMIT:
        raise ValueError("archive input must be a bounded regular file")
    with path.open("rb") as stream:
        body = stream.read(LIMIT + 1)
    if len(body) > LIMIT:
        raise ValueError("archive input exceeds the read budget")
    return body


class Evidence:
    def __init__(self, root):
        self.root = Path(root)
        self.verified = {}

    def bytes(self, item):
        name = item["path"]
        if (not isinstance(name, str) or name.startswith("/")
                or any(p in ("", ".", "..") for p in name.split("/"))):
            raise ValueError("unsafe archive evidence path")
        path = self.root
        for part in name.split("/"):
            path = path / part
            if path.is_symlink():
                raise ValueError("archive symlink forbidden")
        body = read(path)
        digest = hashlib.sha256(body).hexdigest()
        if (not integer(item["bytes"]) or len(body) != item["bytes"]
                or item["sha256"] != digest):
            raise ValueError("archive evidence bytes or digest changed")
        if name in self.verified and self.verified[name] != digest:
            raise ValueError("conflicting archive evidence")
        self.verified[name] = digest
        return body

    def qualifies(self, item):
        if item is None:
            return False
        self.bytes(item)
        return (item.get("qualification") == "independently_reviewed"
                and isinstance(item.get("source"), str)
                and item["source"].startswith("https://"))


def audit(manifest, root, now, previous=None):
    if manifest.get("schema_version") != 1 or not manifest.get("archive_id"):
        raise ValueError("unsupported minute archive contract")
    if now.tzinfo is None or not integer(manifest["revision"], positive=True):
        raise ValueError("archive audit time or revision invalid")
    source_as_of = timestamp(manifest["source_as_of"])
    if source_as_of > now:
        raise ValueError("archive source_as_of is in the future")
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise ValueError("archive root must be a regular directory")
    issues = []
    policy = manifest.get("retention", {})
    for field in ("days", "max_bytes", "max_files", "max_freshness_seconds"):
        if not integer(policy.get(field), positive=True):
            issues.append("retention_input_missing:" + field)
    # Invalid production inputs never remove the verifier's own hard bounds.
    max_files = min(policy.get("max_files") or 200000, 200000)
    max_bytes = min(policy.get("max_bytes") or (1 << 40), 1 << 40)
    if not integer(max_files, positive=True) or not integer(max_bytes, positive=True):
        raise ValueError("invalid archive scan budget")
    total_bytes, entries = 0, 0
    stack = [root]
    while stack:
        for path in stack.pop().iterdir():
            entries += 1
            if entries > max_files or path.is_symlink():
                raise ValueError("archive entry budget exceeded or symlink present")
            if path.is_dir():
                stack.append(path)
            elif path.is_file():
                total_bytes += path.stat().st_size
                if total_bytes > max_bytes:
                    raise ValueError("archive byte budget exceeded")
            else:
                raise ValueError("archive special file forbidden")
    if integer(policy.get("max_freshness_seconds"), positive=True):
        if (now - source_as_of).total_seconds() > policy["max_freshness_seconds"]:
            issues.append("archive_freshness_exceeded")
    evidence = Evidence(root)
    expected, partitions, sources = {}, {}, {}
    if not 1 <= len(manifest["expected"]) <= 100000 or len(manifest["partitions"]) > 100000:
        raise ValueError("archive partition declaration budget exceeded")
    for item in manifest["expected"]:
        identity = key(item)
        slots = item["slots"]
        if identity in expected or item["state"] not in ("Trading", "Suspended", "Delisted"):
            raise ValueError("duplicate expected key or unknown trading state")
        if (not isinstance(slots, list) or len(slots) > 10000 or slots != sorted(set(slots))
                or any(not re.fullmatch(r"(?:[01][0-9]|2[0-3]):[0-5][0-9]", s) for s in slots)
                or (item["state"] == "Trading") != bool(slots)):
            raise ValueError("expected slots must be explicit, unique and ordered")
        if not evidence.qualifies(item.get("evidence")):
            issues.append("expected_scope_unverified:" + ":".join(identity))
        expected[identity] = item
    row_count = 0
    for item in manifest["partitions"]:
        identity = key(item)
        if identity in partitions:
            raise ValueError("duplicate archive partition")
        partitions[identity] = item
        scope = expected.get(identity)
        if scope is None or scope["state"] != "Trading":
            issues.append("extra_partition:" + ":".join(identity))
        source = item["source"]
        kind = source["kind"]
        if kind not in ("PassivePush", "RequestedRefresh", "ArchiveReplay") or not source["endpoint"]:
            raise ValueError("archive update source must be explicit")
        captured = timestamp(source["captured_at"])
        if captured > source_as_of:
            raise ValueError("partition capture exceeds archive source_as_of")
        evidence.bytes(source["response"])
        if kind == "RequestedRefresh":
            evidence.bytes(source["request"])
        elif kind == "PassivePush":
            if source.get("request") is not None:
                raise ValueError("requested refresh cannot be classified as passive push")
            if not evidence.qualifies(source.get("origin_evidence")):
                issues.append("passive_origin_unverified:" + ":".join(identity))
        elif not evidence.qualifies(source.get("origin_evidence")):
            issues.append("replay_origin_unverified:" + ":".join(identity))
        sources[kind] = sources.get(kind, 0) + 1
        for axis in AXES:
            if not evidence.qualifies(item["qualification"].get(axis)):
                issues.append("unqualified_" + axis + ":" + ":".join(identity))
        body = evidence.bytes(item["rows"])
        slots, volume, amount = [], 0, 0
        for line in body.splitlines():
            if not line or len(line) > 16384 or len(slots) >= 10000:
                raise ValueError("archive row budget or blank row invalid")
            row = json.loads(line)
            if set(row) != ROW_FIELDS or key(row) != identity:
                raise ValueError("archive row fields or partition key differs")
            if any(not integer(row[field]) for field in ROW_FIELDS - {"market", "code", "date", "slot"}):
                raise ValueError("archive numeric fields require nonnegative integers")
            slots.append(row["slot"])
            volume += row["volume_shares"]
            amount += row["amount_minor"]
            if row["cumulative_volume_shares"] != volume or row["cumulative_amount_minor"] != amount:
                issues.append("cumulative_closure_differs:" + ":".join(identity))
        row_count += len(slots)
        if scope and slots != scope["slots"]:
            issues.append("missing_or_extra_slots:" + ":".join(identity))
        control = item["day_control"]
        if not evidence.qualifies(control.get("evidence")):
            issues.append("day_control_unverified:" + ":".join(identity))
        if (not integer(control["volume_shares"]) or not integer(control["amount_minor"])
                or volume != control["volume_shares"] or amount != control["amount_minor"]):
            issues.append("day_closure_differs:" + ":".join(identity))
    for identity, item in expected.items():
        if item["state"] == "Trading" and identity not in partitions:
            issues.append("missing_partition:" + ":".join(identity))
    if previous is not None:
        if (previous["schema_version"] != 1 or previous["archive_id"] != manifest["archive_id"]
                or previous["revision"] > manifest["revision"]
                or timestamp(previous["source_as_of"]) > source_as_of
                or (previous["revision"] == manifest["revision"] and previous != manifest)):
            raise ValueError("archive revision rollback or conflicting same revision")
        if integer(policy.get("days"), positive=True):
            cutoff = now.astimezone(dt.timezone(dt.timedelta(hours=8))).date() - dt.timedelta(
                days=policy["days"])
            for item in previous["partitions"]:
                identity = key(item)
                if dt.date.fromisoformat(identity[2]) >= cutoff and identity not in partitions:
                    issues.append("retained_partition_lost:" + ":".join(identity))
                if identity in partitions and timestamp(
                        partitions[identity]["source"]["captured_at"]) < timestamp(
                            item["source"]["captured_at"]):
                    raise ValueError("partition acquisition time rollback")
    replay = hashlib.sha256()
    for identity, item in sorted(partitions.items()):
        replay.update(json.dumps(identity, separators=(",", ":")).encode())
        replay.update(bytes.fromhex(item["rows"]["sha256"]))
    return {"schema_version": 1, "mode": "audit_only",
            "archive_id": manifest["archive_id"], "revision": manifest["revision"],
            "as_of": now.isoformat(), "status": "incomplete" if issues else "covered",
            "issues": sorted(set(issues)), "rows": row_count, "partitions": len(partitions),
            "sources": sources, "passive_push_verified": False,
            "logical_bytes": total_bytes, "entries": entries,
            "evidence_sha256": evidence.verified, "replay_sha256": replay.hexdigest(),
            "scope": "Pinned bytes and declared independent qualifications only; external truth, provider passive capability and production retention remain separately reviewed."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--as-of", type=timestamp, default=dt.datetime.now(dt.timezone.utc))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    archive_root = args.root.resolve()
    output = args.output.resolve()
    if output == archive_root or archive_root in output.parents:
        parser.error("audit output must be outside the archive")
    result = audit(json.loads(read(args.manifest)), args.root, args.as_of,
                   json.loads(read(args.previous)) if args.previous else None)
    with args.output.open("x") as stream:
        stream.write(json.dumps(result, indent=2, ensure_ascii=False, allow_nan=False) + "\n")
    print(json.dumps({"status": result["status"], "rows": result["rows"]}))
    raise SystemExit(0 if result["status"] == "covered" else 1)


if __name__ == "__main__":
    main()
