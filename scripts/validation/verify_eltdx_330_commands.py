#!/usr/bin/env python3
"""Independently decode captured 02c5/02cf/02d0/052b bytes using stdlib only."""
import argparse
import datetime as dt
import hashlib
import json
import math
from pathlib import Path
import struct
import zlib


def u16(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def integer(data, pos):
    first = data[pos]
    pos += 1
    magnitude = first & 63
    negative = bool(first & 64)
    shift = 6
    part = first
    while part & 128:
        assert shift <= 62
        part = data[pos]
        pos += 1
        magnitude += (part & 127) << shift
        shift += 7
    assert magnitude <= 2**63 - 1 + int(negative)
    return (-magnitude if negative else magnitude), pos


def amount(raw):
    if raw == 0:
        return 0.0
    low, middle, high, exponent_byte = raw.to_bytes(4, "little")
    exponent_byte = exponent_byte if exponent_byte < 128 else exponent_byte - 256
    exponent = 2 * exponent_byte - 127
    mantissa = (64 + (high & 127)) / 64 if high > 128 else high / 128
    fraction_scale = 2 if high & 128 else 1
    mantissa += fraction_scale * (middle / 32768 + low / 8388608)
    return math.ldexp(mantissa, exponent)


def gbk(data):
    return data.split(b"\0", 1)[0].decode("gbk")


def delisted(data, requested):
    count = u16(data, 0)
    assert count <= requested and count <= (len(data) - 2) // 17
    pos = 2
    previous = 0
    rows = []
    for i in range(count):
        begin = pos
        day = u32(data, pos)
        date = dt.date(day // 10000, day // 100 % 100, day % 100)
        pos += 4
        values = []
        for _ in range(5):
            value, pos = integer(data, pos)
            values.append(value)
        open_delta, close_delta, high_delta, low_delta, volume = values
        opening = previous + open_delta
        closing, high, low = (opening + delta for delta in values[1:4])
        assert low >= 0 and low <= min(opening, closing) <= max(opening, closing) <= high
        assert 0 <= volume <= 2**32 - 1
        raw_amount = u32(data, pos)
        pos += 4
        suffix = list(data[pos:pos + 4])
        assert len(suffix) == 4
        pos += 4
        rows.append({
            "date_raw": day, "date": date.isoformat(),
            "time": dt.datetime.combine(date, dt.time(), dt.timezone(dt.timedelta(hours=8))).isoformat(),
            "open_price_milli": opening, "close_price_milli": closing,
            "high_price_milli": high, "low_price_milli": low,
            "open": opening / 1000, "close": closing / 1000,
            "high": high / 1000, "low": low / 1000,
            "last_close_price_milli": previous if i else None,
            "open_delta_raw": open_delta, "close_delta_raw": close_delta,
            "high_delta_raw": high_delta, "low_delta_raw": low_delta,
            "volume_raw": volume, "volume_shares": volume, "volume_lots": volume / 100,
            "amount_raw": raw_amount, "amount": amount(raw_amount),
            "unknown_suffix_raw": suffix, "record_raw": list(data[begin:pos]),
        })
        previous = closing
    assert pos == len(data)
    return rows


def equal(actual, expected):
    for key, value in expected.items():
        if isinstance(value, float):
            assert math.isclose(actual[key], value, rel_tol=1e-12, abs_tol=1e-20), (key, actual[key], value)
        else:
            assert actual[key] == value, (key, actual[key], value)


def verify(directory):
    frames = []
    metadata_frames = []
    directories = {}
    text_frames = {}
    bar_frames = {}
    counts = {}
    for request_path in sorted(directory.glob("frame-*.request.bin")):
        stem = request_path.name.removesuffix(".request.bin")
        read = lambda suffix: (directory / (stem + suffix)).read_bytes()
        request = read(".request.bin")
        header, wire, saved_payload = (read(s) for s in (".header.bin", ".wire.bin", ".payload.bin"))
        opcode = u16(request, 10)
        assert request[0] == 12 and request[5] == 1
        assert u16(request, 6) == u16(request, 8) == len(request) - 10
        assert header[:4] == bytes([0xb1, 0xcb, 0x74, 0])
        assert header[5:9] == request[1:5] and u16(header, 10) == opcode
        assert len(wire) == u16(header, 12)
        payload = zlib.decompress(wire) if u16(header, 12) != u16(header, 14) else wire
        assert payload == saved_payload and len(payload) == u16(header, 14)
        body = request[12:]
        decoded = None
        if opcode == 0x02c5:
            assert len(body) == 40 and len(payload) >= 38
            decoded = {
                "size": u32(payload, 0), "md5": gbk(payload[5:37]).strip(),
                "reserved_raw": payload[4], "md5_raw": list(payload[5:37]),
                "terminator_raw": payload[37], "trailing_payload": list(payload[38:]),
            }
            metadata_frames.append(decoded)
        elif opcode == 0x02cf:
            assert len(body) == 12 and body[8:] == bytes(4)
            count = u16(payload, 0)
            assert 2 + count * 152 <= len(payload)
            entries = []
            for i in range(count):
                row = payload[2 + i * 152:2 + (i + 1) * 152]
                entries.append({
                    "name": gbk(row[:64]), "filename": gbk(row[64:144]),
                    "start": u32(row, 144), "length": u32(row, 148),
                    "name_raw": list(row[:64]), "filename_raw": list(row[64:144]),
                })
            key = (u16(body, 0), body[2:8].decode("ascii"))
            decoded = {"entries": entries, "trailing_payload": list(payload[2 + count * 152:])}
            directories[key] = decoded
        elif opcode == 0x02d0:
            assert len(body) == 102 and body[8:10] == bytes(2) and body[98:] == bytes(4)
            length = u16(payload, 10)
            assert length <= u32(body, 94) and 12 + length <= len(payload)
            decoded = {
                "header_raw": list(payload[:10]), "declared_length": length,
                "content_raw": list(payload[12:12 + length]),
                "trailing_payload": list(payload[12 + length:]),
            }
            key = (u16(body, 0), body[2:8].decode("ascii"), body[10:90].split(b"\0", 1)[0],
                   u32(body, 90), u32(body, 94))
            text_frames[key] = decoded
        elif opcode == 0x052b:
            assert len(body) == 12 and 1 <= u16(body, 10) <= 800
            decoded = delisted(payload, u16(body, 10))
            key = (u16(body, 0), body[2:8].decode("ascii"), u16(body, 8), u16(body, 10))
            bar_frames[key] = decoded
        elif opcode != 0x000d:
            raise AssertionError(f"unexpected opcode {opcode:04x}")
        counts[f"0x{opcode:04x}"] = counts.get(f"0x{opcode:04x}", 0) + 1
        frames.append({"stem": stem, "opcode": f"0x{opcode:04x}", "payload_bytes": len(payload),
                       "record_count": len(decoded) if opcode == 0x052b else None})

    checked_records = 0
    checked_chunks = 0
    results = json.loads((directory / "summary.json").read_text())
    for result in results["results"]:
        assert result["decoded"], result
        name = result["name"]
        actual = json.loads((directory / f"{name}.json").read_text())
        if name == "block-metadata":
            equal(actual, metadata_frames[0])
        elif name.startswith("f10-directory"):
            r = actual["request"]
            expected = directories[(r["market"], r["code"])]
            assert len(actual["entries"]) == len(expected["entries"])
            equal(actual, {"trailing_payload": expected["trailing_payload"]})
            for a, e in zip(actual["entries"], expected["entries"]):
                equal(a, e)
        elif name.startswith("f10-text"):
            assembled = bytearray()
            r = actual["request"]
            offset = r["offset"]
            for chunk in actual["chunks"]:
                c = chunk["request"]
                assert c["offset"] == offset
                key = (c["market"], c["code"], bytes(c["filename_raw"]), c["offset"], c["length"])
                expected = text_frames[key]
                equal(chunk, expected)
                assembled.extend(expected["content_raw"])
                offset += expected["declared_length"]
                checked_chunks += 1
            assert bytes(assembled) == bytes(actual["content_raw"])
            assert len(assembled) == r["length"] and actual["complete"]
            assert assembled.decode("gbk") == actual["text"]
        elif name.startswith("delisted-"):
            pages = actual["pages"] if "pages" in actual else [actual]
            merged = {}
            raw_count = 0
            next_offset = 0
            for page in pages:
                r = page["request"]
                key = (r["market"], r["code"], r["start"], r["count"])
                rows = bar_frames[key]
                assert len(rows) == len(page["records"])
                if "pages" in actual:
                    assert r["start"] == next_offset
                    next_offset += len(rows)
                for a, e in zip(page["records"], rows):
                    equal(a, e)
                    merged[e["date_raw"]] = e
                    checked_records += 1
                raw_count += len(rows)
            if "pages" in actual:
                assert actual["complete"] and actual["termination"] == "EmptyPage"
                assert not pages[-1]["records"] and actual["raw_record_count"] == raw_count
                assert [r["date_raw"] for r in actual["records"]] == sorted(merged)
                for a in actual["records"]:
                    equal(a, merged[a["date_raw"]])
    hashes = [{"path": p.name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
               "bytes": p.stat().st_size}
              for p in sorted(directory.iterdir()) if p.is_file() and p.name != "independent-verification.json"]
    return {
        "verified": True, "endpoint": results["endpoint"], "captured_at": results["captured_at"],
        "method": "stdlib independent LE/sign-magnitude/GBK/TDX exponent decode; complete wire and JSON comparison",
        "frame_count": len(frames), "opcode_frame_counts": counts, "frames": frames,
        "checked_delisted_records": checked_records, "checked_text_chunks": checked_chunks,
        "files": hashes,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directories", nargs="+", type=Path)
    args = parser.parse_args()
    for directory in args.directories:
        result = verify(directory)
        (directory / "independent-verification.json").write_text(
            json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps({k: v for k, v in result.items() if k not in ("frames", "files")},
                         ensure_ascii=False))


if __name__ == "__main__":
    main()
