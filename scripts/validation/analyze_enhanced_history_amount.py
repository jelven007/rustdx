#!/usr/bin/env python3
"""Offline amount attribution from retained 0fc6/0fb5 and 1/5-minute/day frames."""
import argparse
from bisect import bisect_left, bisect_right
from collections import Counter
import datetime as dt
from decimal import Decimal
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import zlib

from verify_enhanced_history_transaction import decode as decode_enhanced
from evidence_paths import workspace_path


WORKSPACE = Path(__file__).resolve().parents[2]
KLINE_PATH = workspace_path("docs/testing/evidence/kline-volume-20261024/validate.py")
FIELDS = ["time_minutes", "price_acc_raw", "volume", "order_count", "status_raw", "reserved_raw"]
LEGACY_FIELDS = [field for field in FIELDS if field != "order_count"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def load_module(path):
    spec = importlib.util.spec_from_file_location("amount_kline_decoder", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def decode_legacy(request, payload):
    require(len(request) == 28 and request[10:12] == b"\xb5\x0f", "not legacy history")
    count = struct.unpack_from("<H", payload)[0]
    require(count <= struct.unpack_from("<H", request, 26)[0], "legacy excessive count")
    position, price, rows = 6, 0, []

    def varint():
        nonlocal position
        first = current = payload[position]
        position += 1
        value, shift, used = first & 63, 6, 1
        while current & 128:
            require(used < 5, "legacy overlong varint")
            current = payload[position]
            position += 1
            value |= (current & 127) << shift
            shift += 7
            used += 1
        return -value if first & 64 else value

    for _ in range(count):
        minute = struct.unpack_from("<H", payload, position)[0]
        position += 2
        delta, volume, status, reserved = [varint() for _ in range(4)]
        price += delta
        require(minute < 1440 and price >= 0 and volume >= 0, "legacy invalid values")
        rows.append({"time_minutes": minute, "price_acc_raw": price, "volume": volume,
                     "status_raw": status, "reserved_raw": reserved})
    require(position == len(payload), "legacy trailing payload")
    return rows


def read_frame(stem):
    request, header, wire, retained = [
        stem.with_suffix(f".{suffix}").read_bytes()
        for suffix in ("request.bin", "header.bin", "wire.bin", "payload.bin")]
    require(len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400"), "frame prefix")
    require(header[5:9] == request[1:5] and header[10:12] == request[10:12], "frame identity")
    compressed, expanded = struct.unpack_from("<HH", header, 12)
    require(compressed == len(wire), "frame compressed length")
    payload = wire if compressed == expanded else zlib.decompress(wire)
    require(payload == retained and len(payload) == expanded, "retained payload mismatch")
    frame_path = stem.with_suffix(".frame.bin")
    if frame_path.exists():
        require(frame_path.read_bytes() == header + wire, "complete frame differs")
    return request, payload


def freeze_or_verify(directory, previous, freeze):
    path = directory / "inputs.json"
    if freeze:
        require(not path.exists(), "refusing to overwrite input hashes")
        files = [directory / "capture.json", *sorted((directory / "wire").iterdir())]
        sources = [Path(__file__), Path(__file__).with_name("verify_enhanced_history_transaction.py"),
                   KLINE_PATH,
                   WORKSPACE / "docs/testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/trade.py"]
        write_json(path, {
            "files_sha256": {str(p.relative_to(directory)): sha(p.read_bytes()) for p in files},
            "previous_inputs_sha256": sha((previous / "inputs.json").read_bytes()),
            "source_sha256": {str(p.relative_to(WORKSPACE)): sha(p.read_bytes()) for p in sources},
        })
    frozen = json.loads(path.read_text())
    for relative, expected in frozen["files_sha256"].items():
        require(sha((directory / relative).read_bytes()) == expected, f"input differs: {relative}")
    require(sha((previous / "inputs.json").read_bytes()) == frozen["previous_inputs_sha256"],
            "previous input manifest differs")
    old = json.loads((previous / "inputs.json").read_text())
    for name, expected in old["files_sha256"].items():
        require(sha((previous / name).read_bytes()) == expected, f"previous capture differs: {name}")
    for relative, expected in frozen["source_sha256"].items():
        require(sha(workspace_path(relative).read_bytes()) == expected, f"decoder source differs: {relative}")
    return frozen


def collect_pages(queries, directory, kind):
    pages, offset = [], 0
    for query in queries:
        if query["kind"] != kind:
            continue
        request, payload = read_frame(directory / "wire" / query["label"])
        require(struct.unpack_from("<IH", request, 12) == (20261023, 0)
                and request[18:24] == b"300308", "history request symbol/date")
        require(struct.unpack_from("<HH", request, 24) == (offset, 1800), "raw offset gap")
        rows = (decode_enhanced(request, payload)["records"] if kind == "enhanced"
                else decode_legacy(request, payload))
        require(len(rows) == query["rows"], "capture count differs")
        pages.append(rows)
        offset += len(rows)
    require(pages and not pages[-1] and all(pages[:-1]), "missing terminal empty page")
    return [row for page in reversed(pages) for row in page]


def align_bars(rows, bars, numeric):
    prefix = [0]
    for row in rows:
        prefix.append(prefix[-1] + row["volume"] * 100)
    cumulative, left, result = 0, 0, []
    for bar in bars:
        volume = bar["eltdx_wire"]
        amount = bar["eltdx_amount"]
        cumulative += volume
        stamp = dt.datetime.fromisoformat(bar["time"])
        minute = stamp.hour * 60 + stamp.minute
        candidates = [i for i in range(bisect_left(prefix, cumulative), bisect_right(prefix, cumulative))
                      if i >= left and (i == len(rows) or rows[i]["time_minutes"] >= minute)
                      and (i == 0 or rows[i - 1]["time_minutes"] <= minute)]
        require(len(candidates) == 1, f"ambiguous amount interval: {bar['time']}")
        right = candidates[0]
        events = rows[left:right]
        proxy = sum(row["price_acc_raw"] * row["volume"] for row in events)
        raw = bar["amount_encoding_u32"]
        ulp = numeric.eltdx_volume(raw + 1) - amount if raw else 0
        result.append({
            "time": bar["time"], "event_slice": [left, right],
            "event_count": len(events), "shares_in_both_sources": volume,
            "amount_proxy_yuan": proxy, "kline_amount_yuan": amount, "difference_yuan": proxy - amount,
            "difference_per_share_yuan": (proxy - amount) / volume if volume else None,
            "amount_encoding_ulp_yuan": ulp,
            "source_vwap_yuan": amount / volume if volume else None,
            "proxy_weighted_price_yuan": proxy / volume if volume else None,
            "first_event_price": events[0]["price"] if events else None,
            "last_event_price": events[-1]["price"] if events else None,
            "kline_close": bar["close"], "kline_high": bar["high"], "kline_low": bar["low"],
            "last_event_matches_close": events[-1]["price"] == bar["close"] if events else None,
            "volume_encoding_u32": bar["volume_encoding_u32"],
            "amount_encoding_u32": raw,
        })
        left = right
    require(left == len(rows), "amount intervals do not cover the full regular session")
    return result


def analyze(directory, previous, freeze=False):
    frozen = freeze_or_verify(directory, previous, freeze)
    manifest = json.loads((directory / "capture.json").read_text())
    require(manifest["capture_complete"] and not manifest["auto_reconnect"], "incomplete/reconnected capture")
    require((manifest["market"], manifest["code"], manifest["requested_date"]) == (0, "300308", 20261023),
            "this experiment is pinned to sz300308 / 20261023")
    numeric = load_module(KLINE_PATH)
    bars = {}
    for query in manifest["queries"]:
        request, payload = read_frame(directory / "wire" / query["label"])
        require(query["peer_before"] == query.get("peer_after") == manifest["endpoint"]
                and query["local_before"] == query.get("local_after") == manifest["local"], "socket changed")
        if query["kind"] == "kline":
            category = query["category"]
            require(request[10:12] == b"\x2d\x05" and request[14:20] == b"300308"
                    and struct.unpack_from("<H", request, 12)[0] == 0, "K-line identity")
            require(struct.unpack_from("<H", request, 20)[0] == category
                    and struct.unpack_from("<HH", request, 24) == (0, query["count"]), "K-line parameters")
            bars[category] = numeric.decode(payload, category, "stock", query["count"])
        elif query["kind"] == "handshake":
            require(request[10:12] == b"\x0d\x00", "invalid handshake")
    enhanced = collect_pages(manifest["queries"], directory, "enhanced")
    legacy = collect_pages(manifest["queries"], directory, "legacy")
    require([[row[f] for f in LEGACY_FIELDS] for row in enhanced]
            == [[row[f] for f in LEGACY_FIELDS] for row in legacy], "0fb5 and 0fc6 differ")
    old_manifest = json.loads((previous / "capture.json").read_text())
    old_pages = []
    for query in old_manifest["queries"]:
        if query.get("label", "").startswith("pass-1800-"):
            request, payload = read_frame(previous / query["frame"])
            old_pages.append(decode_enhanced(request, payload)["records"])
        elif query["kind"] == "kline":
            category = query["category"]
            _, payload = read_frame(previous / query["frame"])
            require(payload == (directory / "wire" / f"kline-{category}.payload.bin").read_bytes(),
                    "daily/five-minute source changed since prior capture")
    old_rows = [row for page in reversed(old_pages) for row in page]
    require([[row[f] for f in FIELDS] for row in enhanced] == [[row[f] for f in FIELDS] for row in old_rows],
            "0fc6 changed since prior capture")
    regular = [row for row in enhanced if row["status_raw"] not in (5, 8)]
    after = [row for row in enhanced if row["status_raw"] == 5]
    selected = {category: [bar for bar in value if bar["time"].startswith("2026-10-23")]
                for category, value in bars.items()}
    require(len(selected[8]) == 240 and len(selected[0]) == 48 and len(selected[4]) == len(selected[9]) == 1,
            "missing day/bar coverage")
    expected_minute = list(range(571, 691)) + list(range(781, 901))
    require([bar["time"][-5:] for bar in selected[8]]
            == [f"{m // 60:02}:{m % 60:02}" for m in expected_minute], "missing one-minute labels")
    minutes = align_bars(regular, selected[8], numeric)
    five = align_bars(regular, selected[0], numeric)
    proxy = sum(row["price_acc_raw"] * row["volume"] for row in regular)
    day_amount = selected[4][0]["eltdx_amount"]
    minute_amount = sum(bar["eltdx_amount"] for bar in selected[8])
    five_amount = sum(bar["eltdx_amount"] for bar in selected[0])
    day_raw = selected[4][0]["amount_encoding_u32"]
    day_ulp = numeric.eltdx_volume(day_raw + 1) - day_amount
    float_proxy = sum(row["price"] * row["volume"] * 100.0 for row in regular)
    require(sum(row["difference_yuan"] for row in minutes) == proxy - minute_amount, "minute attribution mismatch")
    require(sum(row["difference_yuan"] for row in five) == proxy - five_amount, "five-minute attribution mismatch")
    close_bar = minutes[-1]
    fixed_price = Decimal(str(close_bar["kline_close"]))
    implied_integer_shares = round(Decimal(str(close_bar["kline_amount_yuan"])) / fixed_price)
    fixed_amount = fixed_price * implied_integer_shares
    closing = {
        "time": close_bar["time"], "reported_event_lots": regular[-1]["volume"],
        "reported_minute_shares": close_bar["shares_in_both_sources"],
        "fixed_price_yuan": str(fixed_price), "reported_kline_amount_yuan": close_bar["kline_amount_yuan"],
        "reported_proxy_yuan": close_bar["amount_proxy_yuan"],
        "amount_derived_integer_share_candidate": implied_integer_shares,
        "candidate_minus_reported_shares": implied_integer_shares - int(close_bar["shares_in_both_sources"]),
        "candidate_amount_yuan": str(fixed_amount),
        "encoded_amount_minus_candidate_yuan": str(Decimal(str(close_bar["kline_amount_yuan"])) - fixed_amount),
        "encoding_ulp_yuan": close_bar["amount_encoding_ulp_yuan"],
        "interpretation": "Inferred under fixed closing-match price and amount error <= one encoding ULP; "
                          "not an observed exchange quantity or proof of rounding algorithm.",
    }
    rows_count = Counter(row["time_minutes"] for row in regular)
    result = {
        "scope": {key: manifest[key] for key in ("market", "code", "requested_date", "endpoint",
                                               "started_utc", "finished_utc")},
        "verification": {"frame_count": len(manifest["queries"]), "input_files": len(frozen["files_sha256"]),
                         "previous_input_files": len(json.loads((previous / "inputs.json").read_text())["files_sha256"]),
                         "same_socket_all_queries": True, "enhanced_raw_rows": len(enhanced),
                         "legacy_raw_rows": len(legacy), "legacy_all_common_fields_match": True,
                         "enhanced_matches_previous_capture": True, "old_5min_day_payloads_unchanged": True,
                         "minute_boundary_count": len(minutes), "five_minute_boundary_count": len(five),
                         "all_volume_boundaries_exact": True},
        "amounts": {"proxy_yuan": proxy, "daily_yuan": day_amount, "one_minute_sum_yuan": minute_amount,
                    "five_minute_sum_yuan": five_amount, "proxy_minus_day_yuan": proxy - day_amount,
                    "proxy_minus_minute_yuan": proxy - minute_amount,
                    "minute_minus_five_yuan": minute_amount - five_amount,
                    "five_minus_day_yuan": five_amount - day_amount,
                    "minute_minus_day_yuan": minute_amount - day_amount,
                    "float_arithmetic_minus_integer_proxy_yuan": float_proxy - proxy,
                    "after_hours_proxy_yuan": sum(row["price_acc_raw"] * row["volume"] for row in after)},
        "quantization": {"daily_amount_ulp_yuan": day_ulp,
                         "one_minute_amount_ulp_sum_yuan": sum(row["amount_encoding_ulp_yuan"] for row in minutes),
                         "five_minute_amount_ulp_sum_yuan": sum(row["amount_encoding_ulp_yuan"] for row in five),
                         "one_minute_plus_daily_one_ulp_bound_yuan":
                             sum(row["amount_encoding_ulp_yuan"] for row in minutes) + day_ulp,
                         "minute_residual_exceeds_own_amount_ulp":
                             sum(abs(row["difference_yuan"]) > row["amount_encoding_ulp_yuan"] for row in minutes),
                         "assumption": "Bounds assume each underlying amount is encoded with at most one ULP error; "
                                       "not a vendor guarantee or full source-statistics error bound."},
        "aggregation_evidence": {
            "regular_records": len(regular), "order_count_gt_one": sum(row["order_count"] > 1 for row in regular),
            "order_count_sum_raw": sum(row["order_count"] for row in regular),
            "nonempty_minute_bars": sum(row["event_count"] > 0 for row in minutes),
            "last_price_matches_minute_close": sum(row["last_event_matches_close"] is True for row in minutes),
            "continuous_records_per_minute_histogram":
                dict(Counter(rows_count[m] for m in list(range(570, 690)) + list(range(780, 897)))),
            "high_price_events": max(row["price"] for row in regular),
            "high_price_day": selected[4][0]["high"],
            "low_price_events": min(row["price"] for row in regular),
            "low_price_day": selected[4][0]["low"],
            "minute_positive_residuals": sum(row["difference_yuan"] > 0 for row in minutes),
            "minute_negative_residuals": sum(row["difference_yuan"] < 0 for row in minutes),
            "minute_positive_residual_sum_yuan": sum(row["difference_yuan"] for row in minutes if row["difference_yuan"] > 0),
            "minute_negative_residual_sum_yuan": sum(row["difference_yuan"] for row in minutes if row["difference_yuan"] < 0),
            "morning_residual_sum_yuan": sum(row["difference_yuan"] for row in minutes if row["time"][-5:] < "12:00"),
            "afternoon_residual_sum_yuan": sum(row["difference_yuan"] for row in minutes if row["time"][-5:] > "12:00"),
        },
        "closing_quantity_inference": closing,
        "one_minute_attribution": minutes, "five_minute_attribution": five,
        "top_minute_residuals": sorted(minutes, key=lambda row: abs(row["difference_yuan"]), reverse=True)[:10],
        "conclusions": {
            "primary_mechanism": "End-point prices times aggregated interval volume differ from sums of actual execution price times quantity.",
            "secondary_mechanism_inferred": "Whole-lot source quantity granularity; fixed-price closing amount suggests 45 additional shares.",
            "precise_price_vs_quantity_split_identifiable": False,
            "unresolved": "No per-interval real turnover or exchange trade IDs in 0fc6; "
                          "intra-record price weighting and exact quantity rounding cannot be uniquely recovered.",
            "not_supported_as_primary_causes": ["floating-point multiplication", "K-line amount encoding alone",
                                                "page gaps/duplicates", "after-hours mixing", "minute boundary misalignment"],
        },
    }
    write_json(directory / "results.json", result)
    print(json.dumps({key: result[key] for key in ("verification", "amounts", "quantization",
                                                  "aggregation_evidence", "closing_quantity_inference")},
                     ensure_ascii=False, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture_directory", type=Path)
    parser.add_argument("--previous", type=Path,
                        default=WORKSPACE / "docs/testing/evidence/enhanced-history-day-20261024")
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args()
    analyze(args.capture_directory, args.previous, args.freeze)


if __name__ == "__main__":
    main()
