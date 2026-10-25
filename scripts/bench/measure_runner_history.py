#!/usr/bin/env python3
"""Measure current runner status/metrics on synthetic history, never a live root."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import sqlite3
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=20000)
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 100000 or not 1 <= args.repetitions <= 30:
        parser.error("jobs 1..100000; repetitions 1..30")
    binary = args.binary.resolve(strict=True)
    if args.output.exists():
        parser.error("output must be new")
    work = Path(tempfile.mkdtemp(prefix="rustdx-runner-history-"))
    root = work / "synthetic"
    root.mkdir()
    commands = []

    def run(command, name):
        start = time.monotonic()
        result = subprocess.run(command, capture_output=True, timeout=300)
        elapsed = time.monotonic() - start
        (work / (name + ".stdout")).write_bytes(result.stdout)
        (work / (name + ".stderr")).write_bytes(result.stderr)
        commands.append({"command": list(map(str, command)), "exit_code": result.returncode,
                         "seconds": elapsed, "log": name})
        if result.returncode:
            raise RuntimeError(f"{name} failed; see {work}")
        return elapsed, result

    run([str(binary), "migrate", "--root", str(root)], "initialize")
    # Old-schema synthetic fixture exercises explicit migration, not live writes.
    payload = json.dumps({"schema_version": 1, "status": "complete",
                          "network": {"requests": 9, "request_failures": 2,
                                      "empty_responses": 1, "acquisition_timeouts": 3,
                                      "queue_wait_micros": 120},
                          "observations": {"stages": {"parse": {
                              "calls": 2, "failures": 1, "completed_span_ms": 2000.0}}}})
    with sqlite3.connect(root / "ledger.sqlite") as db:
        db.executescript(
            "DROP TABLE observation_counters; DROP TABLE observation_stages;"
            "DROP INDEX attempts_by_job; DROP INDEX jobs_by_created;"
            "DROP TRIGGER immutable_observations_update; DROP TRIGGER immutable_observations_delete;"
            "PRAGMA user_version=3;"
        )
        for n in range(args.jobs):
            identifier = f"synthetic-{n}"
            db.execute("INSERT INTO jobs VALUES(?,'synthetic','{}','failed',?,'')",
                       (identifier, "2026-07-07T00:00:00+00:00"))
            db.execute("INSERT INTO attempts(id,job,state,directory,started) VALUES(?,?,'failed',?,?)",
                       (identifier, identifier, str(root / "attempts" / identifier), "2026-07-07T00:00:00+00:00"))
            db.execute("INSERT INTO attempt_observations VALUES(?,'worker',?,?)",
                       (identifier, payload, "2026-07-07T00:00:00+00:00"))
    run([str(binary), "migrate", "--root", str(root)], "migrate-history")
    measured = {}
    for action in ("metrics", "status"):
        samples = []
        for n in range(args.repetitions):
            command = [str(binary), action, "--root", str(root)]
            host = platform.system()
            if host == "Darwin":
                command = ["/usr/bin/time", "-l"] + command
            elif host == "Linux" and Path("/usr/bin/time").exists():
                command = ["/usr/bin/time", "-v"] + command
            elapsed, result = run(command, f"{action}-{n}")
            stderr = result.stderr.decode()
            if host == "Darwin":
                match = re.search(r"(\d+)\s+maximum resident set size", stderr)
                rss = int(match[1]) if match else None
            else:
                match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", stderr)
                rss = int(match[1]) * 1024 if match else None
            if action == "metrics":
                expected = f'rustdx_observed_requests_total{{owner="worker"}} {args.jobs * 9}\n'
                if expected.encode() not in result.stdout:
                    raise AssertionError("projection totals differ from synthetic history")
            elif len(json.loads(result.stdout)) != min(args.jobs, 1000):
                raise AssertionError("status result size differs")
            samples.append({"seconds": elapsed, "rss_bytes": rss})
        measured[action] = {
            "samples": samples,
            "median_seconds": statistics.median(s["seconds"] for s in samples),
            "maximum_rss_bytes": max((s["rss_bytes"] for s in samples if s["rss_bytes"] is not None), default=None),
        }
    result = {"schema_version": 1, "status": "passed", "kind": "synthetic_history",
              "jobs": args.jobs, "observations": args.jobs, "platform": platform.platform(),
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "work": str(work), "measurements": measured, "commands": commands,
              "scope": "single-host CLI latency/RSS; not production workload or process-tree collection RSS"}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        json.dump(result, output, indent=2, allow_nan=False)
        output.write("\n")
    print(args.output)


if __name__ == "__main__":
    main()
