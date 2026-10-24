"""Capture final identities and independently verify cleanup of this private run."""
import datetime
import hashlib
import json
import os
import pathlib
import platform
import re
import signal
import subprocess
import time
import tomllib

task = pathlib.Path("/tmp/rustdx-acceptance-20261024-TLDCn5")
evidence = task / "evidence"
docker = [str(task / "infra/docker/docker"), "-H", "unix://" + str(task / "docker.sock")]


def output(args):
    return subprocess.check_output(args, text=True, timeout=30).strip()


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


assert json.loads((evidence / "completed.json").read_text())["complete"]
results = json.loads((evidence / "results.json").read_text())
assert all(item["exit_code"] == 0 for item in results)
source_manifest = json.loads((task / "source-manifest.json").read_text())
for entry in source_manifest["files"]:
    assert sha(task / "source" / entry["path"]) == entry["sha256"], entry["path"]
images = json.loads("[" + output(docker + ["image", "ls", "--format", "{{json .}}"]).replace("\n", ",") + "]")
image_ids = sorted({item["ID"] for item in images})
image_inspect = json.loads(output(docker + ["image", "inspect", *image_ids]))
(evidence / "images.json").write_text(json.dumps(image_inspect, indent=2) + "\n")
environment = {
    "recorded_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "platform": platform.platform(),
    "os_release": pathlib.Path("/etc/os-release").read_text(),
    "uid": os.getuid(),
    "source_sha256": source_manifest["source_sha256"],
    "source_files": len(source_manifest["files"]),
    "binary_profile": "debug; functional acceptance, not release performance measurement",
    "binary_sha256": {
        name: sha(task / "source/target/debug" / name)
        for name in ["rustdx", "rustdx-runner"]
    },
    "tools": {
        name: {
            "sha256": sha(task / "toolchain/bin" / name),
            "version": output([str(task / "toolchain/bin" / name), "--version"]),
        }
        for name in ["rustc", "cargo"]
    },
    "database_limits": "container inspect is authoritative; host network, loopback only, private daemon",
    "source_reverified_after_execution": True,
    "cargo_lock_sha256": sha(task / "source/Cargo.lock"),
    "integration_env": {
        "RUSTDX_SKIP_INTEGRATION_TESTS": "1",
        "RUSTDX_DB_ACCEPTANCE": "1",
        "RUSTDX_TEST_CLI": str(task / "source/target/debug/rustdx"),
        "RUSTDX_TEST_CH_URL": "http://127.0.0.1:18123",
        "RUSTDX_TEST_CH_USER": "default",
        "RUSTDX_TEST_CH_PASSWORD": "",
        "RUSTDX_TEST_MONGO_URI": "mongodb://127.0.0.1:27018",
        "RUSTDX_TEST_MONGOSH": str(task / "bin/mongosh-2.5.10-linux-x64/bin/mongosh"),
    },
    "github_actions": False,
    "production_deployment": False,
}
lock = tomllib.loads((task / "source/Cargo.lock").read_text())
linux_raw = next(item for item in lock["package"] if item["name"] == "linux-raw-sys")
cached = list((task / "cargo/registry/cache").glob("*/linux-raw-sys-0.12.1.crate"))
assert len(cached) == 1
assert sha(cached[0]) == linux_raw["checksum"]
environment["linux_raw_sys"] = {
    "version": linux_raw["version"],
    "sha256": sha(cached[0]),
    "matched_existing_cargo_lock": True,
}
(evidence / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
stats = {}
for name in ["workspace-tests", "doc-tests", "runner-all"]:
    text = (task / "logs" / (name + ".log")).read_text()
    matches = re.findall(
        r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; "
        r"(\d+) measured; (\d+) filtered out", text
    )
    totals = {
        key: sum(int(item[index]) for item in matches)
        for index, key in enumerate(["passed", "failed", "ignored", "measured", "filtered_out"])
    }
    totals["test_binaries_with_summary"] = len(matches)
    totals["env_skips_are_not_counted_by_rust_harness"] = name == "workspace-tests"
    totals["skip_lines"] = [line for line in text.splitlines() if "skip" in line.lower()]
    stats[name] = totals
assert stats["runner-all"]["ignored"] == 0
assert "real_databases_isolate_versions_reject_extra_keys_and_restore_local_evidence ... ok" in (
    task / "logs/runner-all.log"
).read_text()
(evidence / "test-statistics.json").write_text(json.dumps(stats, indent=2) + "\n")
remaining = output(docker + ["ps", "-aq"])
assert remaining == "", "private daemon still has containers"
volumes_before = output(docker + ["volume", "ls", "-q"]).splitlines()
prune = output(docker + ["volume", "prune", "--all", "--force"])
volumes_after = output(docker + ["volume", "ls", "-q"]).splitlines()
assert not volumes_after
pid = 547562
cmdline = pathlib.Path(f"/proc/{pid}/cmdline").read_bytes()
assert str(task).encode() in cmdline and b"dockerd" in cmdline
os.kill(pid, signal.SIGTERM)
until = time.monotonic() + 20
while pathlib.Path(f"/proc/{pid}").exists() and time.monotonic() < until:
    state = pathlib.Path(f"/proc/{pid}/stat").read_text().split()[2]
    if state == "Z":
        break
    time.sleep(0.2)
alive = pathlib.Path(f"/proc/{pid}").exists()
if alive:
    alive = pathlib.Path(f"/proc/{pid}/stat").read_text().split()[2] != "Z"
assert not alive, "private daemon did not stop"
cleanup = {
    "recorded_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "private_daemon_container_ids_after_pipeline": remaining.splitlines(),
    "owned_containers_removed_verified": True,
    "volumes_before": volumes_before,
    "volume_prune_output": prune,
    "volumes_after": volumes_after,
    "daemon_pid": pid,
    "daemon_owned_path_verified_before_signal": True,
    "daemon_stopped_verified": not alive,
    "retained": "private source, binaries, logs, evidence and downloaded image cache",
    "system_configuration_changed": False,
}
(evidence / "cleanup-audit.json").write_text(json.dumps(cleanup, indent=2) + "\n")
print(json.dumps({"environment": environment, "tests": stats, "cleanup": cleanup}, indent=2))
