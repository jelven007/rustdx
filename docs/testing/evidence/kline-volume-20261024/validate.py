#!/usr/bin/env python3
"""Bounded read-only capture (--capture), or deterministic offline replay.

The two numerical functions below independently transcribe the pinned rustdx
and eltdx source formulas for this research experiment. No SDK is imported.
Default replay performs no network I/O and does not overwrite captured inputs.
"""
import argparse
import csv
import datetime as dt
import hashlib
import json
from pathlib import Path
import socket
import struct
import time
import zlib


ROOT = Path(__file__).resolve().parent
SYMBOLS = [
    ("sz000001", 0, "stock", 100),
    ("sh600000", 1, "stock", 100),
    ("sh510300", 1, "etf", 1000),
    ("sh000001", 1, "index", 100),
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, obj):
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + "\n")


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def signed32(value):
    return value if value < 0x80000000 else value - 0x100000000


def rustdx_volume(value):
    """src/tcp/helper.rs::vol_amount, including its negative-exponent branch."""
    value = signed32(value)
    logpoint = value >> 24
    hi, mid, lo = (value >> 16) & 255, (value >> 8) & 255, value & 255
    ecx, edx, esi, eax = [logpoint * 2 - n for n in (127, 134, 142, 150)]
    base = 1.0 / 2.0 ** (-ecx) if ecx < 0 else 2.0 ** ecx
    if hi > 128:
        high = 2.0 ** edx * 128.0 + (hi & 127) * 2.0 ** (edx + 1)
    elif edx >= 0:
        high = 2.0 ** edx * hi
    else:
        high = (1.0 / 2.0 ** edx) * hi
    scale = 1 if hi & 128 else 0
    return base + high + 2.0 ** (esi + scale) * mid + 2.0 ** (eax + scale) * lo


def eltdx_volume(value):
    """Pinned eltdx 3.3.0 unit.rs::get_volume."""
    if value == 0:
        return 0.0
    value = signed32(value)
    logpoint = value >> 24
    hi, mid, lo = (value >> 16) & 255, (value >> 8) & 255, value & 255
    base = 2.0 ** (logpoint * 2 - 127)
    high = base * (64 + (hi & 127)) / 64.0 if hi > 128 else base * hi / 128.0
    scale = 2.0 if hi & 128 else 1.0
    return base + high + base * mid / 32768.0 * scale + base * lo / 8388608.0 * scale


