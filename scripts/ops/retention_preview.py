#!/usr/bin/env python3
"""Preview retention dependencies and capacity. Never deletes or prunes ledger rows."""
import argparse
from collections import defaultdict
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import sqlite3
import stat
import tempfile
import shutil

SCHEMA = 1
JSON_LIMIT = 32 * 1024 * 1024


def regular(path, directory=False):
    mode = path.lstat().st_mode
    if not (stat.S_ISDIR(mode) if directory else stat.S_ISREG(mode)):
        raise ValueError(f"requires regular {'directory' if directory else 'file'}: {path}")


def safe(root, relative):
    if not isinstance(relative, str) or not relative or any(p in ("", ".", "..") for p in relative.split("/")):
        raise ValueError("unsafe dependency path")
    path = Path(relative)
    if path.is_absolute() or not path.parts or any(p in (".", "..") for p in path.parts):
        raise ValueError("unsafe dependency path")
    current = root
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            raise ValueError("dependency symlink rejected")
    return current


def read_json(path):
    regular(path)
    with path.open("rb") as stream:
        data = stream.read(JSON_LIMIT + 1)
    if len(data) > JSON_LIMIT:
        raise ValueError("JSON budget exceeded")
    return json.loads(data)


class Scan:
    def __init__(self, max_files, max_bytes):
        self.max_files, self.max_bytes = max_files, max_bytes
        self.files = self.bytes = 0

    def tree(self, root):
        regular(root, directory=True)
        stack, files = [root], {}
        while stack:
            directory = stack.pop()
            for path in sorted(directory.iterdir()):
                meta = path.lstat()
                self.files += 1
                if self.files > self.max_files:
                    raise ValueError("filesystem entry budget exceeded")
                if stat.S_ISDIR(meta.st_mode):
                    stack.append(path)
                elif stat.S_ISREG(meta.st_mode):
                    self.bytes += meta.st_size
                    if self.bytes > self.max_bytes:
                        raise ValueError("scan byte budget exceeded")
                    files[path.relative_to(root).as_posix()] = {
                        "bytes": meta.st_size, "allocated_bytes": meta.st_blocks * 512,
                        "modified_ns": meta.st_mtime_ns,
                    }
                else:
                    raise ValueError(f"symlink or special file rejected: {path}")
        return files


