#!/usr/bin/env python3
"""Independent 052b frame, receipt, CSV and official .day comparison (stdlib)."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import struct
import zlib

CONTRACT = "a-share/tcp-052b/day/raw/cny/integer-shares/v2"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def decode_daily(body):
    count, = struct.unpack_from("<H", body)
    position = 2
    base = 0
    rows = []

    def integer():
        nonlocal position
        first = body[position]
        position += 1
        value = first & 63
        shift = 6
        byte = first
        while byte & 128:
            byte = body[position]
            position += 1
            value |= (byte & 127) << shift
            shift += 7
            assert shift <= 69
        return -value if first & 64 else value

    for _ in range(count):
        date, = struct.unpack_from("<I", body, position)
        position += 4
        op = base + integer()
        close, high, low = (op + integer() for _ in range(3))
        volume = integer()
        amount, raw_amount, suffix = (
            struct.unpack_from("<f", body, position)[0],
            struct.unpack_from("<I", body, position)[0],
            body[position + 4:position + 8].hex(),
        )
        position += 8
        assert 0 <= volume <= 0xffffffff
        assert 0 <= low <= min(op, close) <= max(op, close) <= high
        assert amount >= 0 and amount < float("inf")
        rows.append(dict(date=date, open=op / 1000, high=high / 1000,
                         low=low / 1000, close=close / 1000, amount=amount,
                         vol=volume, amount_raw=raw_amount, suffix=suffix))
        base = close
    assert position == len(body)
    assert all(a["date"] < b["date"] for a, b in zip(rows, rows[1:]))
    return rows


def official(path):
    data = path.read_bytes()
    assert len(data) % 32 == 0
    rows = {}
    for date, op, high, low, close, amount, volume, _ in struct.iter_unpack("<IIIIIfII", data):
        rows[date] = dict(date=date, open=op / 100, high=high / 100, low=low / 100,
                          close=close / 100, amount=amount, vol=volume)
    return rows, dict(path=str(path), sha256=sha(data), bytes=len(data))


def verify(candidate, official_root):
    request = json.loads((candidate / "tcp-request.json").read_bytes())
    assert request["schema_version"] == 2 and request["source_contract"] == CONTRACT
    begin = int(request["from"].replace("-", ""))
    end = int(request["through"].replace("-", ""))
    csv_rows = {}
    for row in csv.DictReader((candidate / "collected.csv").open()):
        csv_rows[(row["code"], int(row["date"].replace("-", "")))] = row
    comparisons, sources = [], []
    for code in request["codes"]:
        day, source = official(official_root / f"{code}.day")
        sources.append(source)
        root = candidate / "raw" / code
        receipt = json.loads((root / "receipt.json").read_bytes())
        inventory = json.loads((root / "wire/inventory.json").read_bytes())
        assert receipt["schema_version"] == 2 and receipt["source_contract"] == CONTRACT
        assert receipt["endpoint"] == request["source"]["endpoint"]
        rows, pages, pending = {}, [], None
        for item in inventory["entries"]:
            raw = (root / "wire" / item["file"]).read_bytes()
            assert len(raw) == item["bytes"] and sha(raw) == item["sha256"]
            wire = json.loads(raw)
            assert wire["endpoint"] == receipt["endpoint"] and wire["error_kind"] is None
            req = bytes(wire["request"])
            if wire["phase"] == "request":
                assert pending is None
                pending = req
                continue
            assert wire["phase"] == "response" and req == pending
            pending = None
            header, body = bytes(wire["header"]), bytes(wire["body"])
            assert header[5:9] == req[1:5] and header[10:12] == req[10:12]
            packed_size, plain_size = struct.unpack_from("<HH", header, 12)
            assert packed_size == len(body)
            body = zlib.decompress(body) if packed_size != plain_size else body
            assert len(body) == plain_size
            command, = struct.unpack_from("<H", req, 10)
            if command == 0x000d:
                assert not pages and not rows
                continue
            assert command == 0x052b and len(req) == 24
            market, = struct.unpack_from("<H", req, 12)
            assert market == (1 if code.startswith("sh") else 0)
            assert req[14:20].decode() == code[2:]
            start, count = struct.unpack_from("<HH", req, 20)
            decoded = decode_daily(body)
            assert len(decoded) <= count
            duplicates = 0
            for row in decoded:
                old = rows.get(row["date"])
                if old is not None:
                    assert old == row
                    duplicates += 1
                rows[row["date"]] = row
            pages.append(dict(start=start, count=count, raw_count=len(decoded),
                              payload_bytes=len(body), duplicate_rows=duplicates))
        assert pending is None
        # Live evidence uses lower-bound termination with no empty confirmations.
        assert len(pages) == len(receipt["pages"])
        assert receipt["termination"] == "LowerBoundReached"
        assert sum(p["raw_count"] for p in pages) == receipt["raw_rows"]
        assert sum(p["payload_bytes"] for p in pages) == receipt["payload_bytes"]
        offset = 0
        for decoded, recorded in zip(pages, receipt["pages"]):
            assert decoded["start"] == offset == recorded["start"]
            assert decoded["count"] == recorded["requested_count"]
            for field in ["raw_count", "payload_bytes", "duplicate_rows"]:
                assert decoded[field] == recorded[field]
            assert recorded["requests"] == 1
            offset += decoded["raw_count"]
        selected = [r for d, r in sorted(rows.items()) if begin <= d <= end]
        assert min(rows) < begin and len(selected) == len(receipt["rows"])
        for row, recorded in zip(selected, receipt["rows"]):
            expected = day[row["date"]]
            date = f'{row["date"]:08d}'
            assert recorded["date"].replace("-", "") == date and recorded["code"] == code
            exported = csv_rows.pop((code, row["date"]))
            matches = {}
            for field in ["open", "high", "low", "close", "amount", "vol"]:
                matches[field] = row[field] == expected[field] == recorded[field] == float(exported[field])
            assert all(matches.values()), (code, date, matches)
            comparisons.append(dict(code=code, date=date, fields_exact=matches,
                                    volume_shares=row["vol"], official_volume_shares=expected["vol"],
                                    amount_raw=row["amount_raw"], amount_yuan=row["amount"]))
    assert not csv_rows
    return dict(endpoint=request["source"]["endpoint"], comparisons=comparisons,
                exact_rows=len(comparisons), official_sources=sources,
                csv_sha256=sha((candidate / "collected.csv").read_bytes()))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--official-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("candidates", type=Path, nargs="+")
    args = parser.parse_args()
    results = [verify(p, args.official_root) for p in args.candidates]
    assert len({r["csv_sha256"] for r in results}) == 1
    report = dict(schema_version=1, source_contract=CONTRACT, status="passed",
                  comparison="exact equality; no tolerance or rounding",
                  results=results, exact_rows=sum(r["exact_rows"] for r in results),
                  two_nodes_csv_exact=True,
                  scope="Three listed Shanghai/Shenzhen A shares; 2026-10-19..22 only")
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(dict(status=report["status"], exact_rows=report["exact_rows"])))


if __name__ == "__main__":
    main()
