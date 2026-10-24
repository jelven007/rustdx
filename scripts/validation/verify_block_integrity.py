#!/usr/bin/env python3
"""Independently replay 02c5/06b9 frames, MD5 and fixed-slot board records."""
import argparse
from collections import Counter
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import struct
import zlib


def u16(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def load(path):
    return json.loads(path.read_text())


def observed_day(timestamp, zone):
    return datetime.fromisoformat(timestamp.replace("Z", "+00:00")).astimezone(zone).date()


def decode_records(content):
    assert len(content) >= 386, "truncated board header"
    count = u16(content, 384)
    body = content[386:]
    assert count > 0 and len(body) % count == 0, "uneven slots"
    slot_size = len(body) // count
    assert slot_size >= 13, "short slot"
    records = []
    seventh = Counter()
    for index in range(count):
        slot = body[index * slot_size:(index + 1) * slot_size]
        name = slot[:9].split(b"\0", 1)[0].decode("gbk", errors="strict")
        members = u16(slot, 9)
        kind = u16(slot, 11)
        assert members <= (slot_size - 13) // 7, "member crosses slot"
        assert name or not members, "missing board name"
        for member in range(members):
            raw = slot[13 + member * 7:20 + member * 7]
            assert len(raw) == 7 and all(48 <= b <= 57 for b in raw[:6])
            records.append({"blockname": name, "block_type": kind, "code": raw[:6].decode("ascii")})
            seventh[f"{raw[6]:02x}"] += 1
    return records, count, slot_size, dict(seventh)


def verify_node(node):
    summary = load(node / "summary.json")
    assert summary["source"] == "live_tcp"
    assert all(q["md5_verified"] and q["decoded"] for q in summary["queries"])
    declared = {}
    files = {}
    raw_chunks = {}
    frame_count = 0
    compressed_count = 0
    identities = []
    for request_path in sorted(node.glob("frame-*.request.bin")):
        stem = request_path.name.removesuffix(".request.bin")
        request = request_path.read_bytes()
        header = (node / f"{stem}.header.bin").read_bytes()
        wire = (node / f"{stem}.wire.bin").read_bytes()
        payload = (node / f"{stem}.payload.bin").read_bytes()
        endpoint = (node / f"{stem}.endpoint.txt").read_text()
        fetched = (node / f"{stem}.utc.txt").read_text()
        assert len(request) == 10 + u16(request, 6)
        assert u16(request, 6) == u16(request, 8)
        assert len(header) == 16 and header[:4] == b"\xb1\xcb\x74\0"
        assert header[5:9] == request[1:5], "response sequence mismatch"
        opcode = u16(request, 10)
        assert u16(header, 10) == opcode, "response opcode mismatch"
        assert len(wire) == u16(header, 12)
        if u16(header, 12) != u16(header, 14):
            inflater = zlib.decompressobj()
            expanded = inflater.decompress(wire, 65536)
            assert not inflater.unconsumed_tail, "expanded frame exceeds bound"
            expanded += inflater.flush()
            assert inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail
            compressed_count += 1
        else:
            expanded = wire
        assert expanded == payload and len(payload) == u16(header, 14)
        assert endpoint == summary["endpoint"]
        identities.append({"frame": stem, "opcode": f"{opcode:04x}",
                           "sequence_hex": request[1:5].hex(), "endpoint": endpoint, "utc": fetched})
        frame_count += 1
        if opcode == 0x000d:  # Ordinary PACK1 handshake.
            continue
        if opcode == 0x02c5:
            assert len(request) == 52 and len(payload) >= 38
            filename = request[12:].split(b"\0", 1)[0].decode("ascii")
            assert filename not in declared
            size, digest = u32(payload, 0), payload[5:37].decode("ascii")
            assert len(digest) == 32 and all(c in "0123456789abcdefABCDEF" for c in digest)
            declared[filename] = (size, digest, payload)
            files[filename] = bytearray()
            raw_chunks[filename] = []
        elif opcode == 0x06b9:
            assert len(request) == 120 and len(payload) >= 4
            filename = request[20:].split(b"\0", 1)[0].decode("ascii")
            start, length = u32(request, 12), u32(request, 16)
            assert filename in declared
            assert start == len(files[filename])
            assert length == min(30000, declared[filename][0] - start)
            assert len(payload) == 4 + length
            files[filename].extend(payload[4:])
            raw_chunks[filename].append({"start": start, "length": length,
                                         "header_raw": list(payload[:4]), "utc": fetched})
        else:
            raise AssertionError(f"unexpected opcode {opcode:04x}")
    assert frame_count == summary["wire_frames"]
    results = []
    zone = timezone(timedelta(hours=8))
    for filename, (size, digest, payload) in declared.items():
        content = bytes(files[filename])
        assert len(content) == size
        assert hashlib.md5(content).hexdigest() == digest.lower()
        assert (node / filename).read_bytes() == content
        metadata = load(node / f"{filename}.metadata.json")
        assert metadata["filename"] == filename
        assert metadata["computed_md5"] == digest.lower()
        meta = metadata["metadata"]
        assert meta["size"] == size and meta["md5"] == digest
        assert meta["md5_raw"] == list(payload[5:37])
        assert meta["reserved_raw"] == payload[4] and meta["terminator_raw"] == payload[37]
        assert meta["trailing_payload"] == list(payload[38:])
        source = meta["source"]
        assert source["endpoint"] == summary["endpoint"]
        assert source["connection_id"] is not None
        day = observed_day(source["fetched_at"], zone)
        assert len(metadata["chunks"]) == len(raw_chunks[filename])
        for chunk, raw in zip(metadata["chunks"], raw_chunks[filename]):
            assert all(chunk[k] == raw[k] for k in ["start", "length", "header_raw"])
            assert chunk["source"]["endpoint"] == source["endpoint"]
            assert chunk["source"]["connection_id"] == source["connection_id"]
            assert observed_day(chunk["source"]["fetched_at"], zone) == day
        records, boards, slot_size, seventh = decode_records(content)
        assert records == load(node / f"{filename}.records.json")
        results.append({"filename": filename, "size": size, "md5": digest.lower(),
                        "sha256": hashlib.sha256(content).hexdigest(),
                        "chunks": len(raw_chunks[filename]), "boards": boards, "slot_size": slot_size,
                        "records": len(records), "seventh_byte_counts_raw": seventh,
                        "all_fields_equal": True, "md5_verified": True})
    return {"endpoint": summary["endpoint"], "wire_frames": frame_count,
            "compressed_frames": compressed_count, "response_identities_verified": True,
            "files": results, "frames": identities}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    nodes = [verify_node(node) for node in sorted(args.evidence.glob("live-*")) if node.is_dir()]
    assert len(nodes) == 2, "expected two live nodes"
    assert nodes[0]["files"] == nodes[1]["files"], "nodes differ"
    result = {"schema": 1, "verified_at_utc": datetime.now(timezone.utc).isoformat(),
              "method": "Python struct/zlib/hashlib.md5/strict GBK independent of Rust decoders",
              "nodes": nodes, "two_nodes_content_equal": True,
              "total_files": sum(len(n["files"]) for n in nodes),
              "total_records_compared": sum(f["records"] for n in nodes for f in n["files"]),
              "scope": "captured block_gn.dat/block_zs.dat only; supplier categories and seventh-byte meaning unverified"}
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "nodes"}, ensure_ascii=False))


if __name__ == "__main__":
    main()
