import hashlib
import json
from pathlib import Path
import sys
import tarfile

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path(__file__).parent
PACKAGE = Path("/tmp/rustdx-system-optimization-packages-fixed-20261025")
sys.path.insert(0, str(ROOT / "scripts/release"))
from source_scope import inventory

report = json.loads((PACKAGE / "preflight.json").read_text())
if report["result"] != "passed":
    raise RuntimeError("package checks incomplete")
records, current = inventory(ROOT)
packages = []
for row in report["packages"]:
    crate = PACKAGE / row["crate"]
    if hashlib.sha256(crate.read_bytes()).hexdigest() != row["sha256"]:
        raise RuntimeError("audited crate changed")
    relative = {"jelven-rustdx-complete": "crates/rustdx-core", "jelven-rustdx-cli": "apps/rustdx-cli"}[row["name"]]
    required = [r["path"] for r in records if r["path"].startswith(relative + "/src/")]
    required += [relative + "/README.md", relative + ("/LICENSE" if relative.endswith("core") else "/LICENSE-MIT")]
    with tarfile.open(crate) as archive:
        files = {member.name.split("/", 1)[1]: member for member in archive.getmembers() if member.isfile()}
        for name in required:
            local = ROOT / name
            member = files[local.relative_to(ROOT / relative).as_posix()]
            if hashlib.sha256(archive.extractfile(member).read()).hexdigest() != hashlib.sha256(local.read_bytes()).hexdigest():
                raise RuntimeError(f"packaged source or documentation differs: {name}")
    packages.append({"name": row["name"], "sha256": row["sha256"], "checked_source_and_license_files": len(required)})
(OUT / "current-package-equivalence.json").write_text(json.dumps(
    {"status": "passed", "checked_preflight_source_sha256": report["source_sha256"],
     "current_engineering_source_sha256": current, "packages": packages,
     "publication_qualified": False, "meaning": "Original successful package/consumer audit retained. Current production src, README and license bytes equal actual crates; added engineering inputs are checked separately."},
    indent=2) + "\n")
print(packages)
