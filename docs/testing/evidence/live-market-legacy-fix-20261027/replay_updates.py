#!/usr/bin/env python3
"""Offline replay of the retained update and historical-minute captures."""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import struct
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", type=Path, default=Path(__file__).resolve().parents[4])
parser.add_argument("--data", type=Path, default=Path(__file__).resolve().parent)
parser.add_argument("--day-reference", type=Path)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
if args.output.exists():
    parser.error("use a new output; retained evidence must not be overwritten")
source = args.source.resolve()
art = args.data.resolve()
day_reference = args.day_reference or (
    source / "crates/rustdx-core/tests/fixtures/daily-reference/sz000001.day"
)
sys.path.insert(0, str(source / "scripts/validation"))
from verify_quote_push import change, decode, snapshot_rows
from verify_recent_auction import read_frame, recent
from verify_nine_commands import Reader
from recent_minutes_selector import date_from_selector


def exact(actual, expected):
    if isinstance(expected, dict):
        for key, value in expected.items():
            exact(actual[key], value)
    elif isinstance(expected, list):
        assert len(actual) == len(expected)
        for actual_row, expected_row in zip(actual, expected):
            exact(actual_row, expected_row)
    else:
        assert actual == expected, (actual, expected)


def updates(name):
    root = art / name
    summary = json.loads((root / "summary.json").read_text())
    assert summary["status"] == "captured"
    assert summary["heartbeat"] is False and summary["auto_reconnect"] == 0
    assert len(summary["refreshes"]) == 4
    assert all(row["origin"] == "requested_refresh" for row in summary["refreshes"])
    decimals = {(market, code): decimal for market, code, decimal in
                json.loads((root / "precision.json").read_text())}
    stocks = {tuple(row) for row in summary["symbols"]}
    previous, changes, frames = {}, [], 0
    for path in sorted(root.glob("*.request.bin")):
        opcode, request, payload = read_frame(path, summary["endpoint"])
        frames += 1
        if opcode == 0x000d:
            continue
        assert opcode == 0x0547
        count = struct.unpack_from("<H", request, 12)[0]
        assert count == 3 and len(request) == 14 + count * 11
        for index in range(count):
            offset = 14 + index * 11
            key = request[offset], request[offset + 1:offset + 7].decode()
            cursor = struct.unpack_from("<I", request, offset + 7)[0]
            assert key in stocks
            assert cursor == (previous[key]["update_cursor"] if previous else 0)
        rows = decode(payload, decimals)
        index = len(changes)
        exact(json.loads((root / f"refresh-{index}.json").read_text())["page"]["records"], rows)
        state = json.loads((root / f"snapshot-{index}.json").read_text())
        actual = snapshot_rows(state, stocks, summary["refreshes"][0]["connection_id"], 0)
        for row in rows:
            exact(actual[row["market_raw"], row["code"]], row)
        advanced = [
            {"market": key[0], "code": key[1],
             "previous_cursor": previous[key]["update_cursor"],
             "cursor": actual[key]["update_cursor"]}
            for key in previous if change(previous[key], actual[key])
        ]
        changes.append(dict(index=index, returned_rows=len(rows), market_changes=advanced))
        previous = actual
    assert frames == 5 and len(changes) == 4
    assert any(row["market_changes"] for row in changes)
    return dict(
        endpoint=summary["endpoint"], frame_checks=frames, update_calls=4,
        same_connection_and_generation=True, origin="requested_refresh",
        market_changes_observed=True, changes=changes,
        started_utc=summary["started_utc"], finished_utc=summary["finished_utc"],
        passive_push_verified=False,
    )


def window(name):
    root = art / name
    summary = json.loads((root / "summary.json").read_text())
    assert summary["status"] == "captured"
    decimals = {(market, code): decimal for market, code, decimal in
                json.loads((root / "precision.json").read_text())}
    results, price_checks = {}, []
    day_bytes = day_reference.read_bytes()
    assert hashlib.sha256(day_bytes).hexdigest() == (
        "d0c169a32b03bb63e2588512d630e5021c73318e315303205ca7a6eaebc67173"
    )
    day_rows = {
        struct.unpack_from("<I", day_bytes, index)[0]:
        struct.unpack_from("<4I", day_bytes, index + 4)
        for index in range(0, len(day_bytes), 32)
    }
    for path in sorted(root.glob("*.request.bin")):
        opcode, request, payload = read_frame(path, summary["endpoint"])
        if opcode == 0x000d:
            continue
        assert opcode == 0x0feb
        date = date_from_selector(struct.unpack_from("<I", request, 12)[0], 2)
        market, code = request[16], request[17:23].decode()
        reader = Reader(payload)
        expected = recent(reader, 10 ** decimals[market, code])
        assert reader.offset == len(payload)
        dto = json.loads((root / f"recent-{date}-{market}-{code}.json").read_text())
        exact(dto, expected)
        assert dto["date_qualification"] == {
            "Unverified": {"reason": "ServerDoesNotEchoTradingDate"}
        }
        results[date, market, code] = len(expected["page"]["records"])
        if (market, code) == (0, "000001"):
            reference = day_rows[date]
            assert round(dto["open_price"] * 100) == reference[0]
            assert dto["page"]["records"][-1]["price_raw"] == reference[3]
            price_checks.append(dict(
                requested_date=date, reference_sha256=hashlib.sha256(day_bytes).hexdigest(),
                open_matches=True, close_matches=True,
            ))
    assert len(results) == len(summary["results"]) == 33
    for row in summary["results"]:
        assert results[row["requested_date"], row["market"], row["code"]] == row["points"] == 240
    return dict(
        endpoint=summary["endpoint"], probes=33, wire_replay="passed",
        results=summary["results"], independent_day_open_close_checks=price_checks,
        actual_trading_dates_qualified=False, retention_window_guaranteed=False,
    )


report = dict(
    schema=1, checked_utc=dt.datetime.now(dt.timezone.utc).isoformat(),
    updates=[updates("update-hz"), updates("update-sh")],
    windows=[window("window-hz-explicit"), window("window-sh-explicit")],
)
args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({
    "status": "passed", "updates": sum(row["update_calls"] for row in report["updates"]),
    "minute_requests": sum(row["probes"] for row in report["windows"]),
    "passive_push_verified": False, "retention_window_guaranteed": False,
}, ensure_ascii=False))
