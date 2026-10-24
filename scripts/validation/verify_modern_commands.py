#!/usr/bin/env python3
"""Independent stdlib replay of captured 0004/044d/0537 wire frames."""
import json
import math
from pathlib import Path
import struct
import sys
import zlib


def signed_varint(payload, offset):
    byte = payload[offset]
    offset += 1
    negative = bool(byte & 64)
    value = byte & 63
    shift = 6
    while byte & 128:
        byte = payload[offset]
        offset += 1
        value += (byte & 127) << shift
        shift += 7
        assert shift <= 34
    return (-value if negative else value), offset


def verify(root):
    directory = {0: {}, 1: {}}
    totals = {}
    heartbeats = []
    minute_results = []
    frames = 0
    for request_path in sorted(root.glob("frame-*.request.bin")):
        stem = str(request_path).removesuffix(".request.bin")
        request = request_path.read_bytes()
        header = Path(stem + ".header.bin").read_bytes()
        body = Path(stem + ".wire.bin").read_bytes()
        packed, unpacked = struct.unpack_from("<HH", header, 12)
        assert packed == len(body)
        payload = zlib.decompress(body) if packed != unpacked else body
        assert len(payload) == unpacked
        assert payload == Path(stem + ".payload.bin").read_bytes()
        command = struct.unpack_from("<H", request, 10)[0]
        frames += 1
        if command == 4:
            assert len(request) == 12 and len(payload) >= 10
            heartbeats.append(struct.unpack_from("<I", payload, 6)[0])
        elif command == 0x044E:
            market = struct.unpack_from("<H", request, 12)[0]
            totals[market] = struct.unpack_from("<H", payload)[0]
        elif command == 0x044D:
            market, start, limit = struct.unpack_from("<HII", request, 12)
            count = struct.unpack_from("<H", payload)[0]
            assert count <= limit <= 1600
            assert start == len(directory[market])
            assert len(payload) == 2 + count * 37
            expected = json.loads((root / f"directory-{market}.json").read_text())
            for index in range(count):
                row = payload[2 + index * 37:2 + (index + 1) * 37]
                code = row[:6].decode("ascii")
                assert code not in directory[market]
                unit = struct.unpack_from("<H", row, 6)[0]
                name = row[8:24].split(b"\0", 1)[0].decode("gbk", errors="replace")
                decimal = row[28]
                ratio, close = struct.unpack_from("<f", row, 24)[0], struct.unpack_from("<f", row, 29)[0]
                rust = expected[start + index]
                assert (rust["code"], rust["name"], rust["decimal"], rust["volume_unit"]) == (code, name, decimal, unit)
                # serde_json's shortest f32 representation need not equal Python f64.
                assert math.isclose(rust["previous_close"], close, rel_tol=1e-7, abs_tol=1e-7)
                assert math.isclose(rust["volume_ratio_base"], ratio, rel_tol=1e-7, abs_tol=1e-7)
                assert rust["unknown3_raw"] == list(row[33:37])
                directory[market][code] = decimal
        elif command == 0x0537:
            market = struct.unpack_from("<H", request, 12)[0]
            code = request[14:20].decode("ascii")
            assert request[20:] == bytes([0, 0, 0, 0x93])
            count = struct.unpack_from("<H", payload)[0]
            expected = json.loads((root / f"minutes-{market}-{code}.json").read_text())
            assert count == len(expected["points"]) <= 240
            scale = 10 ** directory[market][code]
            offset = 4
            base = None
            volumes = 0
            for index in range(count):
                price, offset = signed_varint(payload, offset)
                average, offset = signed_varint(payload, offset)
                volume, offset = signed_varint(payload, offset)
                raw = (price, average) if base is None else (base[0] + price, base[1] + average)
                if base is None:
                    base = raw
                rust = expected["points"][index]
                assert rust["price_raw"] == raw[0] and rust["average_raw"] == raw[1]
                assert rust["price_field_raw"] == price and rust["average_field_raw"] == average
                assert rust["volume_raw"] == volume
                assert math.isclose(rust["price"], raw[0] / scale, abs_tol=1e-12)
                assert math.isclose(rust["average_price"], raw[1] / (scale * 100), abs_tol=1e-12)
                volumes += volume
            assert offset == len(payload) and expected["trading_date"] is None
            quotes = json.loads((root / "quotes.json").read_text())["records"]
            quote = next(row for row in quotes if row["code"] == code)
            assert expected["points"][-1]["price"] == quote["price"]
            minute_results.append({
                "market": market, "code": code, "points_checked": count,
                "decimal": directory[market][code], "last_price_matches_053e": True,
                "minute_volume_sum": volumes, "snapshot_volume": quote["vol"],
                "volume_difference": volumes - quote["vol"],
            })
    assert {m: len(rows) for m, rows in directory.items()} == totals
    return {
        "method": "Python stdlib, independent byte offsets/varints/scaling; no eltdx import",
        "wire_frames_checked": frames, "directory_rows_checked": sum(totals.values()),
        "directory_counts": totals, "heartbeat_dates": heartbeats, "minutes": minute_results,
        "limitation": "One endpoint; volume aggregation differences and absent wire dates do not prove exchange completeness",
    }


if __name__ == "__main__":
    root = Path(sys.argv[1])
    result = verify(root)
    text = json.dumps(result, ensure_ascii=False, indent=2)
    (root / "independent-verification.json").write_text(text + "\n")
    print(text)
