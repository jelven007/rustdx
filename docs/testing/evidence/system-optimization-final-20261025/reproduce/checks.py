import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path("/Users/bytedance/Documents/rustdx")
OUT = Path(__file__).parent
env = dict(os.environ, CARGO_HOME=str(ROOT / ".cargo"), RUSTUP_HOME=str(ROOT / ".cargo/rustup"),
           CARGO_TARGET_DIR="/tmp/rustdx-system-review-20261025/target", CARGO_INCREMENTAL="0",
           RUSTDX_SKIP_INTEGRATION_TESTS="1", PATH=str(ROOT / ".cargo/bin") + ":" + os.environ["PATH"])

def run(job, commands):
    records = []
    for index, command in enumerate(commands):
        status = OUT / (job + ".json")
        status.write_text(json.dumps({"status": "running", "command": command, "results": records}, indent=2))
        start = time.monotonic()
        with (OUT / f"{job}-{index}.log").open("wb") as log:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        records.append({"command": command, "exit_code": result.returncode,
                        "seconds": time.monotonic() - start, "log": f"{job}-{index}.log"})
        if result.returncode:
            break
    status.write_text(json.dumps({"status": "passed" if all(r["exit_code"] == 0 for r in records) else "failed",
                                 "results": records}, indent=2) + "\n")

if __name__ == "__main__":
    job, commands = sys.argv[1], json.loads(sys.argv[2])
    if len(sys.argv) > 3:
        run(job, commands)
    else:
        process = subprocess.Popen([sys.executable, __file__, job, json.dumps(commands), "worker"],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, start_new_session=True)
        print(json.dumps({"pid": process.pid, "status": str(OUT / (job + ".json"))}))
