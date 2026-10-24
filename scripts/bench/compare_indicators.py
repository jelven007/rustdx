#!/usr/bin/env python3
"""Compare like-for-like indicator observations; timing is not market acceptance."""
import argparse
import json
import math
from pathlib import Path
import statistics


def load(path):
    report = json.loads(path.read_text())
    if report["schema_version"] != 2 or report["statistic"] != "median" or not report["synthetic"]:
        raise ValueError("requires schema 2 same-host comparison; schema 1 is observation only")
    environment = report["environment"]
    for field in ["os", "arch", "os_version", "hardware", "rustc", "cargo", "build",
                  "input_sha256", "harness_sha256", "samples", "session", "sampling"]:
        if not environment.get(field):
            raise ValueError(f"missing comparison identity: {field}")
    if not isinstance(environment["samples"], int) or not 5 <= environment["samples"] <= 31:
        raise ValueError("invalid sample count")
    entries = {}
    for measurement in report["measurements"]:
        key = (measurement["name"], measurement["rows"], measurement["period"])
        samples = measurement["samples_ns"]
        if (key in entries or len(samples) != environment["samples"]
                or any(isinstance(n, bool) or not isinstance(n, (int, float))
                       or not math.isfinite(n) or n <= 0 for n in samples)
                or measurement["median_ns"] != statistics.median(samples)
                or not measurement["result_hash"]):
            raise ValueError("duplicate key, invalid samples, median or output identity")
        entries[key] = measurement
    if not entries:
        raise ValueError("empty benchmark")
    return report, entries


def compare(before, after, old, new, max_ratio):
    if before["environment"] != after["environment"] or before["lock_sha256"] != after["lock_sha256"]:
        raise ValueError("benchmark host/session/toolchain/input/build/dependencies differ")
    if old.keys() != new.keys():
        raise ValueError("benchmark measurement sets differ")
    rows = []
    for key in sorted(old):
        ratio = new[key]["median_ns"] / old[key]["median_ns"]
        same_output = old[key]["result_hash"] == new[key]["result_hash"]
        rows.append({"name": key[0], "rows": key[1], "period": key[2], "ratio": ratio,
                     "same_output": same_output, "passed": same_output and ratio <= max_ratio})
    return {"passed": all(row["passed"] for row in rows), "max_ratio": max_ratio,
            "scope": "shared batch API; same-host alternating samples; synthetic, not market acceptance",
            "baseline_binary_sha256": before["binary_sha256"],
            "current_binary_sha256": after["binary_sha256"], "measurements": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--current", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--max-ratio", default=1.5, type=float)
    args = parser.parse_args()
    if not 1.0 <= args.max_ratio <= 100:
        parser.error("max-ratio must be finite and in [1,100]")
    before, old = load(args.baseline)
    after, new = load(args.current)
    result = compare(before, after, old, new, args.max_ratio)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    raise SystemExit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
