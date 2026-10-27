#!/usr/bin/env python3
"""Build reproducible parser corpus from retained bytes; never edits source evidence."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OPCODES = {0x044D: 0, 0x02CF: 1, 0x02D0: 2, 0x06B9: 3, 0x052B: 4,
           0x051B: 5, 0x0FD1: 7, 0x0FEB: 8, 0x056A: 9, 0x0FFC: 10,
           0x054C: 11, 0x054B: 12, 0x0547: 13, 0x0452: 14, 0x0537: 15}


def build(out):
    if out.exists() or out.is_symlink() or not out.parent.is_dir():
        raise ValueError("requires a new output directory with an existing parent")
    out.mkdir(mode=0o700)
    for target in ("checked_parsers", "modern_parsers"):
        (out / target).mkdir()
    records, identities, total = [], set(), 0

    def add(target, source, original, seed, variant, selector=None):
        nonlocal total
        sha = hashlib.sha256(seed).hexdigest()
        if (target, sha) in identities:
            return
        if len(seed) > 65536 or total + len(seed) > 64 * 1024 * 1024 or len(records) >= 20000:
            raise ValueError("seed budget exceeded")
        identities.add((target, sha))
        path = target + "/" + sha
        (out / path).write_bytes(seed)
        total += len(seed)
        records.append({"path": path, "sha256": sha, "bytes": len(seed),
                        "source": source.relative_to(ROOT).as_posix(),
                        "source_sha256": hashlib.sha256(original).hexdigest(),
                        "variant": variant, "selector": selector})

    def payloads(source, original, body, target, selector=None):
        prefix = b"" if selector is None else bytes([selector])
        for label, value in [("original", body), ("truncate-last", body[:-1]),
                             ("prefix-2", body[:2]), ("prefix-16", body[:16]),
                             ("prefix-half", body[:len(body)//2]),
                             ("count-max", b"\xff\xff" + body[2:])]:
            add(target, source, original, prefix + value, label, selector)

    fixtures = ROOT / "crates/rustdx-core/tests/fixtures"
    candidates = sorted(fixtures.rglob("*.payload.bin"))
    candidates += sorted((fixtures / "daily-shares").glob("*.official.day"))
    candidates += sorted((fixtures / "kline").glob("*.response.hex"))
    candidates += sorted((fixtures / "protocol").glob("*.response.hex"))
    for source in candidates:
        original = source.read_bytes()
        body = bytes.fromhex(original.decode()) if source.suffix == ".hex" else original
        if len(body) > 65535:
            continue  # decoder harness bound; keep original untouched
        payloads(source, original, body, "checked_parsers")
        if source.name.endswith(".payload.bin"):
            request = source.with_name(source.name.replace(".payload.bin", ".request.bin"))
            if request.is_file():
                raw_request = request.read_bytes()
                selector = OPCODES.get(int.from_bytes(raw_request[10:12], "little"))
                if selector is not None:
                    payloads(source, original, body, "modern_parsers", selector)
    # Frame fuzzing receives compressed wire bytes, not an already inflated body.
    for source in sorted(fixtures.rglob("*.header.bin")):
        wire = source.with_name(source.name.replace(".header.bin", ".wire.bin"))
        if not wire.is_file():
            continue
        original = source.read_bytes()
        body = wire.read_bytes()
        if len(original) != 16 or len(body) > 65519:
            continue
        # Bind both original components in the manifest, including wire provenance.
        for label, header, value in [
            ("original", original, body),
            ("wire-truncated", original, body[:-1]),
            ("frame-length-max", original[:12] + b"\xff\xff\xff\xff", body),
            ("frame-empty", original, b""),
        ]:
            before = len(records)
            add("modern_parsers", source, original, b"\x10" + header + value, label, 16)
            if len(records) > before:
                records[-1]["wire_source"] = wire.relative_to(ROOT).as_posix()
                records[-1]["wire_sha256"] = hashlib.sha256(body).hexdigest()
    if not records or not all(any((out / target).iterdir())
                              for target in ("checked_parsers", "modern_parsers")):
        raise ValueError("empty corpus")
    manifest = {"schema_version": 1, "total_bytes": total, "seeds": records,
                "scope": "Frozen real bytes plus explicit mutations. Parser reachability only; not independent market qualification."}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return {"seeds": len(records), "bytes": total}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-directory", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(build(args.output_directory.absolute())))


if __name__ == "__main__":
    main()
