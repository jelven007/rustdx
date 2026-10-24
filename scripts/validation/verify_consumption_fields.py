#!/usr/bin/env python3
"""Verify required finance fields against pinned raw/issuer evidence, without network I/O."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct


def read(root, record, limit=32 * 1024 * 1024):
    original = root / record["path"]
    if original.is_symlink():
        raise ValueError("evidence symlinks are not allowed")
    path = original.resolve()
    path.relative_to(root)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
        raise ValueError("evidence must be a bounded regular file")
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != record["sha256"]:
        raise ValueError(f"evidence digest differs: {record['path']}")
    return data


def verify(root, manifest):
    if manifest["schema_version"] != 1:
        raise ValueError("unsupported consumption matrix")
    results = []
    entries = manifest["entries"]
    if not 1 <= len(entries) <= 10000:
        raise ValueError("entry count budget")
    for entry in entries:
        scope = entry["scope"]
        if scope["command"] != "0x0010" or scope["period"] != "finance_report":
            raise ValueError("this verifier only qualifies scoped 0010 finance fields")
        raw_text = read(root, entry["raw"]).decode().strip()
        raw = bytes.fromhex(raw_text)
        reference = json.loads(read(root, entry["reference"]))
        authorities = [read(root, item) for item in entry["authority"]]
        if not authorities:
            raise ValueError("independent issuer bytes are required")
        if len(raw) != 145 or struct.unpack_from("<H", raw)[0] != 1:
            raise ValueError("finance response shape")
        if hashlib.sha256(raw).hexdigest() != reference["response_sha256"]:
            raise ValueError("reference applies to different raw response")
        identity = {"market": raw[2], "code": raw[3:9].decode(),
                    "updated_date": struct.unpack_from("<I", raw, 17)[0],
                    "report_period": reference["report_period"]}
        if any(scope[key] != value or reference[key] != value for key, value in identity.items()):
            raise ValueError("security/update/report period changed")
        if scope["date_basis"] != "independent_issuer_report":
            raise ValueError("date basis is not independently scoped")
        fields = {row["field"]: row for row in reference["fields"]}
        required = entry["required_fields"]
        if not required or len(required) != len(set(required)):
            raise ValueError("required fields must be nonempty and unique")
        checked = []
        for name in required:
            if name not in fields:
                raise ValueError(f"required field unqualified: {name}")
            row = fields[name]
            offset = row["offset"]
            if offset != 0 and (offset < 16 or offset > 132 or offset % 4):
                raise ValueError("unregistered finance slot")
            value = struct.unpack_from("<f", raw, 9 + offset)[0]
            bits = struct.unpack_from("<I", raw, 9 + offset)[0]
            expected_bits = struct.unpack("<I", struct.pack("<f", row["raw"]))[0]
            if not math.isfinite(value) or bits != expected_bits:
                raise ValueError("raw field changed")
            normalized = value * row["multiplier"]
            if not math.isfinite(normalized) or not math.isfinite(row["official"]):
                raise ValueError("nonfinite reference value")
            if row["tolerance"] < 0 or abs(normalized - row["official"]) > row["tolerance"]:
                raise ValueError("independent field comparison failed")
            checked.append({"field": name, "raw_bits": bits, "normalized": normalized,
                            "official": row["official"], "tolerance": row["tolerance"]})
        results.append({"scope": scope, "status": "scoped_reference_checked",
                        "required_fields": checked, "raw": entry["raw"],
                        "reference": entry["reference"], "authority": entry["authority"]})
    return {"schema_version": 1, "status": "passed", "entries": results,
            "scope": "Pinned caller evidence only; document authenticity and mappings remain maintainer responsibilities; no other symbol/period/field is qualified."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("use a new output; frozen evidence must not be overwritten")
    result = verify(args.root.resolve(), json.loads(args.matrix.read_text()))
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": "passed", "entries": len(result["entries"])}))


if __name__ == "__main__":
    main()
