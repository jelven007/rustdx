#!/usr/bin/env python3
"""Compare isolated import/readback runs; never accepts an existing table name.

Requires explicit CLICKHOUSE_URL/MONGODB_URI and MongoDB tools in PATH.
Writes the raw manifest, process timings and CLI logs into an owned temp directory.
"""
import argparse
import base64
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import tempfile
import time
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rows", type=int, default=65536)
    parser.add_argument("--symbols", type=int, default=128)
    parser.add_argument("--repeat", type=int, default=2)
    parser.add_argument("--batch-rows", type=int)
    parser.add_argument("--instrument-processes", action="store_true",
                        help="adds Python wrappers; use separate uninstrumented runs for elapsed comparison")
    parser.add_argument("--backend", choices=["clickhouse", "mongodb", "both"], default="both")
    args = parser.parse_args()
    if not 1 <= args.symbols <= 10000 or args.rows < args.symbols or args.repeat < 1:
        parser.error("invalid rows/symbols/repeat")
    binary = args.binary.resolve()
    backends = ["clickhouse", "mongodb"] if args.backend == "both" else [args.backend]
    for backend, key in [("clickhouse", "CLICKHOUSE_URL"), ("mongodb", "MONGODB_URI")]:
        if backend in backends and not os.environ.get(key):
            parser.error(f"explicit {key} required")
    scratch = Path(tempfile.mkdtemp(prefix="rustdx-readback-bench-"))
    database = "rustdx_readback_bench_" + uuid.uuid4().hex
    table = database + ".bars"
    source = scratch / "input"
    source.mkdir()
    shim = scratch / "tools"
    shim.mkdir()
    tools = {}
    if "mongodb" in backends:
        for name in ["mongoimport", "mongoexport", "mongosh"]:
            real = shutil.which(name)
            if not real:
                if name == "mongoexport":
                    continue  # Needed only by the pre-optimization binary.
                parser.error(f"{name} missing")
            tools[name] = real
            wrapper = shim / name
            wrapper.write_text(
                "#!/usr/bin/env python3\n"
                "import json,os,subprocess,sys,time\n"
                "started=time.perf_counter()\n"
                f"result=subprocess.run([{real!r},*sys.argv[1:]])\n"
                "with open(os.environ['RUSTDX_BENCH_PROCESS_LOG'],'a') as log:\n"
                f" log.write(json.dumps({{'tool':{name!r},'seconds':time.perf_counter()-started,'exit':result.returncode}})+'\\n')\n"
                "sys.exit(result.returncode)\n"
            )
            wrapper.chmod(0o755)
    # Synthetic daily records, fixed input across binaries/runs, no external download.
    digest = hashlib.sha256()
    for index in range(args.symbols):
        count = args.rows // args.symbols + (index < args.rows % args.symbols)
        date = datetime.date(2024, 1, 2)
        records = bytearray()
        for _ in range(count):
            while date.weekday() >= 5:
                date += datetime.timedelta(days=1)
            records.extend(struct.pack("<5If2I", int(date.strftime("%Y%m%d")),
                                       1000, 1100, 900, 1050, 1234.5, 12345, 0))
            date += datetime.timedelta(days=1)
        name = f"sh{600000+index}.day"
        (source / name).write_bytes(records)
        digest.update(name.encode())
        digest.update(records)

    def ch(sql):
        request = urllib.request.Request(os.environ["CLICKHOUSE_URL"], sql.encode())
        if os.environ.get("CLICKHOUSE_USER"):
            auth = os.environ["CLICKHOUSE_USER"] + ":" + os.environ.get("CLICKHOUSE_PASSWORD", "")
            request.add_header("Authorization", "Basic " + base64.b64encode(auth.encode()).decode())
        with urllib.request.urlopen(request, timeout=60) as response:
            return response.read().decode().strip()

    def mongo(script):
        return subprocess.check_output(
            [tools["mongosh"], os.environ["MONGODB_URI"], "--quiet", "--eval", script],
            text=True, timeout=60,
        ).strip()

    result = {
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "input_sha256": digest.hexdigest(), "rows": args.rows, "symbols": args.symbols,
        "batch_rows_env": args.batch_rows, "platform": platform.platform(),
        "process_wrappers": args.instrument_processes,
        "scratch": str(scratch), "database": database, "versions": {}, "runs": [],
        "scope": "synthetic_no_factors_end_to_end_and_child_process_wall_time",
        "rss_scope": "/usr/bin/time resource-usage maximum; not simultaneous process-tree RSS sum",
    }
    for backend in backends:
        result["versions"][backend] = ch("SELECT version()") if backend == "clickhouse" else mongo("print(db.version())")

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")

    save()
    try:
        for backend in backends:
            for run in range(args.repeat):
                work = scratch / f"{backend}-{run}"
                work.mkdir()
                env = os.environ.copy()
                if args.instrument_processes:
                    env["PATH"] = str(shim) + os.pathsep + env["PATH"]
                env["RUSTDX_BENCH_PROCESS_LOG"] = str(work / "processes.jsonl")
                if args.batch_rows is not None:
                    env["RUSTDX_VERIFY_BATCH_ROWS"] = str(args.batch_rows)
                else:
                    env.pop("RUSTDX_VERIFY_BATCH_ROWS", None)
                cmd = [str(binary), "--timeout", "3600", "day", str(source), "-o", backend,
                       "-t", table, "--workers", "4", "--keep-csv", "--report", "run.json"]
                time_args = ["-l"] if platform.system() == "Darwin" else ["-v"]
                start = time.perf_counter()
                with (work / "cli.log").open("w") as log:
                    completed = subprocess.run(["/usr/bin/time", *time_args, *cmd], cwd=work,
                                               env=env, stdout=log, stderr=log, timeout=3660)
                elapsed = time.perf_counter() - start
                manifest = json.loads((work / "run.json").read_text())
                processes = [json.loads(line) for line in (work / "processes.jsonl").read_text().splitlines()] \
                    if (work / "processes.jsonl").exists() else []
                log_text = (work / "cli.log").read_text()
                if platform.system() == "Darwin":
                    rss_match = re.search(r"(\d+)\s+maximum resident set size", log_text)
                    rss_scale = 1
                else:
                    rss_match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", log_text)
                    rss_scale = 1024
                entry = {
                    "backend": backend, "run": run, "seconds": elapsed, "exit": completed.returncode,
                    "max_rss_bytes": int(rss_match[1]) * rss_scale if rss_match else None,
                    "processes": processes, "manifest": manifest,
                    "csv_sha256": hashlib.sha256((work / f"{backend}.csv").read_bytes()).hexdigest()
                    if (work / f"{backend}.csv").exists() else None,
                }
                result["runs"].append(entry)
                save()
                assert completed.returncode == 0, str(work / "cli.log")
                assert manifest["database_status"] == "verified"
                assert manifest["database_verification"]["rows"] == args.rows
                print(backend, run, round(elapsed, 3), "seconds",
                      manifest["database_verification"]["batches"], "batches", flush=True)
    finally:
        if "clickhouse" in backends:
            ch(f"DROP DATABASE IF EXISTS {database}")
        if "mongodb" in backends:
            mongo(f'db.getSiblingDB("{database}").dropDatabase()')
    print(args.output, flush=True)


if __name__ == "__main__":
    main()
