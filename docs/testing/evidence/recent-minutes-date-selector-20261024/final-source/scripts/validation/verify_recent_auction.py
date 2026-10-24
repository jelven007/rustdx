#!/usr/bin/env python3
"""Independent offline verification of live 0feb/056a and dated control frames."""
import argparse
import collections
import datetime as dt
import hashlib
import json
import math
from pathlib import Path
import struct
import zlib

from verify_nine_commands import Reader, equal, packed_amount, quote
from recent_minutes_selector import date_from_selector


def sign(value):
    return (value > 0) - (value < 0)


def floating(bits):
    value = struct.unpack("<f", struct.pack("<I", bits))[0]
    assert math.isfinite(value)
    return value


def read_frame(path, endpoint):
    stem = str(path).removesuffix(".request.bin")
    request = path.read_bytes()
    header = Path(stem + ".header.bin").read_bytes()
    wire = Path(stem + ".wire.bin").read_bytes()
    assert len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400")
    packed, unpacked = struct.unpack_from("<HH", header, 12)
    assert len(wire) == packed and len(request) >= 12
    if packed != unpacked:
        inflater = zlib.decompressobj()
        payload = inflater.decompress(wire, unpacked + 1)
        assert inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail
    else:
        payload = wire
    assert len(payload) == unpacked
    assert payload == Path(stem + ".payload.bin").read_bytes()
    assert Path(stem + ".endpoint.txt").read_text() == endpoint
    stamp = dt.datetime.fromisoformat(Path(stem + ".utc.txt").read_text())
    assert stamp.tzinfo is not None
    opcode = struct.unpack_from("<H", request, 10)[0]
    assert struct.unpack_from("<H", header, 10)[0] == opcode
    assert request[6:8] == request[8:10]
    assert struct.unpack_from("<H", request, 6)[0] == len(request) - 10
    assert request[1:5] == header[5:9]
    return opcode, request, payload


def recent(reader, divisor):
    count, previous, opening = reader.scalar("H"), reader.scalar("I"), reader.scalar("I")
    assert count <= 240
    floating(previous)
    floating(opening)
    points, base = [], (0, 0)
    for index in range(count):
        pf, af, volume = reader.varint(), reader.varint(), reader.varint()
        price, average = base[0] + pf, base[1] + af
        assert 0 <= price < 2**63 and 0 <= average < 2**63 and volume >= 0
        if index == 0:
            base = price, average
        minute = 571 + index if index < 120 else 781 + index - 120
        points.append({
            "index": index, "time_label": f"{minute // 60:02}:{minute % 60:02}",
            "price_field_raw": pf, "average_field_raw": af, "volume_raw": volume,
            "price_raw": price, "average_raw": average,
            "price": price / divisor, "average_price": average / (100 * divisor),
        })
    return {
        "previous_close_bits": previous, "open_bits": opening,
        "previous_close": floating(previous), "open_price": floating(opening),
        "server_trading_date": None, "page": {"records": points},
    }


def auction(reader, start, limit):
    count = reader.scalar("H")
    assert count <= limit <= 4095 and len(reader.data) == 2 + count * 16
    points = []
    for index in range(count):
        minute, bits = reader.scalar("H"), reader.scalar("I")
        matched, unmatched = reader.scalar("I"), reader.scalar("i")
        reserved, second = reader.scalar("B"), reader.scalar("B")
        assert minute < 1440 and second < 60 and floating(bits) >= 0
        points.append({
            "index": start + index, "minute_raw": minute, "second_raw": second,
            "time_label": f"{minute // 60:02}:{minute % 60:02}:{second:02}",
            "price": floating(bits), "price_bits": bits,
            "matched_volume_raw": matched, "unmatched_signed_raw": unmatched,
            "unmatched_volume_raw": abs(unmatched), "unmatched_sign": sign(unmatched),
            "reserved_raw": reserved,
        })
    return points


def bars(reader, code, limit):
    count, base, result = reader.scalar("H"), 0, []
    assert count <= limit
    for _ in range(count):
        date = reader.scalar("I")
        day = dt.datetime.strptime(str(date), "%Y%m%d").date()
        opening, closing, high, low = [reader.varint() for _ in range(4)]
        base += opening
        volume, amount = reader.scalar("I"), reader.scalar("I")
        result.append({
            "dt": {"year": day.year, "month": day.month, "day": day.day,
                   "hour": 15, "minute": 0},
            "code": code, "open": base / 1000, "close": (base + closing) / 1000,
            "high": (base + high) / 1000, "low": (base + low) / 1000,
            "vol": packed_amount(volume), "amount": floating(amount),
        })
        base += closing
    return sorted(result, key=lambda r: tuple(r["dt"].values()))


