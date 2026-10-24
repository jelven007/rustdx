"""Package this isolated run's bounded logs and manifests without full CSV copies."""
import hashlib
import json
import pathlib
import shutil
import tarfile

task = pathlib.Path("/tmp/rustdx-acceptance-20261024-TLDCn5")
out = task / "package"
out.mkdir(exist_ok=True)
mapping = []


def copy(source, relative):
    if not source.is_file():
        return
    target = out / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
    mapping.append({
        "original": str(source), "packaged": str(relative),
        "bytes": target.stat().st_size,
        "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
    })


for directory in ["evidence", "logs", "rust-mirror"]:
    for source in (task / directory).rglob("*"):
        if source.is_file() and source.suffix in [".json", ".log"]:
            copy(source, pathlib.Path(directory) / source.relative_to(task / directory))
for name in ["acceptance.xml", "rust-installed.json", "bootstrap-result.json",
             "source-manifest.json", "official-subset-provenance.json",
             "run.py", "mirror-rust.py", "run-controller.log"]:
    copy(task / name, pathlib.Path(name))
for name in ["bulk", "faults", "real-history"]:
    result_file = task / "evidence" / (name + ".json")
    if not result_file.exists():
        continue
    scratch = pathlib.Path(json.loads(result_file.read_text())["scratch"])
    assert str(scratch).startswith("/tmp/rustdx-")
    for source in scratch.rglob("*"):
        if source.is_file() and source.suffix in [".json", ".log", ".jsonl"]:
            copy(source, pathlib.Path("scratch") / name / source.relative_to(scratch))
(out / "packaged-files.json").write_text(json.dumps(mapping, indent=2) + "\n")
archive = task / "linux-evidence.tar.gz"
with tarfile.open(archive, "w:gz") as bundle:
    bundle.add(out, arcname="linux")
print(json.dumps({
    "archive": str(archive), "bytes": archive.stat().st_size,
    "files": len(mapping), "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
}))
