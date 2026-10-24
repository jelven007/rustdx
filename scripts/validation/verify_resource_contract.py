#!/usr/bin/env python3
"""Independent stdlib 300-byte 06b9 framing, file boundaries and ZIP/CFG replay."""
import argparse
import datetime as dt
import hashlib
import io
import json
import math
from pathlib import Path
import struct
import zipfile
import zlib

ZONE = dt.timezone(dt.timedelta(hours=8))


def stamp(value):
    result = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    assert result.tzinfo is not None
    return result.astimezone(ZONE)


def number(value):
    try:
        result = float(value.strip())
        return result if math.isfinite(result) else None
    except ValueError:
        return None


def integer(value):
    result = number(value)
    return int(result) if result is not None and result.is_integer() and -(2**63) <= result < 2**63 else None


def text(value):
    return value.strip() or None


def cfg_rows(data, name):
    stat = name == "tdxstat.cfg"
    rows, unparsed, keys = [], [], set()
    for index, line in enumerate(data.decode("gbk").splitlines(), 1):
        assert index <= 100000 and len(line.encode("utf-8")) <= 16384
        if not line.strip():
            continue
        fields = line.split("|")
        try:
            market = int(fields[0].strip())
            assert -(2**31) <= market < 2**31
        except (ValueError, AssertionError):
            unparsed.append([index, line])
            continue
        if len(fields) < (35 if stat else 21):
            unparsed.append([index, line])
            continue
        code = fields[1].strip()
        assert code and len(code) <= 32 and code.isascii() and "\0" not in code
        assert (market, code) not in keys
        keys.add((market, code))
        if stat:
            values = {"stats_date_raw": text(fields[4])}
            for key, offset in {"beta_60d": 2, "pe_ttm": 3, "free_float_shares_10k": 11}.items():
                values[key] = number(fields[offset])
            for key, offset in {"year_limit_up_days": 26, "limit_stat_days": 31,
                                "limit_up_count_in_stat_days": 32, "limit_up_streak_days": 33}.items():
                values[key] = integer(fields[offset])
        else:
            values = {"stats_date_raw": text(fields[2])}
            for key, offset in {"amount_10k": 3, "seal_amount_10k": 4, "prev_amount_10k": 5,
                                "prev_seal_amount_10k": 6, "prev2_amount_10k": 7,
                                "prev2_seal_amount_10k": 8, "open_volume_hand": 9,
                                "prev_open_volume_hand": 10, "open_amount_10k": 14,
                                "prev_open_amount_10k": 15}.items():
                values[key] = number(fields[offset])
        rows.append({"market_id_raw": market, "code": code, "line_number": index,
                     "fields_raw": fields, "candidate_values": {"Stat" if stat else "Stat2": values}})
    assert rows
    return rows, unparsed


