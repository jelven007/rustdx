"""Freeze a new RowBinary input while preserving all earlier source snapshots."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import tarfile

root = pathlib.Path("/Users/bytedance/Documents/rustdx")
stage = pathlib.Path("/tmp/rustdx-stage5")
evidence = root / "docs/testing/evidence/acceptance-stage5-20261024"
old = json.loads((evidence / "source-rowbinary-final-manifest.json").read_text())
paths = {record["path"] for record in old["files"]}
paths.add("apps/rustdx-cli/src/io/rowbinary.rs")
records = [{"path": name, "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest(),
            "bytes": (root / name).stat().st_size} for name in sorted(paths)]
fingerprint = hashlib.sha256(
    json.dumps(records, sort_keys=True, separators=(",", ":")).encode()
).hexdigest()
manifest = dict(old, files=records, source_sha256=fingerprint,
                prior_source_sha256=old["source_sha256"])
prefix = "source-pacing-final"
(stage / (prefix + "-manifest.json")).write_text(json.dumps(manifest, indent=2) + "\n")
with tarfile.open(stage / (prefix + ".tar.gz"), "w:gz") as bundle:
    for name in sorted(paths):
        bundle.add(root / name, arcname=name)
for suffix in ["-manifest.json", ".tar.gz"]:
    name = prefix + suffix
    shutil.copy2(stage / name, evidence / name)
    subprocess.run(["scp", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8",
                    str(stage / name),
                    "trae-ecs:/tmp/rustdx-acceptance-20261024-TLDCn5/" + name],
                   check=True, timeout=120)
print(json.dumps({"source_sha256": fingerprint, "files": len(records),
                  "runtime_files": len(manifest["runtime_evidence_paths"]),
                  "archive_bytes": (stage / (prefix + ".tar.gz")).stat().st_size}), flush=True)
