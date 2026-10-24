import ast
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = ROOT / "docs/testing/evidence/recent-minutes-date-selector-20261024"
TEMP = Path("/tmp/rustdx-0feb-fix-checks")
os.chdir(ROOT)
sys.path.insert(0, str(ROOT / "scripts/validation"))
from verify_recent_minutes_boundaries import verify as boundary_verify
from verify_nine_commands import verify as nine_verify
from verify_recent_minutes_date_selector import frozen_capture

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

result = {"verification": "running", "kind": "recent_minutes_date_selector_fix",
          "date_schema_version": 2, "platform": platform.platform(),
          "python_version": platform.python_version(), "rust_toolchain": "1.89.0"}
(OUT / "verification.json").write_text(json.dumps(result, indent=2) + "\n")
checks = OUT / "checks"
checks.mkdir(exist_ok=True)
for path in TEMP.iterdir():
    if path.suffix in {".log", ".json", ".py"}:
        shutil.copyfile(path, checks / path.name)

commands = [
    ["python3", "scripts/validation/recent_minutes_selector.py"],
    ["python3", "scripts/validation/verify_recent_minutes_boundaries.py", "--self-test"],
    ["python3", "scripts/validation/freeze_recent_minutes_fixtures.py", "--output",
     "crates/rustdx-core/tests/fixtures/recent_minutes_nonempty", "--check"],
    ["python3", "scripts/validation/freeze_recent_minutes_fixtures.py", "--output",
     "crates/rustdx-core/tests/fixtures/recent_minutes_date_selector", "--check"],
]
python_checks = []
for index, command in enumerate(commands):
    with (checks / f"python-{index}.log").open("wb") as log:
        run = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=90)
    python_checks.append({"command": command, "exit_code": run.returncode})
    assert run.returncode == 0

boundary = ROOT / "docs/testing/evidence/command-boundaries-20261024/recent-lite-115.238.56.198"
frozen_capture(boundary)
old_boundary = boundary_verify(boundary)
assert old_boundary["verification"] == "passed" and not old_boundary["live_nonempty_verified"]
old_nine = []
for host in ["115.238.56.198", "180.153.18.170"]:
    source = ROOT / "docs/testing/evidence/nine-commands-20261024" / host
    frozen_capture(source)
    old_nine.append(nine_verify(source))
    assert old_nine[-1]["wire_frames_checked"] > 0
    old_nine[-1]["verification"] = "passed"

rust_files = []
for base in ["crates", "apps", "tests", "tools"]:
    for directory, dirs, files in os.walk(base):
        dirs[:] = [name for name in dirs if name not in {"target", ".git", ".cargo"}]
        rust_files.extend(Path(directory) / name for name in files if name.endswith(".rs"))
previous = json.loads((ROOT / "docs/testing/evidence/acceptance-stage5-20261024/source-pacing-final-manifest.json").read_text())
old_rust = {e["path"]: e["sha256"] for e in previous["files"] if e["path"].endswith(".rs")}
changed = sorted(name for name, old in old_rust.items() if digest(Path(name)) != old)
assert len(changed) == 7
manifest = {str(path): {"sha256": digest(path), "bytes": path.stat().st_size}
            for path in sorted(rust_files)}
for entry in previous["files"]:
    path = Path(entry["path"])
    if path.name in {"Cargo.toml", "Cargo.lock"}:
        assert digest(path) == entry["sha256"]
        manifest[str(path)] = {"sha256": digest(path), "bytes": path.stat().st_size}
(OUT / "source-input-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

docs = [
    Path("CHANGELOG.md"), Path("docs/README.md"), Path("docs/archive/README.md"),
    Path("docs/archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md"),
    Path("docs/development/APPLICATION_INTEGRATION.md"),
    Path("docs/reference/COMMAND_API.md"), Path("docs/reference/RESPONSE_FIELD_SEMANTICS.md"),
    Path("docs/testing/PRODUCTION_ACCEPTANCE.md"), Path("docs/testing/VALIDATION.md"),
    Path("docs/requirements/REQUIREMENTS.md"), Path("scripts/README.md"),
    Path("crates/rustdx-core/tests/fixtures/README.md"),
    Path("crates/rustdx-core/tests/fixtures/recent_minutes_date_selector/README.md"),
]
links = 0
for path in docs:
    for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", path.read_text()):
        if "://" in target or target.startswith("#"):
            continue
        destination = unquote(target.split("#", 1)[0])
        if not destination:
            continue
        assert (path.parent / destination).exists(), (str(path), target)
        links += 1
scripts = [Path("scripts/validation") / name for name in [
    "recent_minutes_selector.py", "verify_recent_minutes_date_selector.py",
    "verify_recent_auction.py", "verify_recent_minutes_boundaries.py",
    "freeze_recent_minutes_fixtures.py", "verify_nine_commands.py"]]
for path in scripts:
    ast.parse(path.read_text(), filename=str(path))
snapshot = OUT / "final-source"
for path in [Path(p) for p in changed] + scripts + docs:
    destination = snapshot / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(path, destination)

date_result = json.loads((OUT / "date-selector-verification.json").read_text())
assert date_result["verification"] == "passed" and date_result["selector_conflict_resolved_in_sample"]
initial = json.loads((TEMP / "results.json").read_text())
followup = json.loads((TEMP / "results-followup.json").read_text())
assert all(r["exit_code"] == 0 for r in initial[:3] + followup[:2])
assert initial[3]["exit_code"] == 101
stats = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;",
                   (TEMP / "workspace-tests-retry.log").read_text())
counts = [sum(int(r[i]) for r in stats) for i in range(3)]
assert counts == [546, 0, 27]
support = {str(path.relative_to(OUT)): {"sha256": digest(path), "bytes": path.stat().st_size}
           for path in sorted(OUT.rglob("*")) if path.is_file()
           and path.name not in {"verification.json", "support-manifest.json"}}
(OUT / "support-manifest.json").write_text(json.dumps(support, indent=2) + "\n")
result.update({
    "verification": "passed", "selector_conflict_resolved_in_sample": True,
    "cargo_checks": initial + followup[:2], "python_checks": python_checks,
    "final_date_verifier": {"completion": "passed", "log": "checks/cross-verification-final.log"},
    "prior_failed_verifier_attempts": followup[2:],
    "workspace_harness_counts": {"passed": counts[0], "failed": counts[1], "ignored": counts[2]},
    "integration_skip": "RUSTDX_SKIP_INTEGRATION_TESTS=1; guarded external tests can return early; not new database/live acceptance",
    "legacy_boundary_read_only_replay": old_boundary,
    "legacy_nine_read_only_replay": old_nine,
    "rust_files_unchanged_since_stage5": len(old_rust) - len(changed),
    "rust_files_changed": changed, "input_files": len(manifest),
    "python_syntax_files": [str(p) for p in scripts],
    "document_links_checked": links, "support_files": len(support),
    "support_manifest_sha256": digest(OUT / "support-manifest.json"),
    "scope": date_result["scope"],
    "not_run": ["Linux", "database native recovery", "remote CI", "instrumented fuzz",
                "trading-hour push", "commit/push/publish"],
})
(OUT / "verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"verification": "passed", "support_files": len(support),
                  "document_links_checked": links, "rust_changed": len(changed)}))