def verify(root):
    summary = json.loads((root / "summary.json").read_text())
    assert summary["source"] == "live_tcp"
    assert summary["auto_reconnect"] == 0 and summary["heartbeat"] is False
    frames, compressed, responses = [], 0, {}
    for request_path in sorted(root.glob("frame-*.request.bin")):
        stem = request_path.name.removesuffix(".request.bin")
        get = lambda suffix: (root / f"{stem}.{suffix}").read_bytes()
        request, header, body = get("request.bin"), get("header.bin"), get("wire.bin")
        assert len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400")
        opcode = struct.unpack_from("<H", request, 10)[0]
        assert header[10:12] == request[10:12] and header[5:9] == request[1:5]
        assert request[6:8] == request[8:10]
        assert struct.unpack_from("<H", request, 6)[0] == len(request) - 10
        packed, unpacked = struct.unpack_from("<HH", header, 12)
        assert packed == len(body)
        if packed != unpacked:
            decoder = zlib.decompressobj()
            payload = decoder.decompress(body, unpacked + 1)
            assert decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail
            compressed += 1
        else:
            payload = body
        assert len(payload) == unpacked and payload == get("payload.bin")
        assert (root / f"{stem}.endpoint.txt").read_text() == summary["endpoint"]
        at = stamp((root / f"{stem}.utc.txt").read_text())
        if opcode == 0x06B9:
            assert len(request) == 320, "general resources use a 300-byte path"
            offset, size = struct.unpack_from("<II", request, 12)
            raw = request[20:]
            path = raw.rstrip(b"\0").decode("ascii")
            assert b"\0" not in raw[:len(path)] and 1 <= len(path) <= 300
            assert 1 <= size <= 60000 and offset + size < 2**32
            length = struct.unpack_from("<I", payload)[0]
            assert length <= size and len(payload) >= length + 4
            responses.setdefault(path, []).append((offset, size, length, payload[4:4+length],
                                                  payload[4+length:], payload, at))
        else:
            assert opcode == 0x000D
        frames.append(stem)
    assert len(frames) == summary["wire_frames"]
    files = {}
    connection = None
    for query in summary["queries"]:
        assert query["complete"], "this evidence must observe a short/empty terminal chunk"
        path = query["path"]
        dto = json.loads((root / f"{path}.json").read_text())
        content = (root / path).read_bytes()
        assert len(content) == query["size"]
        assert dto["complete"] and dto["termination"] == query["termination"]
        assert len(dto["chunks"]) == len(responses[path]) == query["chunks"]
        assembled, response_bytes, day = bytearray(), 0, None
        for captured, chunk in zip(responses[path], dto["chunks"]):
            offset, size, length, data, tail, payload, at = captured
            assert offset == len(assembled) == chunk["offset"]
            assert size == chunk["requested_size"] and length == chunk["chunk_len"]
            assert chunk["header_raw"] == list(payload[:4]) and chunk["trailing_payload"] == list(tail)
            source = chunk["source"]
            assert source["opcode"] == 0x06B9 and source["endpoint"] == summary["endpoint"]
            if connection is None:
                connection = source["connection_id"]
            assert source["connection_id"] == connection and connection is not None
            fetched = stamp(source["fetched_at"])
            assert 0 <= (fetched - at).total_seconds() < 5
            day = day or fetched.date()
            assert fetched.date() == day
            assembled.extend(data)
            response_bytes += len(payload)
        assert assembled == content and response_bytes == dto["response_bytes"]
        final = responses[path][-1]
        assert final[2] < final[1] and all(r[2] == r[1] for r in responses[path][:-1])
        assert dto["termination"] == ("EmptyChunk" if final[2] == 0 else "ShortChunk")
        files[path] = {"bytes": len(content), "chunks": len(responses[path]),
                       "md5_computed_only": hashlib.md5(content).hexdigest(),
                       "sha256": hashlib.sha256(content).hexdigest()}
    stats = json.loads((root / "zhb.stats.json").read_text())
    assert stats["layout_profile"] == "eltdx-3.3.0/zhb-cfg-candidate-v1"
    totals = {}
    with zipfile.ZipFile(io.BytesIO((root / "zhb.zip").read_bytes())) as archive:
        infos = archive.infolist()
        assert len(infos) <= 128 and len({i.filename for i in infos}) == len(infos)
        assert sum(i.file_size for i in infos) <= 64*1024*1024
        assert all(not (i.flag_bits & 1) and i.file_size <= 32*1024*1024 for i in infos)
        assert archive.testzip() is None, "independent CRC validation of every entry"
        assert [member["name"] for member in stats["members"]] == ["tdxstat.cfg", "tdxstat2.cfg"]
        for member in stats["members"]:
            name = member["name"]
            info = archive.getinfo(name)
            content = archive.read(name)
            assert content == (root / name).read_bytes()
            assert info.CRC == member["crc32"] and info.compress_size == member["compressed_size"]
            rows, unparsed = cfg_rows(content, name)
            assert rows == member["rows"] and unparsed == member["unparsed_lines"]
            totals[name] = {"rows": len(rows), "fields_checked": sum(len(r["fields_raw"]) for r in rows),
                            "candidate_values_checked": sum(len(next(iter(r["candidate_values"].values()))) for r in rows),
                            "unparsed_lines": len(unparsed), "crc32": info.CRC}
    return {"verification": "passed", "source": "live_tcp", "wire_frames": len(frames),
            "compressed_frames": compressed, "files": files, "stats": totals,
            "archive_entries_crc_checked": len(infos),
            "scope": "Framing/declared lengths/byte boundaries/source continuity/ZIP CRC/GBK/all CFG slots and candidate mapping. No independent business-unit, vendor-authority or trading-push acceptance."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables validation")
    root = args.directory
    sources = sorted(p for p in root.iterdir() if p.is_file()
                     and p.name not in {"independent.json", "sha256-manifest.json"})
    digests = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest = root / "sha256-manifest.json"
    if manifest.exists():
        assert json.loads(manifest.read_text()) == digests, "captured files changed"
    else:
        assert args.freeze
    result = verify(root)
    if not manifest.exists():
        manifest.write_text(json.dumps(digests, indent=2) + "\n")
    (root / "independent.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
