#!/usr/bin/env python3
"""Independent 0feb nonempty replay; stdlib only, with explicit no-sample failure."""
import argparse
import datetime
import hashlib
import json
import math
from pathlib import Path
import struct
import unittest
import zlib

from verify_nine_commands import Reader, equal
from recent_minutes_selector import date_from_selector


def decode(payload, divisor):
    assert 0 < divisor <= 10000
    reader = Reader(payload)
    count, close_bits, open_bits = reader.scalar("H"), reader.scalar("I"), reader.scalar("I")
    assert count <= 240, "unsupported session length"
    floats = [struct.unpack("<f", struct.pack("<I", bits))[0] for bits in (close_bits, open_bits)]
    assert all(math.isfinite(value) and value >= 0 for value in floats), "invalid header prices"
    rows = []
    base = None
    for index in range(count):
        price, average, volume = reader.varint(), reader.varint(), reader.varint()
        resolved = (price, average) if base is None else (base[0] + price, base[1] + average)
        assert all(0 <= value <= 2**63 - 1 for value in (*resolved, volume)), "negative/overflow"
        if base is None:
            base = resolved
        minute = 571 + index if index < 120 else 781 + index - 120
        rows.append({
            "index": index, "time_label": f"{minute//60:02}:{minute%60:02}",
            "price_field_raw": price, "average_field_raw": average, "volume_raw": volume,
            "price_raw": resolved[0], "average_raw": resolved[1],
            "price": resolved[0]/divisor, "average_price": resolved[1]/(100*divisor),
        })
    assert reader.offset == len(payload), "trailing payload"
    return close_bits, open_bits, rows


def verify(root):
    summary = json.loads((root / "summary.json").read_text())
    assert summary["schema"] == 1 and summary["opcode"] == "0x0feb"
    date_schema = summary.get("recent_minutes_date_schema_version", 1)
    assert summary["source"] == "live_tcp" and summary["synthetic"] is False
    started = datetime.datetime.fromisoformat(summary["started_utc"].replace("Z", "+00:00"))
    finished = datetime.datetime.fromisoformat(summary["finished_utc"].replace("Z", "+00:00"))
    decimals = {(m, c): d for m, c, d in json.loads((root / "precision.json").read_text())}
    assert set(decimals) == {tuple(item) for item in summary["symbols"]}
    samples, nonempty, checked_records = {}, set(), 0
    for request_path in sorted(root.glob("frame-*.request.bin")):
        stem = Path(str(request_path).removesuffix(".request.bin"))
        request = request_path.read_bytes()
        header, body = stem.with_suffix(".header.bin").read_bytes(), stem.with_suffix(".wire.bin").read_bytes()
        assert len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400")
        opcode = struct.unpack_from("<H", request, 10)[0]
        assert struct.unpack_from("<H", header, 10)[0] == opcode and header[5:9] == request[1:5]
        source = json.loads(stem.with_suffix(".source.json").read_text())
        assert source["source"] == "live_tcp" and source["synthetic"] is False
        assert source["endpoint"] == summary["endpoint"]
        received = datetime.datetime.fromisoformat(source["received_at"].replace("Z", "+00:00"))
        assert started <= received <= finished
        packed, unpacked = struct.unpack_from("<HH", header, 12)
        assert packed == len(body)
        if packed != unpacked:
            inflater = zlib.decompressobj()
            payload = inflater.decompress(body, unpacked + 1)
            assert inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail
        else:
            payload = body
        assert len(payload) == unpacked and payload == stem.with_suffix(".payload.bin").read_bytes()
        if opcode != 0x0FEB:
            continue
        assert len(request) == 23 and request[6:8] == request[8:10] == b"\x0d\x00"
        selector = struct.unpack_from("<I", request, 12)[0]
        date = date_from_selector(selector, date_schema)
        market, code = request[16], request[17:23].decode("ascii")
        key = (date, market, code)
        assert key not in samples, "duplicate sample request"
        assert decimals[(market, code)] in (2, 3, 4)
        close, opening, rows = decode(payload, 10**decimals[(market, code)])
        dto = json.loads((root / f"recent-{date}-{market}-{code}.json").read_text())
        equal(dto, {
            "requested_date": date, "server_trading_date": None,
            "date_selector_raw": selector, "previous_close_bits": close, "open_bits": opening,
        })
        equal(dto["page"]["records"], rows)
        assert dto["page"]["source"]["connection_id"] == summary["connection_id"]
        assert dto["page"]["source"]["endpoint"] == summary["endpoint"]
        samples[key] = len(rows)
        checked_records += len(rows)
        if rows:
            nonempty.add((market, code))
    for sample in summary.get("results", []):
        key = (sample["requested_date"], sample["market"], sample["code"])
        assert samples[key] == sample["points"] and sample["nonempty"] == bool(samples[key])
    expected = {(date, market, code) for date in summary["dates"] for market, code in summary["symbols"]}
    complete = summary["capture_complete"] and set(samples) == expected
    accepted = complete and nonempty == set(decimals)
    assert accepted == (summary["status"] == "nonempty_captured_requires_independent_verification")
    return {
        "verification": "passed", "live_nonempty_verified": accepted,
        "business_status": "passed" if accepted else "nonempty_not_verified",
        "sample_frames_checked": len(samples), "records_checked": checked_records,
        "nonempty_symbols": sorted(nonempty),
        "scope": "Only sampled parameters; no server-date echo or full exchange session proof.",
    }


class Regression(unittest.TestCase):
    def test_handwritten_nonempty_sample_and_empty_are_distinct(self):
        payload = bytes([4, 0, 246, 40, 44, 64, 205, 204, 44, 64, 142, 42, 176, 250, 32,
                         0, 10, 168, 15, 20, 69, 208, 31, 30, 0, 0, 0])
        close, opening, rows = decode(payload, 1000)
        self.assertEqual((close, opening), (0x402c28f6, 0x402ccccd))
        self.assertEqual([(r["price_raw"], r["average_raw"], r["volume_raw"]) for r in rows],
                         [(2702, 270000, 0), (2712, 271000, 20), (2697, 268000, 30), (2702, 270000, 0)])
        self.assertEqual(decode(bytes(10), 1000)[2], [])
        for end in range(len(payload)):
            with self.assertRaises((AssertionError, struct.error)):
                decode(payload[:end], 1000)
        with self.assertRaises(AssertionError):
            decode(payload + b"\0", 1000)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, nargs="?")
    parser.add_argument("--freeze", action="store_true", help="initial archive SHA manifest")
    parser.add_argument("--self-test", action="store_true", help="synthetic decoder regression only")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables validation")
    if args.self_test:
        unittest.main(argv=["verify_recent_minutes_boundaries"], exit=True)
    if args.directory is None:
        parser.error("directory required")
    root = args.directory
    sources = sorted(p for p in root.iterdir() if p.is_file()
                     and p.name not in {"independent-verification.json", "sha256-manifest.json"})
    digests = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest = root / "sha256-manifest.json"
    if manifest.exists():
        assert json.loads(manifest.read_text()) == digests, "captured files changed"
    else:
        assert args.freeze, "first verification requires --freeze"
    result = verify(root)
    if not manifest.exists():
        manifest.write_text(json.dumps(digests, indent=2) + "\n")
    (root / "independent-verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    if not result["live_nonempty_verified"]:
        raise SystemExit(2)


if __name__ == "__main__":
    main()
