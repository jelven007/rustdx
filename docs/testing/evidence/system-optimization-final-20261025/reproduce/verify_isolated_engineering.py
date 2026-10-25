import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import time

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path(__file__).parent
suffix = "-" + sys.argv[1] if len(sys.argv) > 1 else ""
archive = OUT / f"engineering{suffix}.tar.gz"
identity = json.loads((OUT / f"engineering{suffix}-source.json").read_text())
if hashlib.sha256(archive.read_bytes()).hexdigest() != identity["archive_sha256"]:
    raise RuntimeError("engineering archive identity changed")
isolated = OUT / f"isolated-engineering{suffix}"
isolated.mkdir(exist_ok=False)
with tarfile.open(archive) as source:
    for member in source:
        relative = Path(member.name)
        if relative.is_absolute() or ".." in relative.parts or not member.isfile():
            raise RuntimeError(f"unsupported engineering archive member: {member.name}")
        target = isolated / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.extractfile(member).read())
        target.chmod(member.mode)
if (isolated / "docs/testing/evidence").exists():
    raise RuntimeError("historical evidence accidentally included")
commands = [
    [str(ROOT / ".cargo/bin/cargo"), "+1.89.0", "test", "--workspace", "--all-targets", "--all-features", "--locked", "--offline"],
    ["python3", "-m", "unittest", "discover", "-s", "scripts/validation/tests", "-v"],
]
results = []
env = dict(os.environ, CARGO_TARGET_DIR=str(OUT / f"isolated-target{suffix}"))
for index, command in enumerate(commands):
    start = time.monotonic()
    with (OUT / f"isolated{suffix}-{index}.log").open("wb") as log:
        result = subprocess.run(command, cwd=isolated, env=env, stdout=log, stderr=subprocess.STDOUT)
    results.append({"command": command, "exit_code": result.returncode, "seconds": time.monotonic() - start,
                    "log": f"isolated{suffix}-{index}.log"})
    (OUT / f"isolated{suffix}-results.json").write_text(json.dumps(
        {"source_sha256": identity["source_sha256"], "historical_evidence_present": False,
         "status": "running", "results": results}, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError("isolated offline regression failed")
(OUT / f"isolated{suffix}-results.json").write_text(json.dumps(
    {"source_sha256": identity["source_sha256"], "historical_evidence_present": False,
     "status": "passed", "results": results}, indent=2) + "\n")
print("isolated offline regressions passed")
