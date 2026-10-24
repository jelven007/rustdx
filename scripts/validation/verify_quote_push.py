#!/usr/bin/env python3
"""Offline 0547 evidence replay. Stdlib only; never turns diagnostics into acceptance."""
import argparse
import collections
import datetime as dt
import hashlib
import json
from pathlib import Path
import struct
import unittest
import zlib

from verify_nine_commands import Reader, equal, quote

MARKET_FIELDS = (
    "price_fields_raw", "total_volume_raw", "current_volume_raw", "amount_raw",
    "inside_volume_raw", "outside_volume_raw", "bids", "asks",
)
ZONE = dt.timezone(dt.timedelta(hours=8))


def timestamp(value):
    result = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    assert result.tzinfo is not None, "timestamp must declare its timezone"
    return result.astimezone(ZONE)


def session(at, calendar):
    day = at.date().isoformat()
    assert calendar["coverage_start"] <= day <= calendar["coverage_end"], "calendar uncovered"
    if at.weekday() >= 5 or day in calendar["closed_weekdays"]:
        return None
    second = at.hour * 3600 + at.minute * 60 + at.second
    if 34200 <= second < 41400:
        return (day, 34200)
    if 46800 <= second < 54000:
        return (day, 46800)
    return None


def change(previous, current):
    return current["update_cursor"] > previous["update_cursor"] and any(
        current[key] != previous[key] for key in MARKET_FIELDS
    )


def snapshot_rows(snapshot, requested, connection, generation):
    """Require every requested key exactly once, with a quote and matching cursor."""
    assert snapshot["connection_id"] == connection
    assert snapshot["generation"] == generation
    assert len(snapshot["states"]) == len(requested)
    rows = {}
    for state in snapshot["states"]:
        key = (state["symbol"]["market"], state["symbol"]["code"])
        assert key in requested and key not in rows, "missing/extra/duplicate snapshot key"
        row = state["quote"]
        assert row is not None, "snapshot has no quote"
        assert (row["market_raw"], row["code"]) == key, "snapshot identity differs from quote"
        assert state["cursor"] == row["update_cursor"]
        assert len(row["bids"]) == len(row["asks"]) == 5
        rows[key] = row
    assert set(rows) == requested
    return rows


def decode(payload, decimals):
    """Known captured 0547 profile: XOR, five levels, 46-byte opaque record tail."""
    reader = Reader(bytes(b ^ 0x93 for b in payload))
    count = reader.scalar("H")
    assert count <= len(decimals) <= 100
    rows = []
    for _ in range(count):
        row = quote(reader, decimals, True)
        assert (row["market_raw"], row["code"]) in decimals, "unsubscribed symbol"
        row["tail_raw"] = list(reader.take(46))
        rows.append(row)
    assert reader.offset == len(reader.data), "unsupported tail profile or extra data"
    assert len({(r["market_raw"], r["code"]) for r in rows}) == count
    return rows


def frame(root, event):
    stem = root / event["stem"]
    header = stem.with_suffix(".header.bin").read_bytes()
    body = stem.with_suffix(".wire.bin").read_bytes()
    assert len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400")
    packed, unpacked = struct.unpack_from("<HH", header, 12)
    assert packed == len(body)
    if packed != unpacked:
        inflater = zlib.decompressobj()
        payload = inflater.decompress(body, unpacked + 1)
        assert inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail
    else:
        payload = body
    assert len(payload) == unpacked
    assert payload == stem.with_suffix(".payload.bin").read_bytes()
    opcode, message = struct.unpack_from("<H", header, 10)[0], struct.unpack_from("<I", header, 5)[0]
    if opcode == 0x0547:
        assert bytes(b ^ 0x93 for b in payload) == stem.with_suffix(".xor.bin").read_bytes()
    return opcode, message, payload


