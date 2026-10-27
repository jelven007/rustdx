#!/usr/bin/env python3
"""Inventory external Rust tests, or explicitly execute a strict bounded online suite."""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
SUITES = [
    ("jelven-rustdx-complete", "crates/rustdx-core", name)
    for name in ["tcp", "auto_reconnect", "adj_field_probe", "bars_range_live",
                 "k_batch_adjusted", "minute_time_live"]
] + [("tests-integration", "tests/integration", "stocklist")]


def inventory(root):
    suites = []
    for package, directory, name in SUITES:
        source = root / directory / "tests" / (name + ".rs")
        tests = re.findall(r"#\[test\]((?:\s*#\[[^\n]*\])*)\s*fn\s+(\w+)", source.read_text())
        if not tests or any("#[ignore" not in attributes for attributes, _ in tests):
            raise ValueError(f"{source}: every external test must be explicitly ignored")
        suites.append(dict(package=package, target=name, tests=[
            dict(name=test, status="skipped", reason="online execution was not requested")
            for _, test in tests]))
    return suites


def execute(root, suite, cargo, timeout, output):
    command = [cargo, "test", "-p", suite["package"], "--test", suite["target"],
               "--locked", "--", "--ignored", "--test-threads=1", "--nocapture"]
    environment = dict(os.environ)
    environment.pop("RUSTDX_SKIP_INTEGRATION_TESTS", None)
    path = output / (suite["target"] + ".log")
    started = time.monotonic()
    code, reason = None, None
    with path.open("xb") as log:
        process = subprocess.Popen(command, cwd=root, env=environment,
                                   stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            reason = "online suite deadline exceeded"
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    body = path.read_text(errors="replace")
    summaries = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;", body)
    expected = len(suite["tests"])
    passed = (code == 0 and len(summaries) == 1
              and summaries[0] == ("ok", str(expected), "0", "0"))
    # Preserve individual observed outcomes, but every missing result is a failure.
    observed = dict(re.findall(r"^test (\w+) \.\.\..*?\b(ok|FAILED|ignored)\b", body, re.M))
    for test in suite["tests"]:
        test["status"] = "executed" if passed or observed.get(test["name"]) == "ok" else "failed"
        test["reason"] = None if test["status"] == "executed" else reason or "missing or unsuccessful online result"
    suite.update(command=command, exit_code=code, elapsed_seconds=time.monotonic() - started,
                 status="passed" if passed else "failed", log=path.name)
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--mode", choices=["inventory", "execute"], default="inventory")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--timeout-seconds", type=int, default=180)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.timeout_seconds <= 3600:
        parser.error("timeout must be 1..3600 seconds per suite")
    if args.mode == "execute" and os.name != "posix":
        parser.error("bounded online execution requires POSIX process groups")
    suites = inventory(args.root.resolve())
    args.output.mkdir()
    success = True
    if args.mode == "execute":
        for suite in suites:
            try:
                success = execute(args.root.resolve(), suite, args.cargo,
                                  args.timeout_seconds, args.output) and success
            except OSError as error:
                success = False
                suite["status"], suite["error"] = "failed", str(error)
                for test in suite["tests"]:
                    test.update(status="failed", reason=str(error))
    statuses = [test["status"] for suite in suites for test in suite["tests"]]
    report = dict(schema_version=1, mode=args.mode,
                  recorded_at=dt.datetime.now(dt.timezone.utc).isoformat(),
                  status="not_executed" if args.mode == "inventory" else "passed" if success else "failed",
                  counts={state: statuses.count(state) for state in ["executed", "skipped", "failed"]},
                  suites=suites,
                  scope="Online test outcomes only; passing does not qualify all source fields or production scope.")
    (args.output / "online-tests.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ["status", "counts"]}))
    return 0 if success else 1


if __name__ == "__main__":
    raise SystemExit(main())