def verify(root, evidence):
    path = safe(root, evidence["path"])
    regular(path)
    if path.stat().st_size != evidence["bytes"]:
        raise ValueError(f"evidence size differs: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            digest.update(block)
    if digest.hexdigest() != evidence["sha256"]:
        raise ValueError(f"evidence hash differs: {path}")
    return path


@contextmanager
def locked_ledger(root):
    regular(root, directory=True)
    regular(root / "controller.lock")
    regular(root / "ledger.sqlite")
    # Rust File::try_lock uses an exclusive flock on Unix. Reuse its existing file.
    with (root / "controller.lock").open("rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_SH | fcntl.LOCK_NB)
        # Opening WAL-mode SQLite can create shm even for mode=ro. Copy db AND
        # committed WAL while the controller is excluded; never use immutable=1
        # on the source, which can omit its committed WAL state.
        with tempfile.TemporaryDirectory(prefix="rustdx-retention-ledger-") as temporary:
            snapshot = Path(temporary) / "ledger.sqlite"
            identities = {}
            for suffix in ("", "-wal"):
                source = root / ("ledger.sqlite" + suffix)
                if source.exists():
                    regular(source)
                    before = source.stat()
                    if before.st_size > JSON_LIMIT * 16:
                        raise ValueError("ledger snapshot byte budget exceeded")
                    shutil.copyfile(source, Path(str(snapshot) + suffix))
                    identities[source] = (before.st_size, before.st_mtime_ns)
            for source, identity in identities.items():
                after = source.stat()
                if identity != (after.st_size, after.st_mtime_ns):
                    raise ValueError("ledger changed while snapshotting")
            db = sqlite3.connect(snapshot)
            try:
                db.execute("PRAGMA query_only=ON")
                db.execute("BEGIN")
                version = db.execute("PRAGMA user_version").fetchone()[0]
                if version not in (1, 2, 3, 4):
                    raise ValueError("unsupported ledger schema")
                active = db.execute(
                    "SELECT count(*) FROM attempts WHERE state IN "
                    "('running','evaluated','reconciliation_required') OR pid IS NOT NULL"
                ).fetchone()[0]
                if active:
                    raise ValueError("recover active or ambiguous workers before retention preview")
                yield db, version
            finally:
                db.close()


def preview(root, backups=(), retention_days=30, max_files=200000, max_bytes=1 << 40, previous=None):
    regular(Path(root), directory=True)
    root = Path(root).resolve(strict=True)
    for path in backups:
        regular(Path(path), directory=True)
    backups = sorted(Path(p).resolve(strict=True) for p in backups)
    if len(set(backups)) != len(backups) or any(p == root or root in p.parents or p in root.parents for p in backups):
        raise ValueError("backups must be unique and outside the live root")
    if not math.isfinite(retention_days) or retention_days < 0 or max_files <= 0 or max_bytes <= 0:
        raise ValueError("invalid preview budget or retention age")
    scan = Scan(max_files, max_bytes)
    dependencies = defaultdict(set)
    now = datetime.now(timezone.utc)
    backup_reports = []
    relocations = []

    def protect(path, reason):
        safe(root, path)
        dependencies[path].add(reason)

    def proof(evidence, release, target=None):
        path = verify(root, evidence)
        data = read_json(path)
        if data.get("schema_version") != 1 or data.get("release") != release or (
                target is not None and data.get("target") != target):
            raise ValueError("database proof identity differs")
        if not isinstance(data.get("io"), list):
            raise ValueError("database proof I/O list missing")
        protect(evidence["path"], "database_verification_proof")
        for pair in data["io"]:
            if not isinstance(pair, list) or len(pair) != 2:
                raise ValueError("invalid database proof I/O pair")
            for text in pair:
                if text is not None:
                    item = json.loads(text)
                    verify(root, item)
                    protect(item["path"], "database_proof_io")

    with locked_ledger(root) as (db, version):
        files = scan.tree(root)
        restore_path = root / "restore-validated.json"
        restore = read_json(restore_path) if restore_path.exists() else None
        if restore is not None and restore.get("schema_version") != 1:
            raise ValueError("unsupported restore marker")
        original_roots = restore.get("original_roots") if restore else None
        legacy_original = set()
        for identifier, directory, state in db.execute("SELECT id,directory,state FROM attempts"):
            relative = "attempts/" + identifier
            safe(root, relative)
            original = Path(directory)
            if (not original.is_absolute() or original.parts[-2:] != ("attempts", identifier)
                    or ".." in original.parts):
                raise ValueError("attempt directory must be an absolute attempts/id path")
            if original.resolve() != root / relative:
                origin = str(original.parent.parent)
                if restore is None or (original_roots is not None and origin not in original_roots):
                    raise ValueError("attempt relocation lacks a matching restore marker")
                legacy_original.add(origin)
                relocations.append({"original": directory, "logical": relative})
            protect(relative, "ledger_attempt:" + state)
        if original_roots is None and len(legacy_original) > 1:
            raise ValueError("legacy restore contains ambiguous original roots")
        for identifier, text, state in db.execute("SELECT id,manifest,state FROM releases"):
            evidence = json.loads(text)
            manifest_path = verify(root, evidence)
            if manifest_path.parent != safe(root, "releases/" + identifier):
                raise ValueError("release manifest identity differs")
            manifest = read_json(manifest_path)
            if manifest["release_id"] != identifier:
                raise ValueError("release identity differs")
            for item in manifest["files"]:
                verify(manifest_path.parent, item)
            protect("releases/" + identifier, "ledger_release:" + state)
        for dataset, version_number, text in db.execute("SELECT dataset,version,pointer FROM publications"):
            pointer = json.loads(text)
            catalog_path = verify(root, pointer["manifest"])
            catalog = read_json(catalog_path)
            if catalog["dataset_id"] != dataset or catalog["version"] != version_number:
                raise ValueError("catalog identity differs from publication")
            protect(pointer["manifest"]["path"], f"catalog:{dataset}:{version_number}")
            for partition in catalog["partitions"].values():
                protect("releases/" + partition["release_id"], f"catalog:{dataset}:{version_number}")
            for release, receipts in catalog.get("database_releases", {}).items():
                protect("releases/" + release, f"catalog_database:{dataset}:{version_number}")
                for receipt in receipts:
                    proof(receipt["proof"], release, receipt["target"])
        if version >= 2:
            for release, identifier, spec, state, text in db.execute("SELECT release,id,spec,state,proof FROM targets"):
                protect("releases/" + release, "database_target:" + identifier)
                if state == "verified" and text is None:
                    raise ValueError("verified database target has no proof")
                if text is not None:
                    proof(json.loads(text), release, json.loads(spec))
            for release, identifier, request, response in db.execute("SELECT release,target,request,response FROM target_io"):
                for text in (request, response):
                    if text is not None:
                        evidence = json.loads(text)
                        verify(root, evidence)
                        protect(evidence["path"], f"database_receipt:{release}:{identifier}")
        counts = {table: db.execute(f"SELECT count(*) FROM {table}").fetchone()[0]
                  for table in ("jobs", "attempts", "releases", "publications", "events")}
        for backup in backups:
            backup_files = scan.tree(backup)
            inventory = read_json(backup / "backup.json")
            if inventory["schema_version"] != 1 or len(inventory["files"]) > max_files:
                raise ValueError("invalid backup inventory")
            names = set()
            for evidence in inventory["files"]:
                if evidence["path"] in names:
                    raise ValueError("duplicate backup dependency")
                names.add(evidence["path"])
                verify(backup, evidence)
                protect(evidence["path"], "backup:" + str(backup))
            if "ledger.sqlite" not in names or not names <= backup_files.keys():
                raise ValueError("incomplete backup inventory")
            backup_reports.append({
                "path": str(backup), "created_at": inventory["created_at"],
                "logical_bytes": sum(f["bytes"] for f in backup_files.values()),
                "allocated_bytes": sum(f["allocated_bytes"] for f in backup_files.values()),
                "files": len(backup_files),
            })

        objects = defaultdict(list)
        for path, meta in files.items():
            parts = Path(path).parts
            if len(parts) >= 2 and parts[0] in ("attempts", "releases", "database"):
                key = "/".join(parts[:2])
            elif len(parts) >= 2 and parts[0] == "catalogs":
                key = path
            else:
                key = parts[0]
            objects[key].append((path, meta))
        object_reasons = defaultdict(set)
        for dependency, reasons in dependencies.items():
            for ancestor in (PurePosixPath(dependency), *PurePosixPath(dependency).parents):
                key = ancestor.as_posix()
                if key in objects:
                    object_reasons[key].update(reasons)
            # A directory reference protects every object contained within it.
        for key in objects:
            for ancestor in (PurePosixPath(key), *PurePosixPath(key).parents):
                object_reasons[key].update(dependencies.get(ancestor.as_posix(), ()))
        entries, buckets = [], defaultdict(lambda: {"logical_bytes": 0, "allocated_bytes": 0, "files": 0})
        for key, members in sorted(objects.items()):
            reasons = object_reasons[key]
            bucket = key.split("/")[0]
            if bucket not in ("attempts", "releases", "catalogs", "database"):
                reasons.add("controller_or_unclassified_data")
            age = max(0, (now.timestamp() * 1e9 - max(m["modified_ns"] for _, m in members)) / 86400e9)
            status = "protected" if reasons else ("candidate_review" if age >= retention_days else "within_retention")
            item = {
                "path": key, "status": status, "reasons": sorted(reasons), "age_days": age,
                "logical_bytes": sum(m["bytes"] for _, m in members),
                "allocated_bytes": sum(m["allocated_bytes"] for _, m in members), "files": len(members),
            }
            entries.append(item)
            for field in ("logical_bytes", "allocated_bytes", "files"):
                buckets[bucket][field] += item[field]
        logical = sum(m["bytes"] for m in files.values())
        allocated = sum(m["allocated_bytes"] for m in files.values())
        space = __import__("os").statvfs(root)
        result = {
            "schema_version": SCHEMA, "mode": "preview_only", "created_at": now.isoformat(),
            "root": str(root), "ledger_schema": version, "ledger_counts": counts,
            "policy": {"retention_days": retention_days, "all_registered_versions_and_evidence": "protected"},
            "scope": "live root plus explicitly supplied backups; undisclosed external copies are unknown",
            "logical_bytes": logical, "allocated_bytes": allocated,
            "free_bytes": space.f_bavail * space.f_frsize,
            "buckets": dict(buckets), "backups": backup_reports, "objects": entries,
            "relocations": relocations,
            "candidate_bytes": sum(e["logical_bytes"] for e in entries if e["status"] == "candidate_review"),
            "trend": None,
        }
    if previous is not None:
        if previous["schema_version"] != SCHEMA or previous["root"] != str(root) or previous["mode"] != "preview_only":
            raise ValueError("previous preview scope differs")
        days = (now - datetime.fromisoformat(previous["created_at"])).total_seconds() / 86400
        if days <= 0:
            raise ValueError("previous sample must precede the current sample")
        growth = (logical - previous["logical_bytes"]) / days
        allocated_growth = (allocated - previous["allocated_bytes"]) / days
        result["trend"] = {
            "elapsed_days": days, "logical_bytes_per_day": growth, "allocated_bytes_per_day": allocated_growth,
            "projected_logical_bytes_30_days": max(0, logical + 30 * growth),
            "days_until_free_exhausted": result["free_bytes"] / allocated_growth if allocated_growth > 0 else None,
            "assumption": "linear change between two samples; no workload or production SLO claim",
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--backup", type=Path, action="append", default=[])
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--retention-days", type=float, default=30)
    parser.add_argument("--max-files", type=int, default=200000)
    parser.add_argument("--max-bytes", type=int, default=1 << 40)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = preview(args.root, args.backup, args.retention_days, args.max_files, args.max_bytes,
                     read_json(args.previous) if args.previous else None)
    text = json.dumps(result, indent=2, allow_nan=False) + "\n"
    if args.output:
        target = args.output.absolute()
        if target == args.root.absolute() or args.root.absolute() in target.parents:
            parser.error("preview output must be outside the controller root")
        for backup in args.backup:
            if target == backup.absolute() or backup.absolute() in target.parents:
                parser.error("preview output must be outside backups")
        with target.open("x") as stream:
            stream.write(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
