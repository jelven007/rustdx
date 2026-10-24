"""Verify retained evidence and current documentation without running services."""
import ast
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import tarfile
import urllib.parse
import zipfile

root = pathlib.Path("/Users/bytedance/Documents/rustdx")
evidence = root / "docs/testing/evidence/acceptance-stage5-20261024"
checks = []


def sha(data):
    return hashlib.sha256(data).hexdigest()


for name in [
    "scripts/bench/measure_database_readback.py",
    "scripts/validation/verify_database_faults.py",
    "scripts/validation/verify_real_day_database.py",
    *[str(path.relative_to(root)) for path in sorted((evidence / "reproduce").glob("*.py"))],
]:
    ast.parse((root / name).read_text())
    checks.append({"check": "python_syntax", "path": name, "passed": True})

snapshots = []
for prefix in ["source", "source-final", "source-complete", "source-tested",
               "source-accepted", "source-rowbinary", "source-rowbinary-final",
               "source-pacing-final"]:
    manifest = json.loads((evidence / (prefix + "-manifest.json")).read_text())
    canonical = json.dumps(manifest["files"], sort_keys=True, separators=(",", ":")).encode()
    assert sha(canonical) == manifest["source_sha256"]
    with tarfile.open(evidence / (prefix + ".tar.gz")) as archive:
        assert len(archive.getmembers()) == len(manifest["files"])
        for item in manifest["files"]:
            data = archive.extractfile(item["path"]).read()
            assert len(data) == item["bytes"] and sha(data) == item["sha256"], item["path"]
            if prefix == "source-pacing-final":
                assert (root / item["path"]).read_bytes() == data, item["path"]
    snapshots.append({
        "manifest": prefix + "-manifest.json",
        "source_sha256": manifest["source_sha256"],
        "files": len(manifest["files"]),
        "archive_sha256": sha((evidence / (prefix + ".tar.gz")).read_bytes()),
        "current_workspace_exact": prefix == "source-pacing-final",
    })
checks.append({"check": "source_snapshots_and_final_workspace", "passed": True, "snapshots": snapshots})

provenance = json.loads(
    (root / "crates/rustdx-core/tests/fixtures/protocol/stage5-provenance.json").read_text()
)
for item in provenance["files"]:
    data = (root / "crates/rustdx-core/tests/fixtures/protocol" / item["fixture"]).read_bytes()
    assert data == (root / item["original_path"]).read_bytes()
    assert sha(data) == item["sha256"]
checks.append({"check": "compile_time_fixture_original_bytes", "passed": True, "files": 3})

subset = json.loads((evidence / "official-subset-provenance.json").read_text())
assert sha((evidence / "official-subset.zip").read_bytes()) == subset["subset_sha256"]
with zipfile.ZipFile(evidence / "official-subset.zip") as archive:
    assert len(archive.namelist()) == 128
    for item in subset["members"]:
        data = archive.read(item["member"])
        assert len(data) == item["bytes"] and sha(data) == item["sha256"]
checks.append({
    "check": "official_subset_original_member_bytes_and_crc",
    "passed": True, "members": 128, "rows": sum(item["rows"] for item in subset["members"]),
    "original_zip_sha256": subset["original_sha256"],
    "derivative_zip_sha256": subset["subset_sha256"],
})

packaged = json.loads((evidence / "linux/packaged-files.json").read_text())
transfer = json.loads((evidence / "linux-transfer.json").read_text())
archive_bytes = (evidence / "linux-evidence.tar.gz").read_bytes()
assert len(archive_bytes) == transfer["archive_bytes"]
assert sha(archive_bytes) == transfer["archive_sha256"]
assert len(packaged) == transfer["files"]
for item in packaged:
    relative = pathlib.Path(item["packaged"])
    assert not relative.is_absolute() and ".." not in relative.parts
    data = (evidence / "linux" / relative).read_bytes()
    assert len(data) == item["bytes"] and sha(data) == item["sha256"], item["packaged"]
checks.append({"check": "linux_packaged_files", "passed": True, "files": len(packaged)})

linux = evidence / "linux/evidence"
environment = json.loads((linux / "environment.json").read_text())
assert environment["source_sha256"] == snapshots[-1]["source_sha256"]
assert environment["source_reverified_after_execution"]
binary = environment["binary_sha256"]["rustdx"]
history = json.loads((linux / "real-history.json").read_text())
assert history["binary_sha256"] == binary
assert history["completed"] and history["private_database_removed"]
assert not history["cleanup_errors"]
assert history["rows"] == history["independent_decoded_rows"] == 779200
assert len(history["runs"]) == 6
assert {run["csv_sha256"] for run in history["runs"]} == {
    "80f1b74671bcc1d8d3aa5bf7693e5dbacdcf04c60f40300255f23694fffa46e5"
}
for run in history["runs"]:
    assert run["exit"] == 0 and run["status"] == "complete"
    if run["name"].startswith(("clickhouse-", "mongodb-")):
        assert run["database_status"] == "verified"
        assert run["database_verification"]["rows"] == 779200
