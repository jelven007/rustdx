"""Run current candidate checks and preserve real commands, exit codes and logs."""
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path("/private/tmp/rustdx-upstream-capabilities-20261027")
art = Path(__file__).resolve().parent
env = dict(os.environ,
           CARGO_HOME="/Users/bytedance/Documents/rustdx/.cargo",
           RUSTUP_HOME="/Users/bytedance/Documents/rustdx/.cargo/rustup",
           CARGO_TARGET_DIR="/private/tmp/rustdx-system-optimization-packages-20261025/target",
           RUSTDX_SKIP_INTEGRATION_TESTS="1")
env["PATH"] = env["CARGO_HOME"] + "/bin:" + env["PATH"]
cargo = ["cargo", "+1.89.0"]
scope = ["--offline", "--locked", "--workspace", "--all-features", "-j2"]
checks = [
    ("fmt", cargo + ["fmt", "--all", "--check"], {}),
    ("clippy", cargo + ["clippy", *scope, "--all-targets", "--", "-D", "warnings"], {}),
    ("workspace-tests", cargo + ["test", *scope], {}),
    ("rustdoc", cargo + ["doc", *scope, "--no-deps"], {"RUSTDOCFLAGS": "-D warnings"}),
    ("python-tests", ["python3", "-m", "unittest", "discover", "-s", "scripts/validation/tests"], {}),
    ("finance-consumption", ["python3", "scripts/validation/verify_consumption_fields.py",
        "--root", str(root), "--matrix", "config/consumption/finance-sample.json",
        "--output", str(art / "finance-consumption.json")], {}),
    ("consumers", ["python3", "scripts/release/check_consumers.py",
                  "--offline", "--output", str(art / "consumers")], {}),
    ("cash-caller", cargo + ["run", "--manifest-path", str(art / "probe-tool/Cargo.toml"),
        "--offline", "--locked", "-j2", "--bin", "cash-reconciliation-caller", "--",
        str(root / "crates/rustdx-core/tests/fixtures/finance-q3/response.bin")], {}),
    ("replay", ["python3", "docs/testing/evidence/upstream-capabilities-20261027/replay.py",
                "--output", str(art / "independent-replay-final.json")], {}),
    ("replay-negative", ["python3", "docs/testing/evidence/upstream-capabilities-20261027/replay_negative.py",
                         "--output", str(art / "negative-replay.json")], {}),
]
results = []
if sys.argv[1:] == ["--resume"]:
    previous = json.loads((art / "check-results.json").read_text())
    assert previous[-1]["exit_code"] != 0
    results = previous[:-1]
for name, command, extra in checks:
    if any(row["name"] == name and row["exit_code"] == 0 for row in results):
        continue
    started = dt.datetime.now(dt.timezone.utc).isoformat()
    with (art / (name + ".log")).open("w") as output:
        completed = subprocess.run(command, cwd=root, env=dict(env, **extra),
                                   stdout=output, stderr=subprocess.STDOUT)
    results.append(dict(name=name, command=command, environment_override=extra,
                        exit_code=completed.returncode, started_utc=started,
                        finished_utc=dt.datetime.now(dt.timezone.utc).isoformat(),
                        log=name + ".log"))
    (art / "check-results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{name}: exit {completed.returncode}", flush=True)
    if completed.returncode:
        raise SystemExit(completed.returncode)
