#!/usr/bin/env python3
"""Independently decode a deterministic real ZIP subset and verify repeated imports.

Only creates a random private database. Requires --zip, explicit service URLs and
MongoDB tools. Evidence identifies every selected member; this is not full-market QA.
"""
import argparse
import csv
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request
import uuid
import zipfile


def digest(path):
    hash_ = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            hash_.update(block)
    return hash_.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--zip", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--symbols", type=int, default=128)
    args = parser.parse_args()
    if not 1 <= args.symbols <= 875:
        parser.error("symbols must be 1..875")
    if not os.environ.get("CLICKHOUSE_URL") or not os.environ.get("MONGODB_URI"):
        parser.error("explicit CLICKHOUSE_URL and MONGODB_URI required")
    binary = args.binary.resolve()
    scratch = Path(tempfile.mkdtemp(prefix="rustdx-real-day-"))
    source = scratch / "source"
    source.mkdir()
    evidence = {
        "platform": platform.platform(), "binary_sha256": digest(binary),
        "zip_path": str(args.zip.resolve()), "zip_sha256": digest(args.zip),
        "scope": "first_sorted_sh600_members; real_unadjusted_history_subset; not_full_market",
        "scratch": str(scratch), "sources": [], "runs": [],
    }
    with zipfile.ZipFile(args.zip) as archive:
        members = sorted(
            info for info in archive.namelist()
            if info.replace("\\", "/").split("/")[-1].startswith("sh600")
            and info.endswith(".day")
        )[:args.symbols]
        assert len(members) == args.symbols
        for name in members:
            content = archive.read(name)  # ZIP CRC is checked by zipfile.
            assert len(content) % 32 == 0
            path = source / name.replace("\\", "/").split("/")[-1]
            path.write_bytes(content)
            evidence["sources"].append({
                "member": name, "sha256": digest(path), "rows": len(content) // 32,
            })
    rows = sum(item["rows"] for item in evidence["sources"])
    evidence["rows"] = rows
    database = "rustdx_real_acceptance_" + uuid.uuid4().hex
    evidence["database"] = database

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, indent=2) + "\n")

    def ch(sql):
        url = os.environ["CLICKHOUSE_URL"]
        url += ("&" if "?" in url else "?") + "output_format_parallel_formatting=0"
        request = urllib.request.Request(url, sql.encode())
        if os.environ.get("CLICKHOUSE_USER"):
            request.add_header("X-ClickHouse-User", os.environ["CLICKHOUSE_USER"])
        if os.environ.get("CLICKHOUSE_PASSWORD"):
            request.add_header("X-ClickHouse-Key", os.environ["CLICKHOUSE_PASSWORD"])
        with urllib.request.urlopen(request, timeout=60) as response:
            return response.read().decode().strip()

    def mongo(script):
        return subprocess.check_output(
            ["mongosh", os.environ["MONGODB_URI"], "--quiet", "--eval", script],
            text=True, timeout=60,
        ).strip()

    def run(output, workers, backend=None):
        work = scratch / output
        work.mkdir()
        command = [str(binary), "--no-tcp-startup", "--timeout", "1800", "day", str(source),
                   "--workers", str(workers), "--report", "run.json"]
        if backend:
            command += ["-o", backend, "-t", database + ".bars", "--keep-csv"]
            csv_path = work / (backend + ".csv")
        else:
            command += ["-o", "bars.csv"]
            csv_path = work / "bars.csv"
        started = time.perf_counter()
        with (work / "cli.log").open("w") as log:
            completed = subprocess.run(command, cwd=work, stdout=log, stderr=log, timeout=1830)
        manifest = json.loads((work / "run.json").read_text())
        record = {
            "name": output, "workers": workers, "exit": completed.returncode,
            "seconds": time.perf_counter() - started,
            "csv_sha256": digest(csv_path) if csv_path.exists() else None,
            "status": manifest["status"], "artifact": manifest["artifact"],
            "observations": manifest["observations"], "freshness": manifest["freshness"],
            "database_status": manifest["database_status"],
            "database_verification": manifest["database_verification"],
        }
        evidence["runs"].append(record)
        save()
        assert completed.returncode == 0, str(work / "cli.log")
        assert manifest["artifact"]["rows"] == rows
        assert manifest["observations"]["temporary_current_bytes"] == 0
        if backend:
            assert manifest["database_status"] == "verified"
            assert manifest["database_verification"]["rows"] == rows
        return csv_path

    save()
    baseline = run("csv-one-worker", 1)
    # Independent Python struct decoding uses no Rust decoder or CSV fixture.
    count = 0
    with baseline.open(newline="") as file:
        reader = csv.DictReader(file)
        assert reader.fieldnames == ["date", "code", "open", "high", "low", "close", "amount", "vol"]
        for path in sorted(source.glob("*.day")):
            for date, op, hi, lo, cl, amount, volume, _reserved in struct.iter_unpack("<5If2I", path.read_bytes()):
                actual = next(reader)
                expected_date = datetime.datetime.strptime(str(date), "%Y%m%d").date().isoformat()
                assert actual["date"] == expected_date and actual["code"] == path.stem
                for field, value in zip(["open", "high", "low", "close", "amount", "vol"],
                                        [op / 100, hi / 100, lo / 100, cl / 100, float(amount), volume / 100]):
                    assert float(actual[field]) == value, (path.name, expected_date, field, actual[field], value)
                count += 1
        assert next(reader, None) is None
    assert count == rows
    evidence["independent_decoded_rows"] = count
    baseline_sha = digest(baseline)
    assert digest(run("csv-four-workers", 4)) == baseline_sha
    evidence["versions"] = {"clickhouse": ch("SELECT version()"), "mongodb": mongo("print(db.version())")}
    save()
    try:
        for backend in ["clickhouse", "mongodb"]:
            for repeat in range(2):
                assert digest(run(f"{backend}-{repeat}", 4, backend)) == baseline_sha
            if backend == "clickhouse":
                assert ch(f"SELECT count() FROM {database}.bars FINAL") == str(rows)
                ch(f"OPTIMIZE TABLE {database}.bars FINAL")
                assert ch(f"SELECT count() FROM {database}.bars") == str(rows)
            else:
                assert mongo(f'print(db.getSiblingDB("{database}").bars.countDocuments())') == str(rows)
        evidence["completed"] = True
    finally:
        failed = sys.exc_info()[0] is not None
        cleanup_errors = []
        for backend, cleanup in [
            ("clickhouse", lambda: ch(f"DROP DATABASE IF EXISTS {database}")),
            ("mongodb", lambda: mongo(f'db.getSiblingDB("{database}").dropDatabase()')),
        ]:
            try:
                cleanup()
            except Exception as error:
                cleanup_errors.append(f"{backend}: {error}")
        evidence["cleanup_errors"] = cleanup_errors
        evidence["private_database_removed"] = not cleanup_errors
        save()
        if cleanup_errors and not failed:
            raise RuntimeError("; ".join(cleanup_errors))
    print(args.output, rows, "independently decoded real rows; all repeated imports verified", flush=True)


if __name__ == "__main__":
    main()
