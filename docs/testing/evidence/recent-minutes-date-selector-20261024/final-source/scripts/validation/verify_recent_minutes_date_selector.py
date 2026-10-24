#!/usr/bin/env python3
"""Verify corrected 0feb dates against 0fb4, dated bars, official day and legacy bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from evidence_paths import workspace_path
from recent_minutes_selector import date_from_selector
from verify_recent_auction import read_frame, verify
from verify_recent_minutes_boundaries import decode


HOSTS = ["115.238.56.198", "180.153.18.170"]
LEGACY = Path("docs/testing/evidence/recent-auction-contracts-20261024")
FIXTURES = Path("crates/rustdx-core/tests/fixtures/recent_minutes_nonempty")


def frozen_capture(root):
    expected = json.loads((root / "sha256-manifest.json").read_text())
    actual = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
              for p in root.iterdir() if p.is_file()
              and p.name not in {"independent-verification.json", "sha256-manifest.json"}}
    assert actual == expected, f"capture changed: {root}"


def series(root):
    summary = json.loads((root / "summary.json").read_text())
    decimals = {(m, c): d for m, c, d in json.loads((root / "precision.json").read_text())}
    result = {}
    for path in sorted(root.glob("frame-*.request.bin")):
        opcode, request, payload = read_frame(path, summary["endpoint"])
        if opcode != 0x0FEB:
            continue
        raw = struct.unpack_from("<I", request, 12)[0]
        annotation = date_from_selector(raw, summary.get("recent_minutes_date_schema_version", 1))
        key = (request[16], request[17:23].decode(), annotation)
        assert key not in result
        result[key] = {
            "payload": payload, "frame": path.name.removesuffix(".request.bin"),
            "selector": raw, "decimal": decimals[key[:2]],
        }
    return result


def check(root):
    node_checks, legacy_checks = [], []
    captures = {}
    for host in HOSTS:
        current = root / host
        frozen_capture(current)
        result = verify(current)
        assert result["recent_minutes_date_schema_version"] == 2
        assert result["recent_requested_date_business_verified"]
        assert result["date_selector_checks"]["same_day_controls_match"]
        assert result["date_selector_checks"]["dated_daily_open_close_match"]
        captures[host] = series(current)
        node_checks.append(result)
        for suffix in ["", "-archive"]:
            old = workspace_path(str(LEGACY / (host + suffix)))
            frozen_capture(old)
            # Replay the historical annotation scheme without writing its result.
            assert verify(old)["verification"] == "passed"
        old_rows = series(workspace_path(str(LEGACY / (host + "-archive"))))
        for key, old in sorted(old_rows.items()):
            market, code, annotation = key
            if annotation not in {20260817, 20260818}:
                continue
            selected = date_from_selector(old["selector"], 2)
            assert selected == {20260817: 20260609, 20260818: 20260610}[annotation]
            new = captures[host][(market, code, selected)]
            correct_august = captures[host][key]
            assert old["payload"] == new["payload"]
            assert old["payload"] != correct_august["payload"]
            legacy_checks.append({
                "endpoint": host + ":7709", "market": market, "code": code,
                "legacy_requested_date": annotation, "selected_wire_date": selected,
                "selector_raw": old["selector"], "legacy_frame": old["frame"],
                "corrected_june_frame": new["frame"],
                "payload_sha256": hashlib.sha256(old["payload"]).hexdigest(),
                "identical_to_corrected_june": True, "different_from_corrected_august": True,
            })
    assert len(legacy_checks) == 24
    first, second = [captures[host] for host in HOSTS]
    assert first.keys() == second.keys()
    cross_node_checks = []
    for key in sorted(first):
        left, right = first[key], second[key]
        lc, lo, lp = decode(left["payload"], 10**left["decimal"])
        rc, ro, rp = decode(right["payload"], 10**right["decimal"])
        assert (lc, lo, len(lp)) == (rc, ro, len(rp))
        assert all(a["price_raw"] == b["price_raw"] for a, b in zip(lp, rp))
        averages = [{
            "index": a["index"], "left_average_raw": a["average_raw"],
            "right_average_raw": b["average_raw"],
        } for a, b in zip(lp, rp) if a["average_raw"] != b["average_raw"]]
        volumes = [{
            "index": a["index"], "left_volume_raw": a["volume_raw"],
            "right_volume_raw": b["volume_raw"],
        } for a, b in zip(lp, rp) if a["volume_raw"] != b["volume_raw"]]
        cross_node_checks.append({
            "market": key[0], "code": key[1], "requested_date": key[2],
            "points": len(lp), "payload_identical": left["payload"] == right["payload"],
            "previous_close_open_price_match": True,
            "average_mismatches": averages,
            "volume_mismatches": volumes,
        })

    fixtures = workspace_path(str(FIXTURES))
    manifest_bytes = (fixtures / "manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    frozen_files = 1
    for fixture in manifest["fixtures"].values():
        for name, digest in fixture["sha256"].items():
            assert hashlib.sha256((fixtures / name).read_bytes()).hexdigest() == digest
            frozen_files += 1

    official = root / "official"
    provenance = json.loads((official / "provenance.json").read_text())
    data = (official / "sh600000.day").read_bytes()
    assert hashlib.sha256(data).hexdigest() == provenance["member"]["sha256"]
    assert len(data) == provenance["member"]["bytes"] == 32 * provenance["member"]["rows"]
    daily = {}
    previous = None
    for offset in range(0, len(data), 32):
        date, opening, high, low, close, amount, volume, reserved = struct.unpack_from("<IIIIIfII", data, offset)
        assert date not in daily
        daily[date] = {
            "date": date, "open": opening / 100, "close": close / 100,
            "previous_close": previous, "volume_shares": volume, "byte_offset": offset,
        }
        previous = close / 100
    official_checks = []
    for host in HOSTS:
        for date in [20260609, 20260610, 20260817, 20260818, 20261022]:
            entry = captures[host][(1, "600000", date)]
            cb, ob, points = decode(entry["payload"], 10**entry["decimal"])
            close, opening = [struct.unpack("<f", struct.pack("<I", bits))[0] for bits in [cb, ob]]
            day = daily[date]
            tolerance = 0.5 * 10**-entry["decimal"]
            assert abs(close - day["previous_close"]) < tolerance
            assert abs(opening - day["open"]) < tolerance
            assert points[-1]["price"] == day["close"]
            raw_sum = sum(p["volume_raw"] for p in points)
            official_checks.append({
                "endpoint": host + ":7709", **day, "frame": entry["frame"],
                "previous_close_matches": True, "open_matches": True, "last_price_matches": True,
                "volume_sum_raw": raw_sum,
                "candidate_volume_times_100_minus_official_shares": raw_sum * 100 - day["volume_shares"],
                "volume_unit_qualified": False,
            })
    return {
        "verification": "passed", "selector_conflict_resolved_in_sample": True,
        "root_cause": "Legacy Gregorian-ordinal selector encoded -20260609/-20260610 for requested 20260817/20260818",
        "corrected_date_schema_version": 2,
        "node_checks": node_checks,
        "cross_node_checks": cross_node_checks,
        "identical_cross_node_series": sum(r["payload_identical"] for r in cross_node_checks),
        "legacy_wrong_date_checks": legacy_checks, "official_daily_checks": official_checks,
        "legacy_fixture_files_unchanged": frozen_files,
        "legacy_fixture_manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
        "server_date_qualification": "Unverified: ServerDoesNotEchoTradingDate",
        "scope": "Two nodes, six symbols, six trading dates and two empty dates. No general date qualification, retention-window or share-unit acceptance.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables validation")
    result = check(args.directory)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({
        "verification": result["verification"],
        "legacy_wrong_date_checks": len(result["legacy_wrong_date_checks"]),
        "official_daily_checks": len(result["official_daily_checks"]),
        "identical_cross_node_series": result["identical_cross_node_series"],
    }))


if __name__ == "__main__":
    main()
