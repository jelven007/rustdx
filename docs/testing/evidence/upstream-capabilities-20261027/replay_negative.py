#!/usr/bin/env python3
"""Synthetic corruption controls for replay.py; never online evidence."""
import argparse
import json
from pathlib import Path
from unittest.mock import patch
import zipfile

from replay import replay


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables validation")
    source = Path(__file__).resolve().parents[4]
    captures = Path(__file__).with_name("raw-captures.zip")
    original_read = zipfile.ZipFile.read
    results = []
    for fault in ("header_magic", "message_id", "request_selector", "dto_price"):
        changed = [False]

        def read(archive, name, pwd=None):
            data = original_read(archive, name, pwd)
            if changed[0] or not name.startswith("window-hz-boundary/"):
                return data
            if fault in ("header_magic", "message_id") and name.endswith(".header.bin"):
                value = bytearray(data)
                value[0 if fault == "header_magic" else 5] ^= 1
                changed[0] = True
                return bytes(value)
            if fault == "request_selector" and name.endswith(".request.bin") and len(data) == 23:
                value = bytearray(data)
                value[12] ^= 1
                changed[0] = True
                return bytes(value)
            if fault == "dto_price" and "/recent-" in name:
                value = json.loads(data)
                if value["page"]["records"]:
                    value["page"]["records"][0]["price_raw"] += 1
                    changed[0] = True
                    return json.dumps(value).encode()
            return data

        with patch.object(zipfile.ZipFile, "read", read):
            try:
                replay(source, captures)
            except (AssertionError, KeyError, ValueError) as error:
                assert changed[0], "fault was never injected"
                results.append(dict(fault=fault, rejected=True, error_type=type(error).__name__))
            else:
                raise AssertionError(f"corruption accepted: {fault}")
    result = dict(status="passed", source="synthetic_corruption", online_evidence=False,
                  cases=results, original_capture_bytes_modified=False)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
