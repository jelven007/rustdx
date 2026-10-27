#!/usr/bin/env python3
"""Measure fixed genuine .day bytes at 1/4/8 workers; never a market qualification."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output-directory", type=Path, required=True)
    args = parser.parse_args()
    out, binary = args.output_directory.resolve(), args.binary.resolve(strict=True)
    if out.exists():
        parser.error("output directory must be new")
    inputs = sorted(args.source.resolve(strict=True).glob("*.day"))
    if not inputs:
        parser.error("explicit real .day inputs required")
    out.mkdir(mode=0o700)
    source = out / "source"
    source.mkdir()
    identities = []
    for path in inputs:
        if path.is_symlink() or not path.is_file():
            raise ValueError("regular source files required")
        destination = source / path.name
        shutil.copyfile(path, destination)
        identities.append({"source": str(path), "name": path.name,
                           "bytes": path.stat().st_size, "sha256": digest(destination)})
    results, expected = [], None
    for round_number in range(5):
        for workers in [1, 4, 8]:
            prefix = out / f"round-{round_number}-workers-{workers}"
            csv_path, report = Path(str(prefix) + ".csv"), Path(str(prefix) + ".json")
            command = [str(binary), "--no-tcp-startup", "--timeout", "300", "day",
                       str(source), "-o", str(csv_path), "--report", str(report),
                       "--workers", str(workers)]
            if platform.system() == "Darwin":
                command = ["/usr/bin/time", "-l"] + command
            elif platform.system() == "Linux" and Path("/usr/bin/time").is_file():
                command = ["/usr/bin/time", "-v"] + command
            began = time.monotonic()
            completed = subprocess.run(command, capture_output=True, timeout=310)
            elapsed = time.monotonic() - began
            Path(str(prefix) + ".stdout").write_bytes(completed.stdout)
            Path(str(prefix) + ".stderr").write_bytes(completed.stderr)
            if completed.returncode:
                raise RuntimeError(f"replay failed: {prefix}")
            sha = digest(csv_path)
            if expected is None:
                expected = sha
            if sha != expected:
                raise ValueError("1/4/8-worker output bytes differ")
            with csv_path.open(newline="") as stream:
                rows = sum(1 for _ in csv.DictReader(stream))
            stderr = completed.stderr.decode()
            pattern = (r"(\d+)\s+maximum resident set size" if platform.system() == "Darwin"
                       else r"Maximum resident set size \(kbytes\):\s*(\d+)")
            match = re.search(pattern, stderr)
            rss = int(match[1]) * (1 if platform.system() == "Darwin" else 1024) if match else None
            manifest = json.loads(report.read_text())
            results.append({"round": round_number, "workers": workers, "seconds": elapsed,
                            "rss_bytes": rss, "rows": rows, "sha256": sha, "argv": command,
                            "observations": manifest.get("observations")})
    summary = {str(workers): {
        "median_seconds": statistics.median(s["seconds"] for s in results if s["workers"] == workers),
        "maximum_process_rss_bytes": max((s["rss_bytes"] for s in results
                                         if s["workers"] == workers and s["rss_bytes"] is not None), default=None)
    } for workers in [1, 4, 8]}
    (out / "result.json").write_text(json.dumps({
        "schema_version": 1, "status": "passed", "kind": "fixed_real_day_replay",
        "platform": platform.platform(), "binary_sha256": digest(binary),
        "inputs": identities, "samples": results, "summary": summary,
        "scope": "Frozen official file replay, raw without factors or database. Process RSS only; no production throughput claim."
    }, indent=2) + "\n")
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
