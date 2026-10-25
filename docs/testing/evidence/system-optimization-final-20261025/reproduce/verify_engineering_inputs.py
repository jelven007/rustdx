import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path(__file__).parent
sys.path.insert(0, str(ROOT / "scripts/release"))
import preflight

records, current = preflight.inventory()
baseline = json.loads((OUT / "engineering-fixed-source.json").read_text())
old = {row["path"]: row for row in baseline["files"]}
new = {row["path"]: row for row in records}


def rust_input(name):
    return name.startswith(("crates/", "apps/", "tests/", "tools/")) or name in ["Cargo.toml", "Cargo.lock", "rustfmt.toml"]


if {name: row for name, row in old.items() if rust_input(name)} != {name: row for name, row in new.items() if rust_input(name)}:
    raise RuntimeError("Rust inputs changed; full isolated Rust regression must be repeated")

isolated = OUT / "isolated-engineering-fixed"
for row in records:
    if row["kind"] != "file":
        raise RuntimeError("this isolated verification requires ordinary files")
    path = isolated / row["path"]
    source = ROOT / row["path"]
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != row["sha256"]:
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, path)
    if hashlib.sha256(path.read_bytes()).hexdigest() != row["sha256"]:
        raise RuntimeError("isolated source identity differs")
for name in old.keys() - new.keys():
    (isolated / name).unlink()

required = ["config/acceptance/task-matrix.json", "config/acceptance/deployment.example.json",
            "deploy/acceptance/tool-archives.sha256"]
if not set(required).issubset(new) or (isolated / "docs/testing/evidence").exists():
    raise RuntimeError("engineering scope is incomplete")
archive = OUT / "engineering-complete.tar.gz"
preflight.source_archive(records, archive)
(OUT / "engineering-complete-source.json").write_text(json.dumps(
    {"schema_version": 1, "source_scope_version": 1, "files": records, "source_sha256": current,
     "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(), "bytes": sum(row["bytes"] for row in records),
     "archive_bytes": archive.stat().st_size, "dirty_review": True}, indent=2) + "\n")
results = []
for label, directory in [("root", ROOT), ("isolated", isolated)]:
    command = ["python3", "-m", "unittest", "discover", "-s", "scripts/validation/tests", "-v"]
    start = time.monotonic()
    log = OUT / f"complete-python-{label}.log"
    with log.open("wb") as stream:
        result = subprocess.run(command, cwd=directory, env=os.environ, stdout=stream, stderr=subprocess.STDOUT)
    results.append({"command": command, "directory": str(directory), "exit_code": result.returncode,
                    "seconds": time.monotonic() - start, "log": log.name})
    if result.returncode:
        raise RuntimeError("corrected Python inputs do not pass")
(OUT / "engineering-complete-verification.json").write_text(json.dumps(
    {"status": "passed", "source_sha256": current, "historical_evidence_present": False,
     "required_inputs": required, "rust_inputs_identical_to_full_checked_archive": True,
     "rust_full_log": "isolated-fixed-0.log", "rust_full_snapshot": baseline["source_sha256"],
     "results": results}, indent=2) + "\n")
print("complete engineering inputs and both Python regressions verified")