def verify(root):
    summary = json.loads((root / "summary.json").read_text())
    assert summary["schema"] == 1 and summary["opcode"] == "0x0547"
    assert summary["source"] == "live_tcp", "synthetic captures cannot prove live acceptance"
    assert summary["auto_reconnect"] == 0 and summary["heartbeat"] is False
    assert 30 <= summary["requested_seconds"] <= 600
    assert summary["guard_seconds"] == 5
    trace = root / "trace.ndjson"
    if not trace.exists():
        assert summary["status"] in {"market_closed", "calendar_uncovered", "transport_error"}
        assert not list(root.glob("*.bin"))
        assert summary["passive_frames"] == summary["passive_requests"] == 0
        return {
            "verification": "passed", "business_status": summary["status"],
            "wire_frames_checked": 0, "passive_requests": 0,
            "live_trading_push_verified": False,
            "scope": "Pre-network gate only; no baseline or push evidence.",
        }
    events = [json.loads(line) for line in trace.read_text().splitlines()]
    assert len(events) <= 8192
    counts = collections.Counter()
    decimals, directory_counts = {}, collections.Counter()
    requests, passive_requests = 0, 0
    pending, phase, last_ms = None, "warmup", 0
    baseline_rows, previous, baseline_state, push_events, assessments = None, {}, {}, [], []
    requested = {tuple(item) for item in summary["symbols"]}
    assert 1 <= len(requested) == len(summary["symbols"]) <= 100
    started_utc, finished_utc = timestamp(summary["started_utc"]), timestamp(summary["finished_utc"])
    assert finished_utc >= started_utc
    last_utc = started_utc
    seen_phases = set()
    for index, event in enumerate(events):
        assert event["id"] == index and event["stem"] == f"event-{index:05}"
        assert event["elapsed_ms"] >= last_ms
        last_ms = event["elapsed_ms"]
        stamp = timestamp(event["utc"])
        assert last_utc <= stamp <= finished_utc, "capture clock moved backwards or past finish"
        last_utc = stamp
        if event["kind"] == "phase":
            assert event["phase"] in {"warmup", "baseline", "passive"}
            assert event["phase"] not in seen_phases, "phase entered more than once"
            seen_phases.add(event["phase"])
            if event["phase"] == "baseline":
                assert phase == "warmup"
            if event["phase"] == "passive":
                assert phase == "baseline" and baseline_rows is not None and pending is None
                previous = baseline_state.copy()
                snapshot = json.loads((root / "baseline-snapshot.json").read_text())
                assert snapshot["status"] == "Synchronized"
                states = snapshot_rows(snapshot, requested, summary["connection_id"], 0)
                for key, row in states.items():
                    equal(row, previous[key])
                assert set(previous) == requested
            phase = event["phase"]
        assert event["phase"] == phase
        if phase != "warmup":
            assert event["connection_id"] == summary["connection_id"]
        kind = event["kind"]
        if kind == "request":
            assert pending is None, "overlapping outgoing requests"
            pending = (root / f'{event["stem"]}.request.bin').read_bytes()
            requests += 1
            passive_requests += phase == "passive"
            assert passive_requests == 0, "passive phase transmitted a request"
            if phase == "baseline":
                assert struct.unpack_from("<H", pending, 10)[0] == 0x0547
                assert struct.unpack_from("<H", pending, 12)[0] == len(requested)
                assert len(pending) == 14 + len(requested)*11
                cursors = [struct.unpack_from("<I", pending, 21+i*11)[0] for i in range(len(requested))]
                symbols = {(pending[14+i*11], pending[15+i*11:21+i*11].decode("ascii")) for i in range(len(requested))}
                assert symbols == requested and cursors == [0]*len(requested)
        elif kind in {"response", "push"}:
            assert event["endpoint"] == summary["endpoint"]
            opcode, message, payload = frame(root, event)
            counts[f"0x{opcode:04x}"] += 1
            if kind == "response":
                request = (root / f'{event["stem"]}.request.bin').read_bytes()
                assert request == pending, "response without its recorded request"
                assert len(request) >= 12
                assert struct.unpack_from("<H", request, 10)[0] == opcode
                assert struct.unpack_from("<I", request, 1)[0] == message
                assert request[6:8] == request[8:10]
                assert struct.unpack_from("<H", request, 6)[0] == len(request)-10
                pending = None
                if opcode == 0x044D:
                    market, start, limit = struct.unpack_from("<HII", request, 12)
                    reader = Reader(payload)
                    count = reader.scalar("H")
                    assert count <= limit <= 1600 and start == directory_counts[market]
                    for _ in range(count):
                        row = reader.take(37)
                        key = (market, row[:6].decode("ascii"))
                        assert key not in decimals
                        decimals[key] = row[28]
                        directory_counts[market] += 1
                    assert reader.offset == len(payload)
                if opcode == 0x0547:
                    assert phase == "baseline" and baseline_rows is None
                    selected = {key: decimals[key] for key in requested}
                    baseline_rows = decode(payload, selected)
                    dto = json.loads((root / "baseline.json").read_text())
                    equal(dto["page"]["records"], baseline_rows)
                    assert dto["page"]["source"]["connection_id"] == summary["connection_id"]
                    for row in baseline_rows:
                        key = (row["market_raw"], row["code"])
                        if key in baseline_state:
                            assert row["update_cursor"] >= baseline_state[key]["update_cursor"]
                        baseline_state[key] = row
            else:
                assert opcode == 0x0547
                assert event["push_connection_id"] == summary["connection_id"]
                assert event["generation"] == 0 and message == event["message_id"]
                assert started_utc <= timestamp(event["received_at"]) <= stamp
                if phase == "passive":
                    assert pending is None, "push classified while a request is in flight"
                rows = decode(payload, {key: decimals[key] for key in requested})
                dto = json.loads((root / f'{event["stem"]}.decoded.json').read_text())
                equal(dto["page"]["records"], rows)
                assert dto["page"]["source"]["connection_id"] == summary["connection_id"]
                if phase == "baseline":
                    for row in rows:
                        key = (row["market_raw"], row["code"])
                        if key in baseline_state:
                            assert row["update_cursor"] >= baseline_state[key]["update_cursor"]
                        baseline_state[key] = row
                push_events.append((event, rows))
        elif kind == "failed":
            pending = None
        elif kind == "assessment":
            assessments.append(event)
        else:
            assert kind == "phase", "unknown trace event"
        assert event["requests_so_far"] == requests
        assert stamp <= timestamp(summary["finished_utc"])
    assert sum(1 for e in events if e["kind"] == "request" and e["phase"] == "baseline") <= 1
    selected = {key: decimals[key] for key in requested if key in decimals}
    if (root / "precision.json").exists():
        assert {(m, c): d for m, c, d in json.loads((root / "precision.json").read_text())} == selected
    passive = [(e, rows) for e, rows in push_events if e["phase"] == "passive"]
    # Queued baseline notifications are drained too but never qualify after the guard.
    polled = [(e, rows) for e, rows in push_events if e["phase"] == "baseline"] + passive
    accepted = 0
    if "passive_started_utc" in summary:
        start, end = timestamp(summary["passive_started_utc"]), timestamp(summary["passive_finished_utc"])
        assert len(polled) == len(assessments) == summary["passive_frames"]
        for (event, rows), assessment in zip(polled, assessments):
            assert event["message_id"] == assessment["message_id"]
            assert event["received_at"] == assessment["received_at"]
            after_guard = timestamp(event["received_at"]) >= start + dt.timedelta(seconds=5)
            changes = []
            for row in rows:
                key = (row["market_raw"], row["code"])
                old = previous[key]
                if change(old, row):
                    changes.append({"market":key[0], "code":key[1],
                                    "previous_cursor":old["update_cursor"], "cursor":row["update_cursor"]})
                if row["update_cursor"] >= old["update_cursor"]:
                    previous[key] = row
            equal(assessment, {"after_guard":after_guard, "qualifies":after_guard and bool(changes), "changes":changes})
            accepted += after_guard and bool(changes)
        assert summary["advancing_change_frames"] == accepted
        assert summary["passive_requests"] == passive_requests
        assert summary["requests_total"] == requests and summary["events"] == len(events)
        calendar = summary["calendar"]
        window = session(start, calendar) is not None and session(start, calendar) == session(end, calendar)
        assert window == summary["trading_window_complete"]
        if summary["observation_complete"]:
            assert summary["passive_elapsed_ms"] >= summary["requested_seconds"]*1000
            assert (end - start).total_seconds() >= summary["requested_seconds"] - 0.1
        final = json.loads((root / "final-snapshot.json").read_text())
        final_connection = summary["connection_id"]
        if summary["status"] != "passed" and final["status"] in {"Invalid", "Closed"}:
            assert final["connection_id"] is None
            final_connection = None
        final_rows = snapshot_rows(final, requested, final_connection, 0)
        for key, row in final_rows.items():
            equal(row, previous[key])
        if summary["status"] == "passed":
            assert not summary["allow_closed"] and window and summary["observation_complete"]
            assert accepted >= 2 and passive_requests == 0 and final["status"] == "Synchronized"
            assert summary["baseline_complete"] and pending is None
            assert seen_phases == {"warmup", "baseline", "passive"}
            assert len(passive) >= 2
            for event, _ in passive:
                assert start <= timestamp(event["received_at"]) <= end
                assert session(timestamp(event["received_at"]), calendar) == session(start, calendar)
            assert summary.get("final_synchronized", True) is True
        if summary["allow_closed"]:
            assert summary["status"] != "passed"
    else:
        assert summary["status"] != "passed"
    return {
        "verification": "passed", "business_status": summary["status"],
        "method": "Independent stdlib framing/zlib/XOR/signed-varint/five-depth fields; captured holiday dataset, UTC+8 window and request trace replay",
        "wire_frames_checked": sum(counts.values()), "command_frames": dict(counts),
        "directory_rows_checked": sum(directory_counts.values()),
        "baseline_records_checked": len(baseline_rows or []),
        "passive_frames": len(passive), "polled_frames": len(assessments),
        "passive_requests": passive_requests, "advancing_change_frames": accepted,
        "live_trading_push_verified": summary["status"] == "passed",
        "scope": "One endpoint and symbol set. No push is not proof of server non-support; offline replay does not replace live trading-hour evidence.",
    }


