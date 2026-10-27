import datetime as dt
import json
from pathlib import Path
import struct
import sys

source = Path("/private/tmp/rustdx-live-market-fix-20261027")
art = Path(__file__).resolve().parent
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
        for a, b in zip(actual, expected):
            exact(a, b)
    else:
        assert actual == expected, (actual, expected)

def updates(name):
    root = art / name
    summary = json.loads((root / "summary.json").read_text())
    assert summary["status"] == "captured"
    assert summary["heartbeat"] is False and summary["auto_reconnect"] == 0
    assert len(summary["refreshes"]) == 4
    assert all(row["origin"] == "requested_refresh" for row in summary["refreshes"])
    decimals = {(m,c):d for m,c,d in json.loads((root / "precision.json").read_text())}
    stocks = {tuple(x) for x in summary["symbols"]}
    previous, changes, frames = {}, [], 0
    for path in sorted(root.glob("*.request.bin")):
        opcode, request, payload = read_frame(path, summary["endpoint"])
        frames += 1
        if opcode == 0x000d:
            continue
        assert opcode == 0x0547
        count = struct.unpack_from("<H", request, 12)[0]
        assert count == 3 and len(request) == 14 + count * 11
        for i in range(count):
            offset = 14 + i * 11
            key = request[offset], request[offset+1:offset+7].decode()
            cursor = struct.unpack_from("<I", request, offset+7)[0]
            assert key in stocks
            assert cursor == (previous[key]["update_cursor"] if previous else 0)
        rows = decode(payload, decimals)
        index = len(changes)
        exact(json.loads((root / f"refresh-{index}.json").read_text())["page"]["records"], rows)
        state = json.loads((root / f"snapshot-{index}.json").read_text())
        actual = snapshot_rows(state, stocks, summary["refreshes"][0]["connection_id"], 0)
        for row in rows:
            exact(actual[row["market_raw"],row["code"]], row)
        advanced = [{"market":key[0], "code":key[1],
                     "previous_cursor":previous[key]["update_cursor"],
                     "cursor":actual[key]["update_cursor"]}
                    for key in previous if change(previous[key], actual[key])]
        changes.append(dict(index=index, returned_rows=len(rows), market_changes=advanced))
        previous = actual
    assert frames == 5 and len(changes) == 4
    return dict(endpoint=summary["endpoint"], frame_checks=frames, update_calls=4,
                same_connection_and_generation=True, origin="requested_refresh",
                market_changes_observed=any(x["market_changes"] for x in changes), changes=changes,
                started_utc=summary["started_utc"], finished_utc=summary["finished_utc"],
                passive_push_verified=False)

def window(name):
    root = art / name
    summary = json.loads((root / "summary.json").read_text())
    assert summary["status"] == "captured"
    decimals = {(m,c):d for m,c,d in json.loads((root / "precision.json").read_text())}
    results = {}
    price_checks = []
    day_bytes = Path("/private/tmp/rustdx-live-market-20261027/official-current/sz000001.day").read_bytes()
    day_rows = {struct.unpack_from("<I",day_bytes,i)[0]: struct.unpack_from("<4I",day_bytes,i+4)
                for i in range(0,len(day_bytes),32)}
    for path in sorted(root.glob("*.request.bin")):
        opcode, request, payload = read_frame(path, summary["endpoint"])
        if opcode == 0x000d:
            continue
        assert opcode == 0x0feb
        selector = struct.unpack_from("<I", request, 12)[0]
        date = date_from_selector(selector, 2)
        market, code = request[16], request[17:23].decode()
        reader = Reader(payload)
        expected = recent(reader, 10 ** decimals[market,code])
        assert reader.offset == len(payload)
        dto = json.loads((root / f"recent-{date}-{market}-{code}.json").read_text())
        exact(dto, expected)
        assert dto["date_qualification"] == {"Unverified":{"reason":"ServerDoesNotEchoTradingDate"}}
        results[date,market,code] = len(expected["page"]["records"])
        if (market,code) == (0,"000001"):
            reference = day_rows[date]
            open_matches = round(dto["open_price"]*100) == reference[0]
            close_matches = dto["page"]["records"][-1]["price_raw"] == reference[3]
            assert open_matches and close_matches
            price_checks.append(dict(requested_date=date, reference="official-current/sz000001.day",
                                     open_matches=True, close_matches=True))
    assert len(results) == len(summary["results"]) == 33
    for row in summary["results"]:
        assert results[row["requested_date"],row["market"],row["code"]] == row["points"]
    return dict(endpoint=summary["endpoint"], probes=33, wire_replay="passed",
                results=summary["results"], independent_day_open_close_checks=price_checks,
                actual_trading_dates_qualified=False,
                retention_window_guaranteed=False)

if __name__ == "__main__":
    report = dict(schema=1, checked_utc=dt.datetime.now(dt.timezone.utc).isoformat(),
                  updates=[updates("update-hz"),updates("update-sh")])
    for name in sys.argv[1:]:
        report.setdefault("windows", []).append(window(name))
    (art / "live-replay.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False))