def decode(body, category, kind, requested):
    count = struct.unpack_from("<H", body)[0]
    if count > requested:
        raise ValueError("response count exceeds request")
    pos, base, rows = 2, 0, []

    def varint():
        nonlocal pos
        first = byte = body[pos]
        pos += 1
        value, shift, used = first & 63, 6, 1
        while byte & 128:
            if used >= 5:
                raise ValueError("price varint too long")
            byte = body[pos]
            pos += 1
            value += (byte & 127) << shift
            shift += 7
            used += 1
        return -value if first & 64 else value

    for _ in range(count):
        start = pos
        if category in (0, 1, 2, 3, 7, 8):
            packed, minutes = struct.unpack_from("<HH", body, pos)
            year, month, day = (packed >> 11) + 2004, packed % 2048 // 100, packed % 2048 % 100
        else:
            packed = struct.unpack_from("<I", body, pos)[0]
            year, month, day, minutes = packed // 10000, packed // 100 % 100, packed % 100, 900
        timestamp = dt.datetime(year, month, day, minutes // 60, minutes % 60)
        pos += 4
        op, cl, hi, lo = [varint() for _ in range(4)]
        base += op
        volume_offset = pos
        volume, amount = struct.unpack_from("<II", body, pos)
        pos += 8
        row = {
            "time": timestamp.isoformat(timespec="minutes"),
            "open": base / 1000, "close": (base + cl) / 1000,
            "high": (base + hi) / 1000, "low": (base + lo) / 1000,
            "volume_offset": volume_offset,
            "volume_encoding_u32": volume, "volume_encoding_le_hex": body[volume_offset:volume_offset + 4].hex(),
            "amount_encoding_u32": amount,
            "rustdx_vol": rustdx_volume(volume),
            "eltdx_wire": eltdx_volume(volume),
            "eltdx_lots": eltdx_volume(volume) / 100,
            "rustdx_amount": rustdx_volume(amount),
            "eltdx_amount": eltdx_volume(amount),
        }
        base += cl
        if kind == "index":
            row["up_count"], row["down_count"] = struct.unpack_from("<HH", body, pos)
            pos += 4
        row["record_hex"] = body[start:pos].hex()
        if category == 0 and kind != "index" and row["eltdx_wire"]:
            row["amount_div_wire"] = row["eltdx_amount"] / row["eltdx_wire"]
            # Amount is quantized; price proximity remains diagnostic, not a theorem.
            row["vwap_in_low_high_with_0_01_tolerance"] = (
                row["low"] - .01 <= row["amount_div_wire"] <= row["high"] + .01)
        rows.append(row)
    if pos != len(body):
        raise ValueError(f"trailing response bytes: {len(body) - pos}")
    return sorted(rows, key=lambda row: row["time"])


def make_request(symbol, market, category, length, count, template):
    data = struct.pack("<H6sHHHH", market, symbol[2:].encode(), category, 1, 0, count)
    # Both layouts have zero adjust/anchor. Hold msg_id/control fixed to isolate
    # length and category. The 54-byte result is RequestFrame::encode with that id.
    data += bytes(length - 12 - len(data))
    head = bytearray(template[:12])
    struct.pack_into("<HH", head, 6, len(data) + 2, len(data) + 2)
    return bytes(head) + data


def capture(root, template_path):
    wire = root / "wire"
    wire.mkdir()  # Refuse to overwrite prior capture.
    template = bytes.fromhex(template_path.read_text())
    deadline = time.monotonic() + 120
    manifest = {"started_utc": now(), "global_budget_seconds": 120,
                "frame_budget_seconds": 15, "max_simultaneous_connections": 1,
                "attempts": [], "cases": [],
                "request_template_sha256": sha(template)}

    def exchange(sock, name, request):
        frame_deadline = min(deadline, time.monotonic() + 15)
        received = bytearray()
        (wire / f"{name}.request.bin").write_bytes(request)
        meta = {"name": name, "started_utc": now(), "request_sha256": sha(request),
                "request_length": len(request), "endpoint": list(sock.getpeername())}

        def exact(size):
            block = bytearray()
            while len(block) < size:
                remaining = frame_deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("absolute frame or capture deadline")
                sock.settimeout(remaining)
                chunk = sock.recv(size - len(block))
                if not chunk:
                    raise EOFError("server closed connection")
                block.extend(chunk)
                received.extend(chunk)
            return bytes(block)

        try:
            sock.settimeout(max(.001, frame_deadline - time.monotonic()))
            sock.sendall(request)
            header = exact(16)
            if header[:4] != bytes.fromhex("b1cb7400"):
                raise ValueError("unexpected response prefix")
            msgid, opcode, compressed, length = (
                struct.unpack_from("<I", header, 5)[0], *struct.unpack_from("<HHH", header, 10))
            if msgid != struct.unpack_from("<I", request, 1)[0] or opcode != struct.unpack_from("<H", request, 10)[0]:
                raise ValueError("response identity differs from request")
            payload = exact(compressed)
            body = zlib.decompress(payload) if compressed != length else payload
            if len(body) != length:
                raise ValueError("decompressed response size differs")
            (wire / f"{name}.body.bin").write_bytes(body)
            meta.update(body_sha256=sha(body), body_length=len(body),
                        response_message_id=msgid, opcode=opcode, compressed_length=compressed)
        except Exception as exc:
            meta.update(error=f"{type(exc).__name__}: {exc}")
            raise
        finally:
            (wire / f"{name}.frame.bin").write_bytes(received)
            meta.update(finished_utc=now(), frame_sha256=sha(received), frame_length=len(received))
            manifest["attempts"].append(meta)
            write_json(root / "capture.json", manifest)
        return body

    for endpoint in [("115.238.56.198", 7709), ("119.147.212.81", 7709)]:
        try:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("capture global budget exhausted")
            with socket.create_connection(endpoint, timeout=min(8, remaining)) as sock:
                exchange(sock, f"handshake-{len(manifest['attempts'])}", bytes.fromhex("0c0218930001030003000d0001"))
                for symbol, market, kind, _ in SYMBOLS:
                    for category in (9, 4, 0):
                        for length in (38, 54):
                            count = 96 if category == 0 else 10
                            name = f"{symbol}-c{category}-n{length}"
                            body = exchange(sock, name, make_request(
                                symbol, market, category, length, count, template))
                            rows = decode(body, category, kind, count)
                            manifest["cases"].append({
                                "name": name, "symbol": symbol, "market": market,
                                "kind": kind, "category": category, "parameter": 1,
                                "count": count, "start": 0, "adjust": 0, "anchor": 0,
                                "request_length": length, "rows": len(rows)})
                            write_json(root / "capture.json", manifest)
                            print(name, len(rows), "bars", flush=True)
                break
        except Exception as exc:
            manifest.setdefault("connection_errors", []).append({
                "endpoint": list(endpoint), "utc": now(), "error": f"{type(exc).__name__}: {exc}"})
            # Do not recapture or overwrite partially successful cases on another
            # endpoint: a matrix spanning servers would break the experiment.
            if manifest["cases"]:
                break
    manifest["finished_utc"] = now()
    write_json(root / "capture.json", manifest)


def replay(root):
    manifest = json.loads((root / "capture.json").read_text())
    provenance = json.loads((root / "provenance.json").read_text())
    workspace = ROOT.parents[3]
    for path, expected in provenance["sources_sha256"].items():
        if sha((workspace / path).read_bytes()) != expected:
            raise ValueError(f"pinned source differs: {path}")
    for entry in provenance["files"]:
        if sha((root / entry["path"]).read_bytes()) != entry["sha256"]:
            raise ValueError(f"supporting input hash differs: {entry['path']}")
    for attempt in manifest["attempts"]:
        for suffix, key in (("request", "request"), ("frame", "frame"), ("body", "body")):
            if key + "_sha256" in attempt:
                path = root / "wire" / f"{attempt['name']}.{suffix}.bin"
                if sha(path.read_bytes()) != attempt[key + "_sha256"]:
                    raise ValueError(f"wire input hash differs: {path}")
    native_expected_rows = 0
    for name, category, kind in [("stock-day", 9, "stock"), ("index-day", 9, "index"),
                                  ("stock-minute", 0, "stock")]:
        body = bytes.fromhex((root / "native" / f"{name}.response.hex").read_text())
        rows = decode(body, category, kind, 3)
        with (root / "native" / f"{name}.expected.csv").open(newline="") as file:
            expected = list(csv.DictReader(file))
        if len(rows) != len(expected):
            raise ValueError("native/pytdx row counts differ")
        for row, reference in zip(rows, expected):
            date = dt.datetime(*[int(reference[k]) for k in ("year", "month", "day", "hour", "minute")])
            if row["time"] != date.isoformat(timespec="minutes"):
                raise ValueError("native/pytdx timestamps differ")
            for left, right in [("open", "open"), ("high", "high"), ("low", "low"), ("close", "close"),
                                ("rustdx_vol", "vol"), ("rustdx_amount", "amount")]:
                if row[left] != float(reference[right]):
                    raise ValueError(f"native/pytdx field differs: {name}, {left}")
            for field in ("up_count", "down_count"):
                if field in row and row[field] != int(reference[field]):
                    raise ValueError("native/pytdx index breadth differs")
            native_expected_rows += 1
    official = {}
    for symbol, _, kind, divisor in SYMBOLS:
        data = (root / "official-day" / f"{symbol}.day").read_bytes()
        if len(data) % 32:
            raise ValueError("day file record boundary")
        rows = {}
        for offset in range(0, len(data), 32):
            date, op, hi, lo, cl, amount, vol, reserved = struct.unpack_from("<IIIIIfII", data, offset)
            date = str(date)
            rows[f"{date[:4]}-{date[4:6]}-{date[6:]}"] = {
                "open": op / divisor, "high": hi / divisor, "low": lo / divisor,
                "close": cl / divisor, "amount": amount, "volume": vol,
                "record_offset": offset, "reserved": reserved}
        official[symbol] = rows
    decoded, comparisons, pairs, minute_days = {}, [], [], []
    numeric_differences, minute_vwap = [], []
    for case in manifest["cases"]:
        name, symbol, category = case["name"], case["symbol"], case["category"]
        request = (root / "wire" / f"{name}.request.bin").read_bytes()
        template = bytes.fromhex((root / "native" / "stock-day.request.hex").read_text())
        if request != make_request(symbol, case["market"], category, case["request_length"],
                                   case["count"], template):
            raise ValueError(f"request differs from controlled parameters: {name}")
        body = (root / "wire" / f"{name}.body.bin").read_bytes()
        rows = decode(body, category, case["kind"], case["count"])
        decoded[name] = rows
        for row in rows:
            for left, right in (("rustdx_vol", "eltdx_wire"), ("rustdx_amount", "eltdx_amount")):
                if row[left] != row[right]:
                    numeric_differences.append({"case": name, "time": row["time"], "field": left,
                                                "rustdx": row[left], "eltdx": row[right]})
            date = row["time"][:10]
            if category != 0 and date in official[symbol]:
                day = official[symbol][date]
                comparisons.append({
                    "case": name, "symbol": symbol, "date": date, "category": category,
                    "request_length": case["request_length"],
                    "ohlc_matches": all(day[f] == row[f] for f in ("open", "high", "low", "close")),
                    "amount_wire": row["eltdx_amount"], "amount_day": day["amount"],
                    "amount_difference": row["eltdx_amount"] - day["amount"],
                    "volume_wire": row["eltdx_wire"], "eltdx_lots": row["eltdx_lots"],
                    "volume_day": day["volume"],
                    "day_over_wire": day["volume"] / row["eltdx_wire"] if row["eltdx_wire"] else None,
                    "wire_minus_day": row["eltdx_wire"] - day["volume"],
                    "wire_times_100_minus_day": row["eltdx_wire"] * 100 - day["volume"]})
            if "amount_div_wire" in row:
                minute_vwap.append({"case": name, "time": row["time"],
                                    "within_price_range": row["vwap_in_low_high_with_0_01_tolerance"]})
        if category == 0:
            dates = sorted({r["time"][:10] for r in rows} & official[symbol].keys())
            for date in dates:
                group = [r for r in rows if r["time"].startswith(date)]
                volume = sum(r["eltdx_wire"] for r in group)
                minute_days.append({
                    "case": name, "symbol": symbol, "date": date, "bars": len(group),
                    "first": group[0]["time"], "last": group[-1]["time"],
                    "minute_wire_sum": volume, "day_volume": official[symbol][date]["volume"],
                    "sum_minus_day": volume - official[symbol][date]["volume"],
                    "standard_48_timestamps_match": [r["time"][-5:] for r in group] == [
                        f"{minute // 60:02}:{minute % 60:02}"
                        for minute in list(range(575, 691, 5)) + list(range(785, 901, 5))],
                    "minute_amount_sum": sum(r["eltdx_amount"] for r in group),
                    "day_amount": official[symbol][date]["amount"]})
    for symbol, _, _, _ in SYMBOLS:
        for category in (9, 4, 0):
            a, b = [f"{symbol}-c{category}-n{length}" for length in (38, 54)]
            if a in decoded and b in decoded:
                pairs.append({
                    "symbol": symbol, "category": category, "rows": len(decoded[a]),
                    "bodies_identical": (root / "wire" / f"{a}.body.bin").read_bytes() == (
                        root / "wire" / f"{b}.body.bin").read_bytes(),
                    "decoded_rows_identical": decoded[a] == decoded[b]})
    result = {
        "scope": "four instruments, one endpoint, categories 9/4/0, lengths 38/54; unadjusted; no SDK execution",
        "capture_started_utc": manifest["started_utc"], "capture_finished_utc": manifest["finished_utc"],
        "cases": manifest["cases"], "decoded": decoded, "daily_comparisons": comparisons,
        "length_pairs": pairs, "minute_day_sums": minute_days,
        "minute_vwap_checks": minute_vwap, "numeric_formula_differences": numeric_differences,
        "synthetic_formula_edges": [
            {"encoding_u32": value, "rustdx": rustdx_volume(value), "eltdx": eltdx_volume(value)}
            for value in (0, 0x41800000, 1235775464, 1326643033)],
        "counts": {"cases": len(decoded), "decoded_rows": sum(map(len, decoded.values())),
                   "matched_daily_rows": len(comparisons), "numeric_differences": len(numeric_differences),
                   "native_pytdx_expected_rows": native_expected_rows,
                   "length_pairs": len(pairs),
                   "minute_vwap_rows": len(minute_vwap),
                   "minute_vwap_outside": sum(not r["within_price_range"] for r in minute_vwap)},
    }
    write_json(root / "results.json", result)
    print(json.dumps(result["counts"], ensure_ascii=False, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--capture", action="store_true", help="explicitly enable read-only TCP capture")
    parser.add_argument("--request-template", type=Path)
    args = parser.parse_args()
    if args.capture:
        if args.request_template is None:
            parser.error("--capture requires --request-template")
        capture(args.root, args.request_template)
    else:
        replay(args.root)


if __name__ == "__main__":
    main()
