#!/usr/bin/env python3
"""Offline full-day 0fc6 replay, pagination/overlap checks and K-line reconciliation."""
import argparse
from collections import Counter
import datetime as dt
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import zlib

from verify_enhanced_history_transaction import decode as decode_enhanced
from evidence_paths import workspace_path


WORKSPACE = Path(__file__).resolve().parents[2]
KLINE_DECODER = workspace_path("docs/testing/evidence/kline-volume-20261024/validate.py")
FIELDS = ["time_minutes", "price_acc_raw", "volume", "order_count", "status_raw", "reserved_raw"]
SOURCES = [
    "crates/rustdx-core/examples/validate_enhanced_history_day.rs",
    "scripts/validation/verify_enhanced_history_day.py",
    "scripts/validation/verify_enhanced_history_transaction.py",
    "crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs",
    "crates/rustdx-core/src/tcp/stock/kline_request.rs",
    "crates/rustdx-core/src/tcp/helper.rs",
    "crates/rustdx-core/src/tcp/mod.rs",
    "docs/testing/evidence/kline-volume-20261024/validate.py",
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(rows):
    return [tuple(row[field] for field in FIELDS) for row in rows]


def fingerprint(rows):
    return sha(json.dumps(canonical(rows), separators=(",", ":")).encode())


def hhmm(minutes):
    return f"{minutes // 60:02}:{minutes % 60:02}"


def verify_inputs(directory, freeze):
    path = directory / "inputs.json"
    if freeze:
        require(not path.exists(), "refusing to overwrite frozen inputs")
        files = sorted(p for p in directory.iterdir() if p.is_file()
                       and p.name not in {"results.json", "canonical-records.json", "README.md"})
        frozen = {
            "files_sha256": {p.name: sha(p.read_bytes()) for p in files},
            "sources_sha256": {name: sha(workspace_path(name).read_bytes()) for name in SOURCES},
        }
        with path.open("x") as output:
            json.dump(frozen, output, ensure_ascii=False, indent=2)
            output.write("\n")
    frozen = json.loads(path.read_text())
    for name, expected in frozen["files_sha256"].items():
        require(sha((directory / name).read_bytes()) == expected, f"input differs: {name}")
    for name, expected in frozen["sources_sha256"].items():
        require(sha(workspace_path(name).read_bytes()) == expected, f"source differs: {name}")
    return frozen


def replay(directory, freeze=False):
    frozen = verify_inputs(directory, freeze)
    manifest = json.loads((directory / "capture.json").read_text())
    require(manifest.get("capture_complete"), "capture did not finish")
    require(manifest["auto_reconnect"] == 0, "reconnect was enabled")
    require(manifest["frame_count"] == len(manifest["queries"]) + 1, "unexpected extra frames")
    require(len(list(directory.glob("*.request.bin"))) == manifest["frame_count"],
            "frame inventory differs")
    requested_day = dt.datetime.strptime(str(manifest["requested_date"]), "%Y%m%d").date()
    spec = importlib.util.spec_from_file_location("independent_kline", KLINE_DECODER)
    kline = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(kline)
    decoded_pages, decoded_bars, frame_results = {}, {}, []
    queries = [{"frame": "frame-000", "kind": "handshake"}] + manifest["queries"]
    for query in queries:
        stem = query["frame"]
        request, header, wire, retained = [
            (directory / f"{stem}.{suffix}").read_bytes()
            for suffix in ("request.bin", "header.bin", "wire.bin", "payload.bin")]
        require(len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400"),
                f"invalid header: {stem}")
        require(header[5:9] == request[1:5] and header[10:12] == request[10:12],
                f"response identity differs: {stem}")
        compressed, expanded = struct.unpack_from("<HH", header, 12)
        require(compressed == len(wire), f"compressed size differs: {stem}")
        payload = wire if compressed == expanded else zlib.decompress(wire)
        require(len(payload) == expanded and payload == retained, f"payload differs: {stem}")
        require((directory / f"{stem}.endpoint.txt").read_text() == manifest["peer"],
                f"frame endpoint differs: {stem}")
        frame_results.append({"frame": stem, "opcode": request[10:12].hex(),
                              "payload_sha256": sha(payload), "payload_length": len(payload)})
        if query["kind"] == "handshake":
            require(request[10:12] == b"\x0d\x00", "unexpected handshake")
            continue
        require(query["connection_id_before"] == query["connection_id_after"]
                == manifest["connection_id"], f"connection changed: {stem}")
        require(query["peer_after"] == manifest["peer"] and query["local_after"] == manifest["local"],
                f"socket identity changed: {stem}")
        rust = json.loads((directory / query["page"]).read_text())
        if query["kind"] == "enhanced_history":
            decoded = decode_enhanced(request, payload)
            require((decoded["date"], decoded["market"], decoded["code"]) ==
                    (manifest["requested_date"], manifest["market"], manifest["code"]),
                    f"requested symbol/date differs: {stem}")
            require((decoded["start"], decoded["request_count"]) == (query["start"], query["count"]),
                    f"pagination request differs: {stem}")
            for key, value in decoded.items():
                if key == "price_base_raw_f32":
                    require(struct.pack("<f", rust[key]) == payload[2:6],
                            f"Rust reference bits differ: {stem}")
                else:
                    require(rust[key] == value, f"Rust DTO differs at {key}: {stem}")
            require(bytes(rust["raw_payload"]) == payload, f"Rust retained payload differs: {stem}")
            require(rust["source_endpoint"] == manifest["peer"], f"Rust source differs: {stem}")
            require(len(decoded["records"]) == query["rows"], f"row count differs: {stem}")
            decoded_pages[query["label"]] = decoded
        elif query["kind"] == "kline":
            require(request[10:12] == b"\x2d\x05", f"not K-line: {stem}")
            require(struct.unpack_from("<H", request, 12)[0] == manifest["market"]
                    and request[14:20].decode() == manifest["code"], f"K-line symbol differs: {stem}")
            require(struct.unpack_from("<H", request, 20)[0] == query["category"]
                    and struct.unpack_from("<HH", request, 24) == (query["start"], query["count"]),
                    f"K-line parameters differ: {stem}")
            bars = kline.decode(payload, query["category"], "stock", query["count"])
            require(len(bars) == len(rust) == query["rows"], f"K-line count differs: {stem}")
            for independent, native in zip(bars, rust):
                timestamp = dt.datetime(**native["dt"]).isoformat(timespec="minutes")
                require(timestamp == independent["time"], f"K-line time differs: {stem}")
                require(native["code"] == manifest["code"], f"K-line DTO code differs: {stem}")
                for left, right in [("open", "open"), ("high", "high"), ("low", "low"),
                                    ("close", "close"), ("vol", "rustdx_vol"),
                                    ("amount", "rustdx_amount")]:
                    require(native[left] == independent[right], f"K-line DTO {left} differs: {stem}")
            decoded_bars[query["category"]] = bars
        else:
            raise ValueError(f"unknown query kind: {query['kind']}")

    passes = []
    for size in (1800, 1000):
        pages = [decoded_pages[q["label"]] for q in manifest["queries"]
                 if q.get("label", "").startswith(f"pass-{size}-")]
        require(pages and not pages[-1]["records"], f"missing terminal empty page: {size}")
        offset = 0
        for index, page in enumerate(pages):
            require(page["start"] == offset and page["request_count"] == size,
                    f"noncontiguous raw offsets: {size}")
            require(bool(page["records"]) or index == len(pages) - 1, "early empty page")
            offset += len(page["records"])
        # Offsets run backwards from the latest end. Reverse pages, retain wire
        # order within each page; do not sort or deduplicate the records.
        rows = [row for page in reversed(pages) for row in page["records"]]
        require(all(a["time_minutes"] <= b["time_minutes"] for a, b in zip(rows, rows[1:])),
                f"chronology inversion: {size}")
        passes.append({"page_size": size, "total": offset, "records": rows,
                       "sha256": fingerprint(rows),
                       "pages": [{"start": p["start"], "count": len(p["records"])} for p in pages]})
    primary, secondary = passes
    rows = primary["records"]
    require(canonical(rows) == canonical(secondary["records"]), "pagination sizes return different events")
    require(len(rows) == manifest["primary_raw_count"] == manifest["secondary_raw_count"],
            "manifest full-day counts differ")
    overlaps = []
    for label, page in decoded_pages.items():
        if label.startswith("pass-"):
            continue
        lo = max(0, len(rows) - page["start"] - page["request_count"])
        hi = max(0, len(rows) - page["start"])
        require(canonical(page["records"]) == canonical(rows[lo:hi]), f"overlap differs: {label}")
        overlaps.append({"label": label, "start": page["start"], "request_count": page["request_count"],
                         "chronological_slice": [lo, hi], "records": len(page["records"]), "matches": True})

    actual = [row for row in rows if row["status_raw"] != 8]
    after_hours = [row for row in actual if row["status_raw"] == 5 and 905 <= row["time_minutes"] <= 930]
    regular = [row for row in actual if row not in after_hours]
    require(rows and regular, "empty source day has no positive completeness evidence")
    snapshots = [row for row in rows if row["status_raw"] == 8]
    require(all(row["volume"] == 0 for row in snapshots), "nonzero snapshot needs separate reconciliation")
    regular_volume = sum(row["volume"] for row in regular)
    regular_amount_proxy = sum(row["price_acc_raw"] * row["volume"] for row in regular)
    after_volume = sum(row["volume"] for row in after_hours)
    after_amount = sum(row["price_acc_raw"] * row["volume"] for row in after_hours)
    expected_minutes = list(range(570, 690)) + list(range(780, 897))
    minute_counts = Counter(row["time_minutes"] for row in regular)
    missing_minutes = [hhmm(minute) for minute in expected_minutes if minute_counts[minute] == 0]
    minute_day = [bar for bar in decoded_bars[0] if bar["time"][:10] == str(requested_day)]
    expected_bar_times = [hhmm(minute) for minute in list(range(575, 691, 5)) + list(range(785, 901, 5))]
    cumulative = 0
    windows = []
    for bar in minute_day:
        cumulative += bar["rustdx_vol"]
        stamp = dt.datetime.fromisoformat(bar["time"])
        minute = stamp.hour * 60 + stamp.minute
        lower = sum(row["volume"] * 100 for row in regular if row["time_minutes"] < minute)
        upper = sum(row["volume"] * 100 for row in regular if row["time_minutes"] <= minute)
        windows.append({"bar_time": bar["time"], "kline_cumulative_shares": cumulative,
                        "event_cumulative_before_minute_shares": lower,
                        "event_cumulative_through_minute_shares": upper,
                        "within_minute_resolution_bounds": lower <= cumulative <= upper})
    daily = {}
    for category in (9, 4):
        matches = [bar for bar in decoded_bars[category] if bar["time"][:10] == str(requested_day)]
        require(len(matches) == 1, f"missing daily comparison: category {category}")
        bar = matches[0]
        multiplier = 100 if category == 9 else 1
        daily[str(category)] = {
            "source_volume_unit_for_this_sample": "lots" if category == 9 else "shares",
            "bar": bar, "volume_as_shares": bar["rustdx_vol"] * multiplier,
            "events_regular_minus_daily_shares": regular_volume * 100 - bar["rustdx_vol"] * multiplier,
            "event_price_volume_proxy_yuan": regular_amount_proxy,
            "proxy_minus_daily_amount_yuan": regular_amount_proxy - bar["rustdx_amount"],
            "proxy_minus_daily_amount_percent":
                (regular_amount_proxy - bar["rustdx_amount"]) / bar["rustdx_amount"] * 100,
        }
    event_ohlc = {"open": regular[0]["price"], "close": regular[-1]["price"],
                  "high": max(row["price"] for row in regular),
                  "low": min(row["price"] for row in regular)}
    all_dups = Counter(canonical(rows))
    actual_dups = Counter(canonical(actual))
    result = {
        "scope": {"market": manifest["market"], "code": manifest["code"],
                  "requested_date": manifest["requested_date"], "peer": manifest["peer"],
                  "connection_id": manifest["connection_id"], "capture_started_utc": manifest["started_utc"],
                  "capture_finished_utc": manifest["finished_utc"]},
        "verification": {"input_files": len(frozen["files_sha256"]),
                         "pinned_sources": len(frozen["sources_sha256"]),
                         "frames": frame_results, "all_native_fields_match_independent_decoder": True,
                         "same_connection_all_queries": True,
                         "pagination_sizes_match": True, "overlaps": overlaps},
        "pagination": [{k: v for k, v in item.items() if k != "records"} for item in passes],
        "events": {"raw_count": len(rows), "actual_view_count": len(actual), "regular_count": len(regular),
                   "snapshot_count": len(snapshots), "after_hours_count": len(after_hours),
                   "status_counts": dict(sorted(Counter(r["status_raw"] for r in rows).items())),
                   "raw_time_range": [rows[0]["time"], rows[-1]["time"]],
                   "regular_volume_lots": regular_volume, "regular_volume_shares": regular_volume * 100,
                   "after_hours_volume_lots": after_volume, "after_hours_volume_shares": after_volume * 100,
                   "after_hours_fixed_price": sorted({r["price"] for r in after_hours}),
                   "after_hours_amount_proxy_yuan": after_amount,
                   "all_events_volume_shares": (regular_volume + after_volume) * 100,
                   "raw_identical_tuple_groups": sum(n > 1 for n in all_dups.values()),
                   "raw_identical_tuple_excess_records": sum(n - 1 for n in all_dups.values()),
                   "actual_identical_tuple_groups": sum(n > 1 for n in actual_dups.values()),
                   "actual_identical_tuple_excess_records": sum(n - 1 for n in actual_dups.values()),
                   "deduplicated": False},
        "coverage": {"continuous_expected_minutes": len(expected_minutes),
                     "continuous_missing_minutes": missing_minutes,
                     "continuous_records_per_minute_histogram":
                         dict(sorted(Counter(minute_counts[m] for m in expected_minutes).items())),
                     "regular_minute_counts": {hhmm(k): v for k, v in sorted(minute_counts.items())},
                     "minute_bars": len(minute_day),
                     "standard_48_bar_timestamps_match":
                         [bar["time"][-5:] for bar in minute_day] == expected_bar_times,
                     "cumulative_volume_checks": windows},
        "reconciliation": {
            "daily": daily, "event_ohlc": event_ohlc,
            "event_ohlc_minus_daily": {k: v - daily["4"]["bar"][k] for k, v in event_ohlc.items()},
            "five_minute_volume_shares": sum(bar["rustdx_vol"] for bar in minute_day),
            "events_regular_minus_five_minute_shares":
                regular_volume * 100 - sum(bar["rustdx_vol"] for bar in minute_day),
            "five_minute_amount_yuan": sum(bar["rustdx_amount"] for bar in minute_day),
            "five_minute_minus_daily_amount_yuan":
                sum(bar["rustdx_amount"] for bar in minute_day) - daily["4"]["bar"]["rustdx_amount"],
        },
        "conclusion": {
            "source_pagination_gap_detected": False,
            "continuous_minute_gap_detected": bool(missing_minutes),
            "exchange_event_completeness_proven": False,
            "limitation": "No wire date echo, total count, seconds or exchange trade IDs. "
                          "Volume agreement does not establish individual exchange prints. "
                          "Price*volume is a proxy: 0fc6 supplies no actual turnover field.",
        },
    }
    write_json(directory / "results.json", result)
    write_json(directory / "canonical-records.json",
               {"fields": FIELDS, "chronological_records": canonical(rows),
                "sha256": fingerprint(rows), "duplicates_preserved": True})
    print(json.dumps({"events": result["events"], "event_ohlc": event_ohlc,
                      "minute_gaps": missing_minutes, "canonical_sha256": primary["sha256"],
                      "volume_difference_shares": result["reconciliation"]["events_regular_minus_five_minute_shares"],
                      "amount_proxy_minus_daily_yuan": daily["4"]["proxy_minus_daily_amount_yuan"]},
                     ensure_ascii=False, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture_directory", type=Path)
    parser.add_argument("--freeze", action="store_true", help="create input/source hashes once")
    args = parser.parse_args()
    replay(args.capture_directory, args.freeze)


if __name__ == "__main__":
    main()
