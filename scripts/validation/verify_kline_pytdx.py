#!/usr/bin/env python3
"""Create independent expectations using a pinned pytdx implementation.

Requires six and an extracted pytdx checkout (test tooling only).
Download that checkout separately; this script never fetches executable code.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import sys

REFERENCE_COMMIT = "2857fdad08534533610bd2aeca387f760c4baa42"
SOURCE_HASHES = {
    "pytdx/__init__.py": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "pytdx/parser/__init__.py": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "pytdx/helper.py": "1574270eee7da0313dcc753e2fa50c61fbf6305541967d974944bc28effa2772",
    "pytdx/parser/get_security_bars.py": "94a763fdc856d09fdcbb75c3498ea0e00fe0ae1ea3994fa8d8875c442713be56",
    "pytdx/parser/get_index_bars.py": "3dbe7d7ecaf36b0f47f88499b50dc45798d72fe4d3b59be9df78b8a9dbee46d2",
    "pytdx/parser/base.py": "365821fc502097740987075e06d7d4ae3417f503776d2588af36e85a6b643148",
    "pytdx/log.py": "c566fe3b9f15406e924c45a4188c3836b3f46e7ada3f4a1e8515117079dbd7d8",
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pytdx-source", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    args = parser.parse_args()
    sources = {}
    for name, expected in SOURCE_HASHES.items():
        actual = sha256((args.pytdx_source / name).read_bytes())
        if actual != expected:
            raise ValueError(f"unverified reference source: {name}: {actual}")
        sources[name] = actual
    sys.path.insert(0, str(args.pytdx_source.resolve()))
    from pytdx.parser.get_security_bars import GetSecurityBarsCmd
    from pytdx.parser.get_index_bars import GetIndexBarsCmd

    fields = ["year", "month", "day", "hour", "minute", "open", "close",
              "high", "low", "vol", "amount", "up_count", "down_count"]
    cases = []
    for name, market, code, category in [
        ("stock-day", 0, "000001", 9),
        ("index-day", 1, "000001", 9),
        ("stock-minute", 0, "000001", 0),
    ]:
        decoder = (GetIndexBarsCmd if name == "index-day" else GetSecurityBarsCmd)(None)
        decoder.setParams(category, market, code, 0, 3)
        request_path = args.fixtures / f"{name}.request.hex"
        response_path = args.fixtures / f"{name}.response.hex"
        request = bytes.fromhex(request_path.read_text())
        response = bytes.fromhex(response_path.read_text())
        if decoder.send_pkg != request:
            raise ValueError(f"{name}: request differs from pytdx struct.pack")
        rows = decoder.parseResponse(response)
        if len(rows) != 3:
            raise ValueError(f"{name}: expected 3 rows")
        expected_path = args.fixtures / f"{name}.expected.csv"
        with expected_path.open("w", newline="") as output:
            writer = csv.DictWriter(
                output, fieldnames=fields, extrasaction="ignore", lineterminator="\n")
            writer.writeheader()
            writer.writerows(sorted(rows, key=lambda row: row["datetime"]))
        cases.append({
            "case": name, "market": market, "code": code, "category": category,
            "rows": len(rows), "request_sha256": sha256(request),
            "response_sha256": sha256(response),
            "expected_csv_sha256": sha256(expected_path.read_bytes()),
        })
        print(f"{name}: independent pytdx request and {len(rows)} decoded rows")
    manifest = {
        "captured_date": "2026-10-23",
        "reference": "https://github.com/rainx/pytdx/tree/" + REFERENCE_COMMIT,
        "reference_sources_sha256": sources,
        "scope": "3 recent stock day, index day and stock 5-minute bars each; "
                 "raw TDX bodies, independent pytdx expectations, not full market",
        "cases": cases,
    }
    (args.fixtures / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
