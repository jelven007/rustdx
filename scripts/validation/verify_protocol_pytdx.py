#!/usr/bin/env python3
"""Capture bounded real TDX samples and reproduce independent pytdx expectations.

Requires six and the separately downloaded reference sources. No executable
code is downloaded here. Without --capture, only stored responses are decoded.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import socket
import sys
import zlib

from verify_kline_pytdx import SOURCE_HASHES, REFERENCE_COMMIT

HASHES = dict(SOURCE_HASHES, **{
    "pytdx/parser/get_security_quotes.py": "04af067059ad88f66cab0b712478be3ed5e3eed32c72be5b67059a5b2f2e0c24",
    "pytdx/parser/get_finance_info.py": "91a881051b9fb4aaadb0c0d2d9581b10b2c8dddc83ead8a056361546a4f9aedf",
    "pytdx/parser/get_transaction_data.py": "9c66930a2b73b4dc8e5af1ffc49914c00c1c334e2be3cd3facfeb60d19f7d353",
    "pytdx/parser/get_history_transaction_data.py": "207c496eb1d35e2b90a8665439c0d56303d5b03375b17f4a484b6af0c2c1ff62",
})


def sha(data):
    return hashlib.sha256(data).hexdigest()


def exchange(conn, request):
    def exact(size):
        result = bytearray()
        while len(result) < size:
            part = conn.recv(size - len(result))
            if not part:
                raise EOFError("truncated TDX frame")
            result.extend(part)
        return bytes(result)
    conn.sendall(request)
    header = exact(16)
    compressed = int.from_bytes(header[12:14], "little")
    uncompressed = int.from_bytes(header[14:16], "little")
    body = exact(compressed)
    if compressed != uncompressed:
        body = zlib.decompress(body)
    if len(body) != uncompressed:
        raise ValueError("TDX length mismatch")
    return body


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pytdx-source", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--capture", action="store_true")
    parser.add_argument("--history-date", type=int, default=20261022)
    args = parser.parse_args()
    for name, expected in HASHES.items():
        if sha((args.pytdx_source / name).read_bytes()) != expected:
            raise ValueError(f"unverified reference: {name}")
    sys.path.insert(0, str(args.pytdx_source.resolve()))
    from pytdx.parser.get_security_quotes import GetSecurityQuotesCmd
    from pytdx.parser.get_finance_info import GetFinanceInfo
    from pytdx.parser.get_transaction_data import GetTransactionData
    from pytdx.parser.get_history_transaction_data import GetHistoryTransactionData

    cases = [
        ("quotes", GetSecurityQuotesCmd(None), ([(0, "000001"), (1, "600000")],)),
        ("finance", GetFinanceInfo(None), (0, "000001")),
        ("transaction", GetTransactionData(None), (0, "000001", 0, 3)),
        ("history-transaction", GetHistoryTransactionData(None),
         (0, "000001", 0, 3, args.history_date)),
    ]
    args.fixtures.mkdir(parents=True, exist_ok=True)
    endpoint = None
    if args.capture:
        for host in ["115.238.56.198", "180.153.18.170", "110.41.147.114"]:
            try:
                with socket.create_connection((host, 7709), timeout=8) as conn:
                    conn.settimeout(8)
                    exchange(conn, bytes.fromhex("0c0218930001030003000d0001"))
                    captured = []
                    for name, decoder, params in cases:
                        decoder.setParams(*params)
                        captured.append((name, bytes(decoder.send_pkg),
                                         exchange(conn, decoder.send_pkg)))
                for name, request, response in captured:
                    (args.fixtures / f"{name}.request.hex").write_text(request.hex() + "\n")
                    (args.fixtures / f"{name}.response.hex").write_text(response.hex() + "\n")
                endpoint = f"{host}:7709"
                break
            except (OSError, EOFError, ValueError) as error:
                print(host, str(error), flush=True)
        if endpoint is None:
            raise RuntimeError("no complete sample captured")
    evidence = []
    for name, decoder, params in cases:
        decoder.setParams(*params)
        request = bytes.fromhex((args.fixtures / f"{name}.request.hex").read_text())
        response = bytes.fromhex((args.fixtures / f"{name}.response.hex").read_text())
        if request != decoder.send_pkg:
            raise ValueError(f"{name}: independent encoding differs")
        rows = decoder.parseResponse(response)
        if name == "finance":
            rows = [rows]
        if not rows:
            raise ValueError(f"{name}: empty sample")
        if name == "quotes":
            for row in rows:
                row["speed"] = row.pop("reversed_bytes9")
                row["change"] = row["price"] - row["last_close"]
                row["change_percent"] = (row["change"] / row["last_close"] * 100
                                         if row["last_close"] > 0 else 0.0)
                for level in range(1, 6):
                    for side in ["bid", "ask"]:
                        row[f"{side}{level}_vol"] = row.pop(f"{side}_vol{level}")
                for key in list(row):
                    if key in ["market", "active1", "active2"] or key.startswith("reversed_"):
                        del row[key]
                # Rust pads one-digit hours; compare the same semantic time.
                row["servertime"] = row["servertime"].zfill(12)
        if name == "history-transaction":
            for row in rows:
                row["num"] = 0
        path = args.fixtures / f"{name}.expected.csv"
        with path.open("w", newline="") as output:
            writer = csv.DictWriter(output, fieldnames=list(rows[0]), lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)
        evidence.append({
            "case": name, "rows": len(rows), "request_sha256": sha(request),
            "response_sha256": sha(response), "expected_csv_sha256": sha(path.read_bytes()),
        })
        print(f"{name}: {len(rows)} independent rows", flush=True)
    manifest = {
        "reference": "https://github.com/rainx/pytdx/tree/" + REFERENCE_COMMIT,
        "reference_sources_sha256": HASHES, "cases": evidence,
        "scope": "Two securities' quotes, one finance record, three current and "
                 "three historical ticks; all exposed fields, not full market.",
        "history_date": args.history_date,
    }
    if args.capture:
        from datetime import datetime, timezone
        manifest.update(captured_at=datetime.now(timezone.utc).isoformat(), endpoint=endpoint)
    else:
        previous = args.fixtures / "manifest.json"
        if previous.exists():
            saved = json.loads(previous.read_text())
            for key in ["captured_at", "endpoint"]:
                if key in saved:
                    manifest[key] = saved[key]
    (args.fixtures / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
