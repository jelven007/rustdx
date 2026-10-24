"""Run focused Rust checks for the RowBinary change using the project's MSRV."""
import json
import os
import pathlib
import subprocess
import time

root = pathlib.Path("/Users/bytedance/Documents/rustdx")
out = root / "docs/testing/evidence/acceptance-stage5-20261024/rowbinary-checks"
out.mkdir(exist_ok=True)
env = dict(os.environ, CARGO_HOME=str(root / ".cargo"),
           RUSTUP_HOME=str(root / ".cargo/rustup"), RUSTDX_SKIP_INTEGRATION_TESTS="1")
cargo = [str(root / ".cargo/bin/cargo"), "+1.89.0"]
results = []
commands = [
    ("fmt", cargo + ["fmt", "--all", "--", "--check"]),
    ("io-tests", cargo + ["test", "-p", "jelven-rustdx-cli", "--bin", "rustdx",
                         "io::", "--all-features", "--offline", "--locked"]),
    ("pagination-tests", cargo + ["test", "-p", "jelven-rustdx-complete",
                                 "--test", "transaction_pagination",
                                 "--all-features", "--offline", "--locked"]),
    ("clippy", cargo + ["clippy", "--workspace", "--all-targets", "--all-features",
                       "--offline", "--locked", "--", "-D", "warnings"]),
]
for name, cmd in commands:
    begin = time.monotonic()
    with (out / (name + ".log")).open("w") as log:
        result = subprocess.run(cmd, cwd=root, env=env, stdout=log,
                                stderr=subprocess.STDOUT, timeout=900)
    results.append(dict(name=name, command=cmd, exit_code=result.returncode,
                        seconds=time.monotonic() - begin))
    (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    if result.returncode:
        raise SystemExit(result.returncode)
