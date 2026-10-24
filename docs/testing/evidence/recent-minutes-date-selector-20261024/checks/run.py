import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path("/tmp/rustdx-0feb-fix-checks")
env = os.environ.copy()
env.update(CARGO_HOME=str(ROOT / ".cargo"), RUSTUP_HOME=str(ROOT / ".cargo/rustup"),
           CARGO_TERM_COLOR="never")
cargo = [str(ROOT / ".cargo/bin/cargo"), "+1.89.0"]
checks = [
    ("format", ["fmt", "--all"]),
    ("format-check", ["fmt", "--all", "--", "--check"]),
    ("targeted-tests", ["test", "-p", "jelven-rustdx-complete", "--offline", "--locked",
                       "--test", "recent_minutes_boundaries", "--test", "recent_minutes_fixtures",
                       "--test", "command_boundaries", "--test", "protocol_7709_decoders"]),
    ("workspace-tests", ["test", "--workspace", "--offline", "--locked"]),
    ("clippy", ["clippy", "--workspace", "--all-targets", "--all-features",
                "--offline", "--locked", "--", "-D", "warnings"]),
]
results = []
for name, args in checks:
    started = time.time()
    command = cargo + args
    with (OUT / (name + ".log")).open("wb") as log:
        try:
            code = subprocess.run(command, cwd=ROOT, env=env, stdout=log,
                                  stderr=subprocess.STDOUT, timeout=600).returncode
        except subprocess.TimeoutExpired:
            code = 124
    results.append({"name": name, "command": command, "exit_code": code,
                    "elapsed_seconds": round(time.time() - started, 3)})
    (OUT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    if code:
        break
