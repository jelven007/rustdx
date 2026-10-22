#!/usr/bin/env python3
"""macOS release-binary comparison; synthetic data is not market acceptance."""
import argparse
import csv
import datetime
import hashlib
import json
import pathlib
import re
import struct
import subprocess
import tempfile
import time


def evidence(path):
    hashes, rows = {}, 0
    with path.open(newline="") as stream:
        for row in csv.DictReader(stream):
            digest = hashes.setdefault(row["code"], hashlib.sha256())
            digest.update(json.dumps(row, sort_keys=True, separators=(",", ":")).encode())
            digest.update(b"\n")
            rows += 1
    return rows, {code: digest.hexdigest() for code, digest in hashes.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=pathlib.Path)
    parser.add_argument("--current", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--sizes", nargs="+", type=int, default=[20_000, 200_000])
    args = parser.parse_args()
    binaries = {"baseline": args.baseline.resolve(), "current": args.current.resolve()}
    binary_hashes = {
        name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in binaries.items()
    }
    if binary_hashes["baseline"] == binary_hashes["current"]:
        raise RuntimeError("baseline/current binaries are identical; rebuild with isolated caches")
    results = []
    with tempfile.TemporaryDirectory(prefix="rustdx-memory-") as temporary:
        directory = pathlib.Path(temporary)
        source = directory / "source"
        source.mkdir()
        for size in args.sizes:
            data = bytearray()
            start = datetime.date(1900, 1, 1)
            for offset in range(size):
                date = start + datetime.timedelta(days=offset)
                data.extend(struct.pack("<5If2I", int(date.strftime("%Y%m%d")),
                                        1000, 1100, 900, 1050, 1000.0, 100, 0))
            for code in range(1, 5):
                (source / f"sz00000{code}.day").write_bytes(data)
            expected, current_sha = None, None
            for version, binary in binaries.items():
                for workers in [1, 4]:
                    output = directory / "bars.csv"
                    began = time.monotonic()
                    result = subprocess.run(
                        ["/usr/bin/time", "-l", str(binary), "--timeout", "300", "day",
                         str(source), "-o", str(output), "--report", str(directory / "run.json"),
                         "--workers", str(workers)],
                        cwd=directory, capture_output=True, text=True, timeout=310,
                    )
                    if result.returncode:
                        raise RuntimeError(result.stderr)
                    elapsed = time.monotonic() - began
                    peak = int(re.search(r"(\d+)\s+maximum resident set size", result.stderr)[1])
                    rows, by_code = evidence(output)
                    if rows != size * 4:
                        raise RuntimeError("row count changed")
                    if expected is None:
                        expected = by_code
                    elif by_code != expected:
                        raise RuntimeError("independent per-code CSV evidence changed")
                    sha = hashlib.sha256(output.read_bytes()).hexdigest()
                    if version == "current":
                        if current_sha is None:
                            current_sha = sha
                        elif sha != current_sha:
                            raise RuntimeError("current worker counts changed output bytes")
                    sample = {
                        "version": version, "workers": workers, "source_rows_per_file": size,
                        "source_files": 4, "source_bytes": len(data) * 4, "output_rows": rows,
                        "peak_rss_bytes": peak, "wall_seconds": round(elapsed, 3),
                        "output_sha256": sha, "per_code_sha256": by_code,
                    }
                    results.append(sample)
                    print(json.dumps({key: value for key, value in sample.items()
                                      if key != "per_code_sha256"}), flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({
        "environment": "macOS arm64; Rust 1.89; release opt-level=3, lto=true",
        "scope": "synthetic ascending day files; OHLC/volume fixed; no gbbq/previous/database",
        "baseline_commit": "0892427", "binary_sha256": binary_hashes, "samples": results,
    }, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
