import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path(__file__).parent
sys.path.insert(0, str(ROOT / "scripts/release"))
import preflight
from source_scope import inventory

started = time.monotonic()
records, identity = inventory(ROOT)
archive = OUT / "engineering.tar.gz"
preflight.source_archive(records, archive)
(OUT / "engineering-source.json").write_text(json.dumps(
    {"schema_version": 1, "source_scope_version": 1, "files": records,
     "source_sha256": identity, "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
     "bytes": sum(row["bytes"] for row in records), "archive_bytes": archive.stat().st_size,
     "seconds": time.monotonic() - started, "dirty_review": True}, indent=2) + "\n")

controls = []
commands = [
    [sys.executable, "scripts/validation/audit_task_admission.py",
     "--request", "config/acceptance/deployment.example.json", "--evidence-root", str(ROOT),
     "--output", str(OUT / "template-admission.json")],
]
(OUT / "dirty-preflight.json").write_text(json.dumps(
    {"schema_version": 2, "source_scope_version": 1,
     "head": preflight.git("rev-parse", "HEAD").decode().strip(),
     "source_sha256": identity, "dirty": True, "local_gate_passed": False, "result": "passed"}) + "\n")
commands.append([sys.executable, "scripts/release/release_gate.py",
                 "--preflight", str(OUT / "dirty-preflight.json"),
                 "--output", str(OUT / "dirty-release-manifest.json")])
for i, command in enumerate(commands):
    result = subprocess.run(command, cwd=ROOT, capture_output=True)
    (OUT / f"stage3-denial-{i}.log").write_bytes(result.stdout + result.stderr)
    controls.append({"command": command, "exit_code": result.returncode, "expected_exit_code": 1,
                     "log": f"stage3-denial-{i}.log"})
    if result.returncode != 1:
        raise RuntimeError("required denial not observed")
(OUT / "stage3-denials.json").write_text(json.dumps(controls, indent=2) + "\n")
print(json.dumps({"engineering_files": len(records), "engineering_bytes": sum(row["bytes"] for row in records),
                  "archive_bytes": archive.stat().st_size, "denials": len(controls)}))
