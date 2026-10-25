"""Versioned engineering identity; historical evidence has a separate manifest."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

SCOPE_VERSION = 1
EXCLUDED_PREFIXES = ("docs/testing/evidence/", "project-review-20261022/")


def engineering_path(name):
    return not name.startswith(EXCLUDED_PREFIXES)


def digest_file(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def fingerprint(records):
    return hashlib.sha256(json.dumps(records, ensure_ascii=True, sort_keys=True,
                                     separators=(",", ":")).encode()).hexdigest()


def git_paths(root):
    paths = set(subprocess.check_output(["git", "-C", str(root), "ls-files", "-z"]).split(b"\0"))
    paths.update(subprocess.check_output(
        ["git", "-C", str(root), "ls-files", "--others", "--exclude-standard", "-z"]).split(b"\0"))
    return sorted(os.fsdecode(raw) for raw in paths if raw)


def inventory(root):
    root = Path(root).resolve()
    records = []
    for name in git_paths(root):
        if not engineering_path(name):
            continue
        path = root / name
        if path.is_symlink():
            data = os.fsencode(os.readlink(path))
            kind, size, digest = "symlink", len(data), hashlib.sha256(data).hexdigest()
        elif path.is_file():
            kind, size, digest = "file", path.stat().st_size, digest_file(path)
        elif not path.exists():
            continue  # The preflight also binds the complete Git status.
        else:
            raise ValueError(f"unsupported engineering entry: {name}")
        records.append({"path": name, "kind": kind, "bytes": size, "sha256": digest,
                        "executable": bool(path.lstat().st_mode & 0o111)})
    return records, fingerprint(records)