bulk = json.loads((linux / "bulk.json").read_text())
assert bulk["binary_sha256"] == binary and bulk["rows"] == 65536
assert len(bulk["runs"]) == 4
for run in bulk["runs"]:
    assert run["exit"] == 0 and run["manifest"]["database_status"] == "verified"
    assert run["manifest"]["database_verification"]["rows"] == 65536
faults = json.loads((linux / "faults.json").read_text())
assert faults["binary_sha256"] == binary
assert faults["completed"] and faults["private_database_removed"] and not faults["cleanup_errors"]
assert len(faults["cases"]) == 4
for case in faults["cases"]:
    assert case["database_status_after_fault"] == "unknown"
    assert case["recovery_verified_rows"] == case["recovery_verification"]["rows"] == 8192
    assert case["run_archives"] == 2
probe = json.loads((linux / "float-probe/rowbinary-results.json").read_text())
assert probe["rowbinary_exact_low_bits"] and probe["version_default_generated"]
assert probe["bits"]["low_bits"] == 4619499763780875715
assert len(bytes.fromhex(probe["payload_hex"])) == 59
checks.append({"check": "final_source_binary_database_and_float_evidence",
               "passed": True, "source_sha256": environment["source_sha256"],
               "binary_sha256": binary, "real_rows": 779200, "fault_cases": 4})

for relative in ["local-checks/results.json", "capacity-final-checks/results.json",
                 "rowbinary-checks/results.json", "linux/evidence/results.json"]:
    results = json.loads((evidence / relative).read_text())
    assert all(item["exit_code"] == 0 for item in results)
    checks.append({"check": "recorded_command_exit_codes", "passed": True, "path": relative,
                   "commands": len(results)})
assert json.loads((evidence / "linux/evidence/completed.json").read_text())["complete"]
cleanup = json.loads((evidence / "linux/evidence/cleanup-audit.json").read_text())
assert cleanup["owned_containers_removed_verified"] and cleanup["daemon_stopped_verified"]
assert not cleanup["volumes_after"]
checks.append({"check": "independently_audited_linux_cleanup", "passed": True})

files = [
    "docs/README.md", "docs/archive/README.md",
    "docs/archive/development/REMAINING_ACCEPTANCE_STAGE5_20261024.md",
    "docs/testing/PRODUCTION_ACCEPTANCE.md", "docs/testing/DATABASE_ACCEPTANCE.md",
    "docs/requirements/REQUIREMENTS.md", "docs/development/APPLICATION_INTEGRATION.md",
    "crates/rustdx-core/tests/fixtures/README.md", "docs/operations/IMPORTS.md",
    "docs/testing/evidence/acceptance-stage5-20261024/README.md",
]
count = 0
for name in files:
    path = root / name
    text = re.sub(r"```.*?```", "", path.read_text(), flags=re.S)
    for href in re.findall(r"\[[^\]]*\]\(([^\s)]+)", text):
        target = urllib.parse.unquote(href.split("#")[0])
        if not target or re.match(r"^[a-zA-Z]+:", target):
            continue
        resolved = (path.parent / target).resolve()
        # This verification creates its own linked output after all other checks.
        if resolved != (evidence / "final-verification.json").resolve():
            assert resolved.exists(), (name, target)
        count += 1
checks.append({"check": "current_markdown_local_targets", "passed": True,
               "files": len(files), "links": count, "anchors_and_external_urls": "not checked"})

result = subprocess.run(["git", "diff", "--check"], cwd=root, text=True, capture_output=True)
(evidence / "final-diff-check.log").write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
checks.append({"check": "git_diff_check", "passed": True, "exit_code": result.returncode})

report = {
    "recorded_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "passed": True, "checks": checks,
    "current_document_sha256": {name: sha((root / name).read_bytes()) for name in files},
    "tested_source_sha256": snapshots[-1]["source_sha256"],
    "full_market_accepted": False, "production_deployment_accepted": False,
    "tcp_cross_source_integer_shares_exact": False,
}
(evidence / "final-verification.json").write_text(json.dumps(report, indent=2) + "\n")
assert (evidence / "final-verification.json").is_file()
print(json.dumps({"passed": True, "checks": len(checks), "local_links": count,
                  "source_sha256": report["tested_source_sha256"]}))
