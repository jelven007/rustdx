"""Three bounded reproductions of the unchanged process-group cleanup assertion."""
import datetime as dt
import json
import os
from pathlib import Path
import subprocess

root = Path("/private/tmp/rustdx-upstream-capabilities-20261027")
art = Path(__file__).resolve().parent
env = dict(os.environ,
           CARGO_HOME="/Users/bytedance/Documents/rustdx/.cargo",
           RUSTUP_HOME="/Users/bytedance/Documents/rustdx/.cargo/rustup",
           CARGO_TARGET_DIR="/private/tmp/rustdx-system-optimization-packages-20261025/target",
           RUSTDX_SKIP_INTEGRATION_TESTS="1")
env["PATH"] = env["CARGO_HOME"] + "/bin:" + env["PATH"]
command = ["cargo", "+1.89.0", "test", "--offline", "--locked", "-p", "rustdx-runner",
           "--lib", "--all-features", "-j2",
           "database_process::tests::cancellation_and_deadline_kill_blocked_pipe_and_descendants",
           "--", "--exact"]
results = []
for index in range(1, 4):
    started = dt.datetime.now(dt.timezone.utc).isoformat()
    log = f"process-cleanup-isolated-{index}.log"
    with (art / log).open("w") as output:
        result = subprocess.run(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT,
                                timeout=60)
    results.append(dict(command=command, exit_code=result.returncode, log=log,
                        started_utc=started, finished_utc=dt.datetime.now(dt.timezone.utc).isoformat()))
(art / "isolated-cleanup-results.json").write_text(json.dumps(results, indent=2) + "\n")
print(json.dumps(results, indent=2))