class Regression(unittest.TestCase):
    def test_final_snapshot_requires_full_identity_and_generation(self):
        import copy
        row = {"market_raw": 0, "code": "000001", "update_cursor": 10,
               "bids": [0]*5, "asks": [0]*5}
        valid = {"connection_id": 7, "generation": 0, "states": [
            {"symbol": {"market": 0, "code": "000001"}, "cursor": 10, "quote": row}]}
        requested = {(0, "000001")}
        self.assertEqual(snapshot_rows(valid, requested, 7, 0), {(0, "000001"): row})
        faults = []
        for field, value in (("connection_id", 8), ("generation", 1), ("states", [])):
            faults.append({**valid, field: value})
        for key, value in (("cursor", 9), ("quote", None), ("symbol", {"market": 1, "code": "000001"})):
            fault = copy.deepcopy(valid)
            fault["states"][0][key] = value
            faults.append(fault)
        for fault in faults:
            with self.assertRaises(AssertionError):
                snapshot_rows(fault, requested, 7, 0)

    def test_changes_require_cursor_and_known_market_fields(self):
        old = {key: 0 for key in MARKET_FIELDS}
        old["update_cursor"] = 10
        new = {**old, "update_cursor": 11, "tail_raw": [99], "active_raw": 7}
        self.assertFalse(change(old, new))
        new["total_volume_raw"] = 1
        self.assertTrue(change(old, new))
        for cursor in (9, 10):
            self.assertFalse(change(old, {**new, "update_cursor": cursor}))

    def test_sessions_reject_weekends_holidays_and_close(self):
        calendar = {"coverage_start":"2026-01-01", "coverage_end":"2026-12-31",
                    "closed_weekdays":["2026-10-01"]}
        for value in ("2026-10-24T14:00:00+08:00", "2026-10-01T10:00:00+08:00",
                      "2026-10-26T11:30:00+08:00", "2026-10-26T15:00:00+08:00"):
            self.assertIsNone(session(timestamp(value), calendar))
        self.assertIsNotNone(session(timestamp("2026-10-26T09:35:00+08:00"), calendar))
        self.assertNotEqual(session(timestamp("2026-10-26T11:29:59+08:00"), calendar),
                            session(timestamp("2026-10-26T13:00:00+08:00"), calendar))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, nargs="?")
    parser.add_argument("--freeze", action="store_true", help="create the initial SHA manifest; subsequent runs require exact match")
    parser.add_argument("--self-test", action="store_true", help="run explicitly synthetic classifier/calendar regressions")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables validation assertions")
    if args.self_test:
        unittest.main(argv=["verify_quote_push"], exit=True)
    if args.directory is None:
        parser.error("directory required")
    root = args.directory
    sources = sorted(p for p in root.iterdir() if p.is_file()
                     and p.name not in {"independent-verification.json", "sha256-manifest.json"})
    digests = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest = root / "sha256-manifest.json"
    if manifest.exists():
        assert json.loads(manifest.read_text()) == digests, "captured files changed"
    else:
        assert args.freeze, "first verification requires --freeze"
    result = verify(root)
    if not manifest.exists():
        manifest.write_text(json.dumps(digests, indent=2) + "\n")
    (root / "independent-verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
