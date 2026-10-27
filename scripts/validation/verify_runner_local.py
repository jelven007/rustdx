#!/usr/bin/env python3
"""Real binary/file replay with synthetic admission facts; isolated engineering drill."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import platform
import struct
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--output-directory", type=Path, required=True)
    args = parser.parse_args()
    out = args.output_directory.absolute()
    if out.exists() or not out.parent.is_dir():
        parser.error("use a new output directory with an existing parent")
    out.mkdir(mode=0o700)
    runner, cli = args.runner.resolve(strict=True), args.cli.resolve(strict=True)
    source, facts, live = out / "source", out / "facts", out / "live"
    source.mkdir()
    facts.mkdir()
    fake = b"SYNTHETIC ENGINEERING ADMISSION; NOT INDEPENDENT MARKET FACT"
    fact_digest = hashlib.sha256(fake).hexdigest()
    (facts / fact_digest).write_bytes(fake)
    codes = ["sh600000", "sh600519", "sz000001"]
    expected, inputs = {}, []
    for code in codes:
        original = ROOT / f"crates/rustdx-core/tests/fixtures/daily-shares/115.238.56.198-{code}.official.day"
        destination = source / (code + ".day")
        destination.write_bytes(original.read_bytes())
        inputs.append({"path": str(original.relative_to(ROOT)), "sha256": sha(original)})
        for date, op, hi, lo, close, amount, volume, _ in struct.iter_unpack("<5If2I", original.read_bytes()):
            day = f"{date//10000:04d}-{date//100%100:02d}-{date%100:02d}"
            expected[(day, code[:2], code[2:])] = [op/100, hi/100, lo/100, close/100, float(amount), volume]
    def write(name, value):
        path = out / name
        path.write_text(json.dumps(value, indent=2) + "\n")
        return str(path)
    config = {
        "schema_version": 1, "root": str(live), "cli": str(cli), "no_tcp_startup": True,
        "dataset_id": "isolated_real_replay_synthetic_facts",
        "contract_version": "a-share/day/raw/cny/shares/v1",
        "policy_revision": 1, "source_revision": 1, "source_as_of": "2026-10-22",
        "planned_date": "2026-10-22", "from": "2026-10-19", "through": "2026-10-20",
        "day_directories": [str(source)], "facts_directory": str(facts),
        "calendar": write("calendar.json", {"version": "synthetic-calendar", "source": "engineering-only",
                                          "start": "2026-10-19", "end": "2026-10-22", "holidays": []}),
        "calendar_source_sha256": fact_digest,
        "security_history": write("history.json", {"version": "synthetic-states", "checked_through": "2026-10-22",
            "intervals": [{"code": code, "start": "2026-10-19", "end": "2026-10-22", "board": "Main",
                          "st": False, "stage": "Seasoned", "state": "Trading",
                          "announcement": {"url": "https://example.org/engineering-only", "sha256": fact_digest,
                                           "published": "2026-10-19", "checked": "2026-10-22"}} for code in codes]}),
        "universe": write("universe.json", {"version": "synthetic-universe", "checked_through": "2026-10-22",
            "source_sha256": fact_digest, "members": [{"code": code, "from": "2026-10-19", "through": "2026-10-22"} for code in codes]}),
        "timeout_seconds": 60, "publish_reserve_seconds": 10, "cancel_grace_seconds": 2,
        "workers": 4, "max_source_bytes": 32*1024**2, "max_workspace_bytes": 256*1024**2,
        "max_rows": 1000, "min_free_bytes": 1024**2, "database_targets": [],
    }
    result = {"schema_version": 1, "status": "running", "platform": platform.platform(),
              "binary_sha256": {"rustdx": sha(cli), "rustdx-runner": sha(runner)}, "inputs": inputs,
              "synthetic_facts": True, "production_rpo_rto_verified": False,
              "scope": "Real official 12-row replay, adjacent windows and isolated local restore. Synthetic facts; no market, Linux or database qualification.",
              "commands": []}
    def run(action, *flags, success=True):
        command = [str(runner), "--no-tcp-startup", action, *map(str, flags)]
        began = time.monotonic()
        process = subprocess.run(command, capture_output=True, timeout=90)
        number = len(result["commands"])
        (out / f"{number:02d}.stdout").write_bytes(process.stdout)
        (out / f"{number:02d}.stderr").write_bytes(process.stderr)
        result["commands"].append({"argv": command, "exit_code": process.returncode,
                                   "seconds": time.monotonic()-began})
        if (process.returncode == 0) != success:
            raise ValueError(f"unexpected {action} outcome; see {out}/{number:02d}.stderr")
        return process.stdout
    try:
        for number, start, end in [(1, "2026-10-19", "2026-10-20"), (2, "2026-10-21", "2026-10-22")]:
            config.update({"from": start, "through": end})
            path = write(f"job-{number}.json", config)
            plan = json.loads(run("plan", "--config", path))
            if plan["expected_keys"]["total"] != 6:
                raise ValueError("window plan differs from full three-security universe")
            run("run", "--config", path)
        config.update({"from": "2026-10-19", "through": "2026-10-22", "source_revision": 2})
        path = write("job-revision.json", config)
        run("run", "--config", path)
        scope = ["--dataset", config["dataset_id"], "--from", "2026-10-19", "--through", "2026-10-22",
                 "--version", 3, "--max-rows", 1000, "--max-bytes", 256*1024**2]
        before = out / "before.csv"
        run("export", "--root", live, *scope, "--output", before)
        actual = {}
        with before.open(newline="") as stream:
            for row in csv.DictReader(stream):
                actual[(row["date"], row["market"], row["code"])] = [float(row[field]) for field in
                    ["open", "high", "low", "close", "amount_cny", "volume_shares"]]
        if actual != expected:
            raise ValueError("independent Python day fields differ")
        metrics = run("business-metrics", "--config", path)
        if (b"rustdx_business_missing_keys 0\n" not in metrics
                or b"rustdx_business_job_released 1\n" not in metrics):
            raise ValueError("healthy scoped business metrics differ")
        config["source_revision"] = 3
        config["max_workspace_bytes"] = config["max_source_bytes"]
        config["min_free_bytes"] = 2**63
        run("plan", "--config", write("job-disk-rejected.json", config), success=False)
        after_reject = out / "after-reject.csv"
        run("export", "--root", live, *scope, "--output", after_reject)
        if sha(before) != sha(after_reject):
            raise ValueError("disk rejection changed fixed version")
        backup, restored = out / "backup", out / "restored"
        run("backup", "--root", live, "--destination", backup, "--max-bytes", 256*1024**2)
        began = time.monotonic()
        run("restore", "--root", backup, "--destination", restored, "--max-bytes", 256*1024**2)
        after = out / "after.csv"
        run("export", "--root", restored, *scope, "--output", after)
        if sha(before) != sha(after):
            raise ValueError("restored full keys/fields differ")
        result.update({"status": "passed", "rows": len(expected), "catalog_version": 3,
                       "csv_sha256": sha(before), "isolated_restore_to_verified_seconds": time.monotonic()-began})
    except Exception as error:
        result.update({"status": "failed", "error": str(error)})
        raise
    finally:
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ["status", "rows", "catalog_version"]}))


if __name__ == "__main__":
    main()
