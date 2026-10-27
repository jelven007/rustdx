#!/usr/bin/env python3
"""Inventory, verify or bundle retained evidence without rewriting historical bytes."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile

from source_scope import digest_file, fingerprint, git_paths
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "validation"))
from evidence_paths import relocated_path

ROOT = Path(__file__).resolve().parents[2]
PREFIXES = ("docs/testing/evidence/", "project-review-20261022/")
LOCAL = "docs/testing/evidence/optimization-source-delivery-20261025/local-binaries.json"


def safe_path(root, name):
    relative = PurePosixPath(name)
    if relative.is_absolute() or ".." in relative.parts or not relative.parts:
        raise ValueError(f"unsafe evidence path: {name}")
    path = relocated_path(root, str(relative))
    for parent in [path, *path.parents]:
        if parent == root:
            break
        if parent.is_symlink():
            raise ValueError(f"evidence symlink: {name}")
    path.resolve().relative_to(root.resolve())
    return path


def validate_manifest(manifest):
    if manifest.get("schema_version") != 1:
        raise ValueError("unsupported evidence manifest")
    records = manifest["files"]
    if not isinstance(records, list) or len(records) > 200000:
        raise ValueError("evidence entry budget")
    if len({row["path"] for row in records}) != len(records):
        raise ValueError("duplicate evidence path")
    for row in records:
        name = row["path"]
        if (not isinstance(name, str) or PurePosixPath(name).is_absolute()
                or any(part in ("", ".", "..") for part in name.split("/"))
                or type(row["bytes"]) is not int or row["bytes"] < 0
                or not isinstance(row["sha256"], str) or len(row["sha256"]) != 64
                or any(c not in "0123456789abcdef" for c in row["sha256"])):
            raise ValueError("invalid evidence record")
    if fingerprint(records) != manifest["files_sha256"]:
        raise ValueError("manifest identity differs")
    return records


def verify(root, manifest):
    issues = []
    for row in validate_manifest(manifest):
        path = safe_path(root, row["path"])
        if not path.is_file():
            issues.append({"path": row["path"], "reason": "missing",
                           "availability": row["availability"]})
        elif path.stat().st_size != row["bytes"] or digest_file(path) != row["sha256"]:
            issues.append({"path": row["path"], "reason": "changed"})
    return {"status": "passed" if not issues else "incomplete", "issues": issues,
            "checked_entries": len(manifest["files"]),
            "scope": "Byte identity only; semantic qualification and source authenticity are separate."}


def inventory(root, include_local=False):
    head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"]).decode().strip()
    tree = subprocess.check_output(["git", "-C", str(root), "ls-tree", "-r", "-z", head])
    frozen = {}
    for entry in tree.split(b"\0"):
        if entry:
            header, name = entry.split(b"\t", 1)
            mode, kind, oid = header.decode().split()
            frozen[name.decode()] = (mode, kind, oid)
    names = {name: "ordinary_git" for name in git_paths(root) if name.startswith(PREFIXES)}
    local = json.loads((root / LOCAL).read_text())["files"] if (root / LOCAL).is_file() else []
    expected_local = {row["path"]: row for row in local}
    if include_local:
        names.update({name: "local_retained" for name in expected_local})
    migrated_frozen = {}
    for old, identity in frozen.items():
        if old.startswith("project-review-20261022/"):
            moved = relocated_path(root, old).relative_to(root).as_posix()
            migrated_frozen[moved] = identity
    records = []
    for name, availability in sorted(names.items()):
        path = safe_path(root, name)
        if not path.is_file():
            raise ValueError(f"evidence missing: {name}")
        size = path.stat().st_size
        sha = hashlib.sha256()
        blob = hashlib.sha1(f"blob {size}\0".encode())
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                sha.update(block)
                blob.update(block)
        identity = frozen.get(name, migrated_frozen.get(name))
        if identity:
            mode, kind, oid = identity
            actual_mode = "100755" if path.stat().st_mode & 0o111 else "100644"
            if kind != "blob" or mode != actual_mode or oid != blob.hexdigest():
                raise ValueError(f"frozen HEAD evidence changed: {name}")
        row = {"path": name, "bytes": size, "sha256": sha.hexdigest(),
               "availability": availability}
        if name in expected_local and any(row[key] != expected_local[name][key] for key in ["bytes", "sha256"]):
            raise ValueError(f"retained collector differs: {name}")
        records.append(row)
    return {"schema_version": 1, "head": head, "files": records,
            "files_sha256": fingerprint(records), "frozen_head_bytes": "verified",
            "local_binaries_included": include_local, "local_binaries_declared": len(local),
            "local_binaries_reference": LOCAL, "remote_object_store": "not configured",
            "retention": "Existing evidence bytes retained; recorded path migrations resolved separately."}


def bundle(root, manifest, destination, prefix):
    records = [row for row in validate_manifest(manifest) if row["path"].startswith(prefix)]
    if not records:
        raise ValueError("bundle scope is empty")
    subset = dict(manifest, files=records, files_sha256=fingerprint(records))
    checked = verify(root, subset)
    if checked["status"] != "passed":
        raise ValueError(f"bundle inputs incomplete: {checked['issues'][:5]}")
    # The result manifest binds the gzip file as well as every original entry.
    with destination.open("xb") as stream, gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode="w|") as archive:
            for row in records:
                path = safe_path(root, row["path"])
                info = tarfile.TarInfo(row["path"])
                info.mode = 0o644
                info.size = row["bytes"]
                with path.open("rb") as source:
                    archive.addfile(info, source)
    # A concurrent input change invalidates distribution rather than refreshing its digest.
    if verify(root, subset)["status"] != "passed":
        raise ValueError("evidence changed while bundling")
    return dict(subset, archive={"bytes": destination.stat().st_size,
                               "sha256": digest_file(destination), "prefix": prefix})


def restore(root, manifest, archive):
    """Retrieve a pinned local bundle into a new root; never overwrite retained bytes."""
    records = validate_manifest(manifest)
    identity = manifest["archive"]
    if root.exists() or root.is_symlink() or not root.parent.is_dir():
        raise ValueError("restore requires a new root with an existing parent")
    if (archive.is_symlink() or not archive.is_file()
            or archive.stat().st_size != identity["bytes"]
            or digest_file(archive) != identity["sha256"]):
        raise ValueError("archive identity differs or archive is unavailable")
    expected = {row["path"]: row for row in records}
    seen = set()
    with tempfile.TemporaryDirectory(prefix=".evidence-restore-", dir=root.parent) as temporary:
        staging = Path(temporary) / "payload"
        staging.mkdir()
        with tarfile.open(archive, mode="r|gz") as stream:
            for member in stream:
                row = expected.get(member.name)
                if (row is None or member.name in seen or not member.isfile()
                        or member.size != row["bytes"] or member.issparse()):
                    raise ValueError("unexpected, duplicate or invalid archive entry")
                path = safe_path(staging, member.name)
                path.parent.mkdir(parents=True, exist_ok=True)
                digest, size = hashlib.sha256(), 0
                with stream.extractfile(member) as source, path.open("xb") as destination:
                    for block in iter(lambda: source.read(1024 * 1024), b""):
                        size += len(block)
                        if size > row["bytes"]:
                            raise ValueError("archive entry exceeds declared bytes")
                        digest.update(block)
                        destination.write(block)
                if size != row["bytes"] or digest.hexdigest() != row["sha256"]:
                    raise ValueError("archive entry identity differs")
                seen.add(member.name)
        if seen != expected.keys() or verify(staging, manifest)["status"] != "passed":
            raise ValueError("archive is incomplete")
        if digest_file(archive) != identity["sha256"]:
            raise ValueError("archive changed during restore")
        # Original archived names are retained, including names in old manifests.
        # Do not merge into an existing checkout or apply its path migrations.
        if root.exists() or root.is_symlink():
            raise ValueError("restore root appeared during retrieval")
        staging.rename(root)
    return dict(status="passed", checked_entries=len(records),
                archive_sha256=identity["sha256"], files_sha256=manifest["files_sha256"],
                scope="Pinned archive and original file bytes; semantic qualification remains separate.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["inventory", "verify", "bundle", "restore"])
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--include-local-binaries", action="store_true")
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--prefix", default="docs/testing/evidence/")
    args = parser.parse_args()
    if args.output.exists() or args.command == "bundle" and args.archive and args.archive.exists():
        parser.error("use new outputs; never overwrite frozen material")
    root = args.root.resolve()
    if args.command == "inventory":
        result = inventory(root, args.include_local_binaries)
    else:
        if not args.manifest:
            parser.error("--manifest is required")
        manifest = json.loads(args.manifest.read_text())
        if args.command == "verify":
            result = verify(root, manifest)
        else:
            if not args.archive:
                parser.error("--archive is required")
            result = (restore(root, manifest, args.archive) if args.command == "restore"
                      else bundle(root, manifest, args.archive, args.prefix))
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ["status", "checked_entries"] if key in result}))
    return 0 if result.get("status", "passed") == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
