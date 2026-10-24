#!/usr/bin/env python3
"""Independent stdlib 0fc6 decoder; verify a capture, optionally freeze expectations."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import struct
import zlib


def decode(request, payload):
    if len(request) != 28 or request[:1] != b"\x0c":
        raise ValueError("invalid request")
    if request[6:10] != bytes.fromhex("12001200") or request[10:12] != b"\xc6\x0f":
        raise ValueError("not an enhanced history request")
    date, market = struct.unpack_from("<IH", request, 12)
    code = request[18:24].decode("ascii")
    start, requested = struct.unpack_from("<HH", request, 24)
    day = datetime.datetime.strptime(str(date), "%Y%m%d").date()
    count, base = struct.unpack_from("<Hf", payload)
    if not 1 <= requested <= 1800 or count > requested:
        raise ValueError("invalid count")
    position = 6
    accumulated = 0

    def signed_varint():
        nonlocal position
        first = payload[position]
        position += 1
        value = first & 63
        current = first
        shift = 6
        while current & 128:
            current = payload[position]
            position += 1
            value |= (current & 127) << shift
            shift += 7
            if shift > 34:
                raise ValueError("overlong varint")
        return -value if first & 64 else value

    rows = []
    for index in range(count):
        minutes = struct.unpack_from("<H", payload, position)[0]
        position += 2
        delta, volume, order_count, status, reserved = [signed_varint() for _ in range(5)]
        accumulated += delta
        time = datetime.time(minutes // 60, minutes % 60)
        rows.append({
            "index": index,
            "absolute_index": start + index,
            "time_minutes": minutes,
            "time": time.strftime("%H:%M"),
            "trade_datetime": datetime.datetime.combine(day, time).isoformat(),
            "price": accumulated / 100,
            "price_delta_raw": delta,
            "price_acc_raw": accumulated,
            "volume": volume,
            "order_count": order_count,
            "status_raw": status,
            "reserved_raw": reserved,
        })
    if position != len(payload):
        raise ValueError("trailing payload bytes")
    return {"date": date, "market": market, "code": code, "start": start,
            "request_count": requested, "price_base_raw_f32": base, "records": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture_directory", type=Path)
    parser.add_argument("--freeze", action="store_true",
                        help="create expected-records.json after checking the Rust capture")
    args = parser.parse_args()
    directory = args.capture_directory
    matches = [path for path in directory.glob("*.request.bin")
               if path.read_bytes()[10:12] == b"\xc6\x0f"]
    if len(matches) != 1:
        raise ValueError("expected exactly one 0fc6 response")
    stem = matches[0].name.removesuffix(".request.bin")

    def read(suffix):
        return (directory / f"{stem}.{suffix}").read_bytes()

    request = read("request.bin")
    header = read("header.bin")
    wire = read("wire.bin")
    if len(header) != 16 or header[:4] != bytes.fromhex("b1cb7400") or header[10:12] != b"\xc6\x0f":
        raise ValueError("response header mismatch")
    compressed, expanded = struct.unpack_from("<HH", header, 12)
    if compressed != len(wire):
        raise ValueError("compressed length mismatch")
    payload = wire if compressed == expanded else zlib.decompress(wire)
    if len(payload) != expanded or payload != read("payload.bin"):
        raise ValueError("decoded wire differs from captured payload")
    decoded = decode(request, payload)
    page_path = directory / "page.json"
    if page_path.exists():
        page = json.loads(page_path.read_text())
        for key, value in decoded.items():
            if key == "price_base_raw_f32":
                # Rust serializes the shortest f32 decimal; compare original bits.
                if struct.pack("<f", page[key]) != payload[2:6]:
                    raise ValueError("Rust reference f32 bits differ")
                continue
            if page[key] != value:
                raise ValueError(f"Rust capture differs at {key}")
    expected_path = directory / "expected-records.json"
    if args.freeze:
        if not page_path.exists():
            raise ValueError("freezing requires the original Rust page.json")
        with expected_path.open("x") as output:
            json.dump(decoded["records"], output, ensure_ascii=False, separators=(",", ":"))
            output.write("\n")
    if json.loads(expected_path.read_text()) != decoded["records"]:
        raise ValueError("frozen field expectations differ")
    statuses = {}
    for row in decoded["records"]:
        status = str(row["status_raw"])
        statuses[status] = statuses.get(status, 0) + 1
    summary = {key: value for key, value in decoded.items() if key != "records"}
    summary.update({
        "record_count": len(decoded["records"]),
        "all_record_fields_match": True,
        "status_counts": statuses,
        "sha256": {suffix: hashlib.sha256(read(suffix)).hexdigest()
                   for suffix in ["request.bin", "header.bin", "wire.bin", "payload.bin"]},
        "expected_sha256": hashlib.sha256(expected_path.read_bytes()).hexdigest(),
    })
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
