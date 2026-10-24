#!/usr/bin/env python3
"""Compare frozen 052d category 9/4 and 052b probes without altering history."""
import argparse
import json
from pathlib import Path
import struct

from verify_tcp_daily_shares import decode_daily, official, sha


def kline(body):
    count, = struct.unpack_from("<H", body)
    position = 2
    rows = []
    for _ in range(count):
        date, = struct.unpack_from("<I", body, position)
        position += 4
        for _ in range(4):
            byte = body[position]
            position += 1
            while byte & 128:
                byte = body[position]
                position += 1
        volume_raw, amount_raw = struct.unpack_from("<II", body, position)
        position += 8
        volume = struct.unpack("<f", struct.pack("<I", volume_raw))[0]
        rows.append(dict(date=date, volume_raw=volume_raw, volume=volume,
                         amount_raw=amount_raw))
    assert position == len(body)
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--official-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    results = []
    for probe in sorted(args.evidence_root.glob("probe-*")):
        values = {}
        for path in sorted(probe.glob("frame-*.request.bin")):
            req = path.read_bytes()
            command, = struct.unpack_from("<H", req, 10)
            if command not in (0x052d, 0x052b):
                continue
            market, = struct.unpack_from("<H", req, 12)
            code = ("sh" if market == 1 else "sz") + req[14:20].decode()
            payload_path = path.with_name(path.name.replace("request.bin", "payload.bin"))
            payload = payload_path.read_bytes()
            category = struct.unpack_from("<H", req, 20)[0] if command == 0x052d else None
            decoded = kline(payload) if command == 0x052d else decode_daily(payload)
            expected, _ = official(args.official_root / f"{code}.day")
            for row in decoded:
                date = row["date"]
                if not 20261019 <= date <= 20261022:
                    continue
                result = values.setdefault((code, date), dict(
                    endpoint=probe.name.removeprefix("probe-") + ":7709",
                    code=code, date=date, official_shares=expected[date]["vol"]))
                shares = row["vol"] if command == 0x052b else row["volume"] * (100 if category == 9 else 1)
                result["052b" if command == 0x052b else f"052d_category{category}"] = dict(
                    shares=shares, difference=shares - expected[date]["vol"],
                    volume_raw=row.get("volume_raw", row.get("vol")),
                    payload_sha256=sha(payload), source=str(payload_path))
        assert len(values) == 12
        for result in values.values():
            assert result["052b"]["difference"] == 0
            assert all(key in result for key in ["052d_category9", "052d_category4", "052b"])
        results.extend(values.values())
    assert len(results) == 24
    diffs = [r["052d_category9"]["difference"] for r in results]
    report = dict(schema_version=1, status="passed", selected_rows=24,
                  category9_difference_range=[min(diffs), max(diffs)],
                  category4_mismatches=sum(r["052d_category4"]["difference"] != 0 for r in results),
                  integer_052b_mismatches=0,
                  conclusion="052d source precision cannot reconstruct exact shares; use 052b integer field.",
                  results=results)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "results"}))


if __name__ == "__main__":
    main()
