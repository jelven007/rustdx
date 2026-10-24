#!/usr/bin/env python3
"""Freeze independently decoded NONEMPTY live 0feb frames into a new fixture directory."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from verify_recent_auction import read_frame
from verify_recent_minutes_boundaries import decode
from evidence_paths import workspace_path
from recent_minutes_selector import date_from_selector


def check(output):
    manifest = json.loads((output / "manifest.json").read_text())
    assert manifest["synthetic"] is False and manifest["server_trading_date_verified"] is False
    count = 0
    for name, fixture in manifest["fixtures"].items():
        for file, digest in fixture["sha256"].items():
            assert hashlib.sha256((output / file).read_bytes()).hexdigest() == digest, file
        source = workspace_path(fixture["source_directory"])
        request_path = source / (fixture["source_frame"] + ".request.bin")
        opcode, request, payload = read_frame(request_path, fixture["endpoint"])
        assert opcode == 0x0FEB
        source_summary = json.loads((source / "summary.json").read_text())
        assert fixture["requested_date"] == date_from_selector(
            struct.unpack_from("<I", request, 12)[0],
            source_summary.get("recent_minutes_date_schema_version", 1),
        )
        for suffix in ["request.bin", "header.bin", "wire.bin", "payload.bin"]:
            assert (output / (name + "." + suffix)).read_bytes() == (
                source / (fixture["source_frame"] + "." + suffix)).read_bytes()
        close, opening, rows = decode(payload, 10**fixture["decimal"])
        assert len(rows) == fixture["records"] > 0
        expected = json.loads((output / (name + ".expected.json")).read_text())
        assert expected == {
            "previous_close_bits": close, "open_bits": opening,
            "requested_date": fixture["requested_date"], "server_trading_date": None,
            "date_selector_raw": struct.unpack_from("<I", request, 12)[0], "records": rows,
        }
        count += len(rows)
    return len(manifest["fixtures"]), count


def freeze(sources, output):
    """Original captures remain read-only; never generate expectations with Rust."""
    entries = {}
    for source in sources:
        manifest = json.loads((source / "sha256-manifest.json").read_text())
        actual = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in source.iterdir() if path.is_file()
                  and path.name not in {"independent-verification.json", "sha256-manifest.json"}}
        assert actual == manifest, "captured files changed"
        summary = json.loads((source / "summary.json").read_text())
        verification = json.loads((source / "independent-verification.json").read_text())
        assert summary["source"] == "live_tcp" and summary["status"] == "captured"
        assert verification["verification"] == "passed" and verification["recent_nonempty_live_verified"]
        precision = {(m, c): d for m, c, d in json.loads((source / "precision.json").read_text())}
        for request_path in sorted(source.glob("frame-*.request.bin")):
            opcode, request, payload = read_frame(request_path, summary["endpoint"])
            if opcode != 0x0FEB or not struct.unpack_from("<H", payload)[0]:
                continue
            selector = struct.unpack_from("<I", request, 12)[0]
            date_schema = summary.get("recent_minutes_date_schema_version", 1)
            date = date_from_selector(selector, date_schema)
            market, code = request[16], request[17:23].decode("ascii")
            decimal = precision[(market, code)]
            assert decimal in (2, 3, 4)
            close, opening, rows = decode(payload, 10**decimal)
            host = summary["endpoint"].replace(".", "-").replace(":", "-")
            name = f"{host}-{date}-{market}-{code}"
            assert name not in entries, "duplicate fixture"
            stem = Path(str(request_path).removesuffix(".request.bin"))
            data = {suffix: stem.with_suffix("." + suffix).read_bytes()
                    for suffix in ["request.bin", "header.bin", "wire.bin", "payload.bin"]}
            expected = {
                "previous_close_bits": close, "open_bits": opening,
                "requested_date": date, "server_trading_date": None,
                "date_selector_raw": selector, "records": rows,
            }
            data["expected.json"] = (json.dumps(expected, ensure_ascii=False, indent=2) + "\n").encode()
            entries[name] = {
                "opcode": "0x0feb", "market": market, "code": code, "decimal": decimal,
                "recent_minutes_date_schema_version": date_schema,
                "requested_date": date, "records": len(rows), "endpoint": summary["endpoint"],
                "received_at": stem.with_suffix(".utc.txt").read_text(),
                "source_directory": str(source),
                "source_frame": stem.name,
                "sha256": {name + "." + suffix: hashlib.sha256(content).hexdigest()
                           for suffix, content in data.items()},
                "_files": data,
            }
    assert entries, "no nonempty live frames found"
    output.mkdir()  # A new directory is mandatory; no existing fixture is overwritten.
    for name, entry in entries.items():
        for suffix, data in entry.pop("_files").items():
            (output / (name + "." + suffix)).write_bytes(data)
    manifest = {
        "schema_version": 1, "synthetic": False,
        "origin": "Frozen live TCP captures; independently decoded Python stdlib expectations",
        "independent_verifiers": ["scripts/validation/verify_recent_auction.py",
                                 "scripts/validation/verify_recent_minutes_boundaries.py"],
        "server_trading_date_verified": False,
        "scope": "Nonempty wire/field regression only; requested date is not server date evidence",
        "fixtures": entries,
    }
    (output / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    return len(entries), sum(entry["records"] for entry in entries.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, action="append")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check", action="store_true", help="read-only hashes/source/independent fields verification")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables verification")
    if args.check:
        frames, records = check(args.output)
    else:
        if not args.source:
            parser.error("--source required when freezing")
        frames, records = freeze(args.source, args.output)
    print(json.dumps({"nonempty_frames": frames, "records": records, "output": str(args.output)}))


if __name__ == "__main__":
    main()