def verify(root):
    summary = json.loads((root / "summary.json").read_text())
    assert summary["source"] == "live_tcp" and summary["status"] == "captured"
    assert summary["request_spacing_ms"] >= 250
    assert summary["total_timeout_seconds"] <= 180
    assert all(q["decoded"] for q in summary["queries"]), "a query failed"
    date_schema = summary.get("recent_minutes_date_schema_version", 1)
    assert date_schema in (1, 2)
    decimals, directory_counts = {}, collections.Counter()
    counts, records = collections.Counter(), collections.Counter()
    recent_rows, auction_rows, controls, dated_bars = {}, {}, {}, {}
    snapshots = {}
    for path in sorted(root.glob("frame-*.request.bin")):
        opcode, request, payload = read_frame(path, summary["endpoint"])
        counts[f"0x{opcode:04x}"] += 1
        reader = Reader(payload)
        if opcode == 0x044D:
            market, start, limit = struct.unpack_from("<HII", request, 12)
            count = reader.scalar("H")
            assert count <= limit <= 1600 and start == directory_counts[market]
            dto = json.loads((root / f"directory-{market}.json").read_text())
            for index in range(count):
                row = reader.take(37)
                code, decimal = row[:6].decode("ascii"), row[28]
                assert (market, code) not in decimals
                decimals[(market, code)] = decimal
                equal(dto[start + index], {"code": code, "decimal": decimal,
                    "volume_unit": struct.unpack_from("<H", row, 6)[0],
                    "previous_close": floating(struct.unpack_from("<I", row, 29)[0])})
                directory_counts[market] += 1
        elif opcode in {0x0FEB, 0x0FB4}:
            raw = struct.unpack_from("<I", request, 12)[0]
            day = date_from_selector(raw, date_schema) if opcode == 0x0FEB else raw
            market, code = request[16], request[17:23].decode("ascii")
            divisor = 10 ** decimals[(market, code)]
            if opcode == 0x0FEB:
                dto = json.loads((root / f"recent-{day}-{market}-{code}.json").read_text())
                decoded = recent(reader, divisor)
                equal(dto, {**decoded, "requested_date": day, "date_selector_raw": raw,
                    "scale": {"decimal": decimals[(market, code)], "divisor": divisor}})
                recent_rows[(market, code, day)] = decoded
                records["0x0feb"] += len(decoded["page"]["records"])
            else:
                dto = json.loads((root / f"history-minute-{day}-{market}-{code}.json").read_text())
                assert bytes(dto["payload"]) == payload
                count, previous, base, points = reader.scalar("H"), reader.scalar("I"), 0, []
                floating(previous)
                assert count <= 240
                for _ in range(count):
                    base += reader.varint()
                    auxiliary, volume = reader.varint(), reader.varint()
                    assert base >= 0 and volume >= 0
                    points.append({"price_raw": base, "price": base / divisor,
                                   "volume_raw": volume, "auxiliary_raw": auxiliary})
                controls[(market, code, day)] = points
                records["0x0fb4"] += count
        elif opcode == 0x056A:
            market, code = request[12], request[14:20].decode("ascii")
            day, mode, reserved, start, limit = struct.unpack_from("<IIIII", request, 20)
            assert mode == 3 and reserved == 0
            points = auction(reader, start, limit)
            dto = json.loads((root / f"auction-{day}-{market}-{code}.json").read_text())
            equal(dto, {"requested_date": day or None, "server_trading_date": None,
                        "process_snapshots": True, "page": {"records": points}})
            auction_rows[(market, code, day)] = points
            records["0x056a"] += len(points)
        elif opcode == 0x052D:
            market, code = request[12], request[14:20].decode("ascii")
            assert struct.unpack_from("<H", request, 20)[0] == 9
            rows = bars(reader, code, struct.unpack_from("<H", request, 26)[0])
            equal(json.loads((root / f"bars-{market}-{code}.json").read_text()), rows)
            for row in rows:
                d = row["dt"]
                dated_bars[(market, code, d["year"] * 10000 + d["month"] * 100 + d["day"])] = row
            records["0x052d"] += len(rows)
        elif opcode == 0x054C:
            dto = json.loads((root / "snapshots.json").read_text())
            assert reader.scalar("H") == dto["header_raw"]
            n = reader.scalar("H")
            decoded = []
            for _ in range(n):
                row = quote(reader, decimals, False)
                row["tail_raw"] = list(reader.take(56))
                snapshots[(row["market_raw"], row["code"])] = row
                decoded.append(row)
            equal(dto["page"]["records"], decoded)
        elif opcode == 0x0004:
            dto = json.loads((root / "heartbeat.json").read_text())
            equal(dto, {"reserved_raw": list(reader.take(6)), "server_date_raw": reader.scalar("I"),
                        "raw_payload": list(payload)})
        else:
            assert opcode in {0x000D, 0x044E}, f"unexpected opcode {opcode:x}"
            reader.take(len(payload))
        assert reader.offset == len(payload), "unconsumed payload"
    assert sum(counts.values()) == summary["wire_frames"]
    selected = {(m, c): d for m, c, d in json.loads((root / "precision.json").read_text())}
    assert all(decimals[key] == value for key, value in selected.items())
    cross_recent, nonempty, recent_bar_checks = [], [], []
    for key, series in sorted(recent_rows.items()):
        points = series["page"]["records"]
        if not points:
            continue
        market, code, day = key
        nonempty.append({"market": market, "code": code, "requested_date": day,
                        "points": len(points), "previous_close": series["previous_close"],
                        "open": series["open_price"], "first_price": points[0]["price"],
                        "last_price": points[-1]["price"],
                        "volume_sum_raw": sum(p["volume_raw"] for p in points)})
        if key in controls:
            control = controls[key]
            pairs = list(zip(points, control))
            mismatches = [{
                "index": i, "recent_price_raw": left["price_raw"],
                "control_price_raw": right["price_raw"],
                "recent_volume_raw": left["volume_raw"],
                "control_volume_raw": right["volume_raw"],
            } for i, (left, right) in enumerate(pairs)
                if (left["price_raw"], left["volume_raw"]) != (right["price_raw"], right["volume_raw"])]
            cross_recent.append({"market": market, "code": code, "requested_date": day,
                "recent_points": len(points), "control_points": len(control),
                "same_price_points": sum(a["price_raw"] == b["price_raw"] for a, b in pairs),
                "same_volume_points": sum(a["volume_raw"] == b["volume_raw"] for a, b in pairs),
                "matches_control": len(points) == len(control) and not mismatches,
                "mismatched_points": len(mismatches), "first_mismatches": mismatches[:3]})
        if key in dated_bars:
            bar = dated_bars[key]
            tick = 10 ** -decimals[(market, code)]
            recent_bar_checks.append({
                "market": market, "code": code, "requested_date": day,
                "recent_open": series["open_price"], "dated_daily_open": bar["open"],
                "recent_last_price": points[-1]["price"], "dated_daily_close": bar["close"],
                "open_matches": abs(series["open_price"] - bar["open"]) < tick / 2,
                "close_matches": abs(points[-1]["price"] - bar["close"]) < tick / 2,
            })
    date_checks, final_price_checks = [], []
    for market, code in selected:
        days = sorted(day for m, c, day in auction_rows if (m, c) == (market, code) and day)
        fingerprints = {}
        for day in days:
            points = auction_rows[(market, code, day)]
            if not points:
                continue
            fingerprints[day] = hashlib.sha256(json.dumps(points, sort_keys=True).encode()).hexdigest()
            if (market, code, day) in dated_bars:
                final = points[-1]
                bar = dated_bars[(market, code, day)]
                tick = 10 ** -decimals[(market, code)]
                final_price_checks.append({
                    "market": market, "code": code, "requested_date": day,
                    "auction_time": final["time_label"], "auction_price": final["price"],
                    "dated_daily_close": bar["close"],
                    "same_price_within_half_tick": abs(final["price"] - bar["close"]) < tick / 2,
                })
        distinct = len(set(fingerprints.values()))
        date_checks.append({"market": market, "code": code,
                            "nonempty_dates": list(fingerprints), "distinct_payloads": distinct,
                            "date_selection_changed_payload": distinct >= 2,
                            "empty_requested_dates": [d for d in days if not auction_rows[(market, code, d)]]})
        if (market, code, 0) in auction_rows and fingerprints:
            latest = max(fingerprints)
            assert auction_rows[(market, code, 0)] == auction_rows[(market, code, latest)]
    direction = []
    heartbeat = json.loads((root / "heartbeat.json").read_text())
    quote_observation_date = heartbeat["server_date_raw"] if heartbeat["server_date"] else None
    for key, snapshot in snapshots.items():
        candidates = [(day, rows) for (m, c, day), rows in auction_rows.items()
                      if (m, c) == key and day == quote_observation_date and rows]
        if not candidates:
            continue
        day, points = max(candidates, key=lambda item: item[0])
        last = points[-1]
        tick = 10 ** -decimals[key]
        same = lambda price: abs(price - last["price"]) < tick / 2
        bid, ask = snapshot["bids"][0], snapshot["asks"][0]
        side = "buy" if same(bid["price"]) and not same(ask["price"]) else (
            "sell" if same(ask["price"]) and not same(bid["price"]) else "ambiguous")
        direction.append({
            "market": key[0], "code": key[1], "requested_date": day,
            "auction_time": last["time_label"], "signed_raw": last["unmatched_signed_raw"],
            "auction_price": last["price"], "bid1": bid["price"], "ask1": ask["price"],
            "post_close_book_side_at_auction_price": side,
            "supports_positive_buy_negative_sell": side == ("buy" if last["unmatched_signed_raw"] > 0 else "sell")
                if last["unmatched_signed_raw"] and side != "ambiguous" else None,
            "synchronous_side_confirmation": False,
        })
    selector_checks = None
    if summary.get("date_selector_check"):
        assert date_schema == 2
        expected = {(m, c, d) for m, c in summary["symbols"] for d in summary["recent_dates"]}
        assert set(recent_rows) == set(controls) == expected
        trading = {20260609, 20260610, 20260817, 20260818, 20261022, 20261023}
        empty_days = {20261024, 20261026}
        assert set(summary["recent_dates"]) == trading | empty_days
        assert all(len(recent_rows[key]["page"]["records"]) == len(controls[key]) == 240
                   for key in expected if key[2] in trading)
        assert all(not recent_rows[key]["page"]["records"] and not controls[key]
                   for key in expected if key[2] in empty_days)
        assert len(cross_recent) == len(recent_bar_checks) == len(selected) * len(trading)
        selector_checks = {
            "complete": True, "trading_dates": sorted(trading),
            "empty_weekend_and_future_dates": sorted(empty_days),
            "same_day_controls_match": all(r["matches_control"] for r in cross_recent),
            "dated_daily_open_close_match": all(r["open_matches"] and r["close_matches"]
                                               for r in recent_bar_checks),
        }
    return {
        "verification": "passed", "endpoint": summary["endpoint"],
        "recent_minutes_date_schema_version": date_schema,
        "date_selector_checks": selector_checks,
        "method": "Independent stdlib framing/zlib/IEEE bits/signed varints/fixed auction records/dated K-line and historical minute controls",
        "wire_frames_checked": sum(counts.values()), "command_frames": dict(counts),
        "directory_rows_checked": sum(directory_counts.values()), "records_checked": dict(records),
        "nonempty_recent_series": nonempty, "recent_control_matches": cross_recent,
        "recent_dated_bar_comparisons": recent_bar_checks,
        "auction_date_selection": date_checks, "auction_close_comparisons": final_price_checks,
        "auction_sign_counts": dict(collections.Counter(str(p["unmatched_sign"]) for rows in auction_rows.values() for p in rows)),
        "auction_direction_corroboration": direction,
        "recent_nonempty_live_verified": bool(nonempty),
        "recent_requested_date_business_verified": bool(cross_recent) and bool(recent_bar_checks)
            and all(c["matches_control"] for c in cross_recent)
            and all(c["open_matches"] and c["close_matches"] for c in recent_bar_checks),
        "auction_date_selection_live_verified": bool(date_checks) and all(r["date_selection_changed_payload"] for r in date_checks),
        "auction_side_mapping_exchange_confirmed": False,
        "scope": "Requested dates are not echoed dates. Post-close books are not synchronous auction side evidence. Empty responses do not establish the complete retention window.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables assertions")
    root = args.directory
    files = sorted(p for p in root.iterdir() if p.is_file()
                   and p.name not in {"independent-verification.json", "sha256-manifest.json"})
    digests = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    manifest = root / "sha256-manifest.json"
    if manifest.exists():
        assert json.loads(manifest.read_text()) == digests, "capture files changed"
    else:
        assert args.freeze, "initial verification requires --freeze"
    result = verify(root)
    if not manifest.exists():
        manifest.write_text(json.dumps(digests, indent=2) + "\n")
    (root / "independent-verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
