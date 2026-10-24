import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path("/tmp/rustdx-0feb-fix-checks")
env = os.environ.copy()
env.update(CARGO_HOME=str(ROOT / ".cargo"), RUSTUP_HOME=str(ROOT / ".cargo/rustup"),
           CARGO_TERM_COLOR="never", RUSTDX_SKIP_INTEGRATION_TESTS="1")
cargo = [str(ROOT / ".cargo/bin/cargo"), "+1.89.0"]
checks = [
    ("workspace-tests-retry", cargo + ["test", "--workspace", "--offline", "--locked"]),
    ("clippy", cargo + ["clippy", "--workspace", "--all-targets", "--all-features",
                       "--offline", "--locked", "--", "-D", "warnings"]),
    ("cross-verification", ["python3", "scripts/validation/verify_recent_minutes_date_selector.py",
                           "docs/testing/evidence/recent-minutes-date-selector-20261024",
                           "--output", "docs/testing/evidence/recent-minutes-date-selector-20261024/date-selector-verification.json"]),
]
results = []
for name, command in checks:
    started = time.time()
    with (OUT / (name + ".log")).open("wb") as log:
        try:
            code = subprocess.run(command, cwd=ROOT, env=env, stdout=log,
                                  stderr=subprocess.STDOUT, timeout=600).returncode
        except subprocess.TimeoutExpired:
            code = 124
    results.append({"name": name, "command": command, "exit_code": code,
                    "elapsed_seconds": round(time.time() - started, 3),
                    "RUSTDX_SKIP_INTEGRATION_TESTS": "1"})
    (OUT / "results-followup.json").write_text(json.dumps(results, indent=2) + "\n")
    if code:
        break
