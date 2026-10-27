#!/usr/bin/env python3
"""Independent stdlib replay of the fixed 2026-10-27 upstream capability samples."""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import struct
import zlib
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def instant(text):
    value = dt.datetime.fromisoformat(text.replace("Z", "+00:00"))
    assert value.tzinfo is not None
    return value


class Reader:
    def __init__(self, data):
        self.data, self.offset = data, 0

    def scalar(self, fmt):
        size = struct.calcsize("<" + fmt)
        assert self.offset + size <= len(self.data), "truncated field"
        value = struct.unpack_from("<" + fmt, self.data, self.offset)[0]
        self.offset += size
        return value

    def varint(self):
        byte = self.scalar("B")
        negative, value, shift = byte & 64, byte & 63, 6
        while byte & 128:
            byte = self.scalar("B")
            value |= (byte & 127) << shift
            shift += 7
            assert shift <= 69, "varint overflow"
        value = -value if negative else value
        assert -(2**63) <= value < 2**63
        return value


def decode(payload, divisor):
    reader = Reader(payload)
    count, close, opening = [reader.scalar(fmt) for fmt in ("H", "I", "I")]
    assert 0 <= count <= 240
    prices = [struct.unpack("<f", struct.pack("<I", bits))[0] for bits in (close, opening)]
    assert all(0 <= value < float("inf") for value in prices)
    rows, base = [], None
    for index in range(count):
        price, average, volume = [reader.varint() for _ in range(3)]
        resolved = (price, average) if base is None else (base[0] + price, base[1] + average)
        assert all(0 <= value < 2**63 for value in (*resolved, volume))
        if base is None:
            base = resolved
        minute = 571 + index if index < 120 else 781 + index - 120
        rows.append(dict(
            index=index, time_label=f"{minute//60:02}:{minute%60:02}",
            price_field_raw=price, average_field_raw=average, volume_raw=volume,
            price_raw=resolved[0], average_raw=resolved[1],
            price=resolved[0] / divisor, average_price=resolved[1] / (100 * divisor),
        ))
    assert reader.offset == len(payload), "trailing payload"
    return close, opening, prices, rows


def frame(request, header, wire, recorded_payload):
    assert len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400"), "header magic"
    assert len(request) >= 12
    opcode = struct.unpack_from("<H", request, 10)[0]
    assert struct.unpack_from("<H", header, 10)[0] == opcode, "opcode mismatch"
    assert header[5:9] == request[1:5], "message mismatch"
    packed, unpacked = struct.unpack_from("<HH", header, 12)
    assert packed == len(wire), "wire length"
    if packed == unpacked:
        payload = wire
    else:
        inflater = zlib.decompressobj()
        payload = inflater.decompress(wire, unpacked + 1)
        assert inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail
    assert len(payload) == unpacked and payload == recorded_payload, "payload mismatch"
    return opcode, payload


def replay(source, captures):
    with zipfile.ZipFile(captures) as archive:
        names = archive.namelist()
        assert len(names) == len(set(names)) and archive.testzip() is None
        assert all(not name.startswith("/") and ".." not in Path(name).parts for name in names)
        read = archive.read
        groups = sorted({name.split("/")[0] for name in names})
        expected_groups = {f"window-{node}-{run}" for node in ("hz", "sh")
                           for run in ("wide", "boundary", "early", "june")}
        assert set(groups) == expected_groups
        endpoints = {"hz": "115.238.56.198:7709", "sh": "180.153.18.170:7709"}
        by_node = {node: {} for node in endpoints}
        run_results, frames, records, repeated, nonempty = [], 0, 0, 0, 0
        for group in groups:
            prefix = group + "/"
            node = group.split("-")[1]
            endpoint = endpoints[node]
            summary = json.loads(read(prefix + "summary.json"))
            dates = json.loads(read(prefix + "dates.json"))
            decimals = {(m, c): d for m, c, d in json.loads(read(prefix + "precision.json"))}
            assert summary["schema"] == 1 and summary["source"] == "live_tcp"
            assert summary["synthetic"] is False and summary["status"] == "captured"
            assert summary["endpoint"] == endpoint
            started, finished = [instant(summary[key]) for key in ("started_utc", "finished_utc")]
            assert started <= finished
            samples, connection = {}, None
            stems = sorted(name.removesuffix(".request.bin") for name in names
                           if name.startswith(prefix) and name.endswith(".request.bin"))
            for stem in stems:
                request, header, wire, recorded = [read(stem + "." + ext)
                    for ext in ("request.bin", "header.bin", "wire.bin", "payload.bin")]
                opcode, payload = frame(request, header, wire, recorded)
                assert read(stem + ".endpoint.txt").decode() == endpoint
                received = instant(read(stem + ".utc.txt").decode())
                frames += 1
                if opcode != 0x0FEB:
                    continue  # Handshakes occur before the tool's sample start.
                assert started <= received <= finished
                assert len(request) == 23 and request[0] == 12 and request[5] == 1
                assert request[6:8] == request[8:10] == b"\x0d\x00"
                selector = struct.unpack_from("<I", request, 12)[0]
                date = (2**32 - selector) & 0xFFFFFFFF
                dt.date(date // 10000, date // 100 % 100, date % 100)
                market, code = request[16], request[17:23].decode("ascii")
                key = (date, market, code)
                assert key not in samples, "duplicate selector in run"
                decimal = decimals[(market, code)]
                assert decimal in (2, 3, 4)
                close, opening, prices, rows = decode(payload, 10**decimal)
                dto = json.loads(read(prefix + f"recent-{date}-{market}-{code}.json"))
                expected = dict(
                    requested_date=date, server_trading_date=None, date_selector_raw=selector,
                    previous_close_bits=close, open_bits=opening,
                    previous_close=prices[0], open_price=prices[1],
                    date_qualification={"Unverified": {"reason": "ServerDoesNotEchoTradingDate"}},
                    date_reference=None, scale={"decimal": decimal, "divisor": 10**decimal},
                    request={"requested_date": date, "selector": selector,
                             "symbol": {"market": market, "code": code}},
                    wire_request=list(request),
                )
                for name, value in expected.items():
                    assert dto[name] == value, (group, key, name)
                assert dto["page"]["records"] == rows, (group, key, "decoded rows")
                metadata = dto["page"]["source"]
                assert metadata["opcode"] == 0x0FEB and metadata["endpoint"] == endpoint
                assert metadata["metadata_generation"] == 0
                if connection is None:
                    connection = metadata["connection_id"]
                assert metadata["connection_id"] == connection and connection > 0
                assert received <= instant(metadata["fetched_at"]) <= finished
                samples[key] = len(rows)
                records += len(rows)
                nonempty += bool(rows)
                if key in by_node[node]:
                    repeated += 1
                    assert by_node[node][key] == (payload, rows), "repeated sample changed"
                by_node[node][key] = (payload, rows)
            expected_keys = {(date, market, code) for date in dates
                             for market, code in ((0, "000001"), (0, "159915"), (1, "510300"))}
            assert set(samples) == expected_keys
            assert len(summary["results"]) == len(samples)
            for result in summary["results"]:
                key = (result["requested_date"], result["market"], result["code"])
                assert result["points"] == samples[key] and result["actual_date_qualified"] is False
                assert result["file"] == f"recent-{key[0]}-{key[1]}-{key[2]}.json"
            run_results.append(dict(run=group, endpoint=endpoint, requests=len(samples),
                                   nonempty=sum(bool(n) for n in samples.values()),
                                   records=sum(samples.values()), started_utc=str(started),
                                   finished_utc=str(finished)))
        assert set(by_node["hz"]) == set(by_node["sh"])
        cross_differences = []
        for key in sorted(by_node["hz"]):
            left_payload, left_rows = by_node["hz"][key]
            right_payload, right_rows = by_node["sh"][key]
            if left_payload == right_payload and left_rows == right_rows:
                continue
            fields = ("price_raw", "average_raw", "volume_raw",
                      "price_field_raw", "average_field_raw")
            indexes = {field: [i for i, (left, right) in enumerate(zip(left_rows, right_rows))
                               if left[field] != right[field]] for field in fields}
            different = [i for i, (left, right) in enumerate(zip(left_rows, right_rows))
                         if left != right]
            cross_differences.append(dict(
                key=list(key), left_count=len(left_rows), right_count=len(right_rows),
                left_payload_sha256=digest(left_payload), right_payload_sha256=digest(right_payload),
                header_bits_differ=[name for offset, name in ((2, "previous_close"), (6, "open"))
                                    if left_payload[offset:offset+4] != right_payload[offset:offset+4]],
                differing_indexes=indexes,
                examples=[dict(index=i, hz=left_rows[i], sh=right_rows[i])
                          for i in (different[:1] + different[-1:] if different else [])],
            ))
    day_path = source / "docs/testing/evidence/live-market-legacy-fix-20261027/official-current/sz000001.day"
    day = day_path.read_bytes()
    assert digest(day) == "d0c169a32b03bb63e2588512d630e5021c73318e315303205ca7a6eaebc67173"
    assert len(day) % 32 == 0
    day_rows = {struct.unpack_from("<I", day, start)[0]: day[start:start + 32]
                for start in range(0, len(day), 32)}
    day_checks, day_differences, day_missing, boundaries = [], [], [], []
    for node, samples in by_node.items():
        for (date, market, code), (payload, rows) in sorted(samples.items()):
            if (market, code) != (0, "000001") or not rows:
                continue
            if date not in day_rows:
                day_missing.append([node, date])
                continue
            official = day_rows[date]
            values = dict(
                open_bits=[struct.unpack_from("<I", payload, 6)[0],
                           struct.unpack("<I", struct.pack(
                               "<f", struct.unpack_from("<I", official, 4)[0] / 100))[0]],
                close_price_raw=[rows[-1]["price_raw"], struct.unpack_from("<I", official, 16)[0]],
            )
            differences = {name: value for name, value in values.items() if value[0] != value[1]}
            day_checks.append([node, date])
            if differences:
                day_differences.append(dict(node=node, date=date, differences=differences))
    for market, code in ((0, "000001"), (0, "159915"), (1, "510300")):
        available = sorted(date for (date, m, c), (_, rows) in by_node["hz"].items()
                           if (m, c) == (market, code) and rows)
        empty = sorted(date for (date, m, c), (_, rows) in by_node["hz"].items()
                       if (m, c) == (market, code) and not rows)
        boundaries.append(dict(market=market, code=code, earliest_sampled_nonempty=available[0],
                               prior_sampled_empty=max(d for d in empty if d < available[0]),
                               nonempty_dates=available, empty_dates=empty))
    fixture = source / "crates/rustdx-core/tests/fixtures"
    cash = json.loads((fixture / "finance-cash-basis/expected.json").read_text())
    assert digest((fixture / "finance-q3/pingan-2026q3.pdf").read_bytes()) == cash["reference_pdf_sha256"]
    text = (fixture / "finance-q3/page_0033.txt").read_bytes()
    assert digest(text) == cash["reference_text_sha256"]
    lines = text.decode().splitlines()
    fields = {
        "operating": "经营活动产生的现金流量净额",
        "investing": "投资活动产生/(使用)的现金流量净额",
        "financing": "筹资活动使用的现金流量净额",
        "exchange_rate_effect": "四、汇率变动对现金及现金等价物的影响",
        "reported_net_change": "五、现金及现金等价物净增加/(减少)额",
    }
    for key, label in fields.items():
        matches = [line.removeprefix(label).split()[0] for line in lines if line.startswith(label)]
        assert len(matches) == 1
        token = matches[0].replace(",", "")
        value = -int(token[1:-1]) if token.startswith("(") else int(token)
        assert value == cash[key], key
    total = cash["operating"] + cash["investing"] + cash["financing"]
    assert total == cash["total_excluding_exchange_rate_effect"] == 8515
    assert total + cash["exchange_rate_effect"] == cash["reported_net_change"] == 5872
    finance = (fixture / "finance-q3/response.bin").read_bytes()
    bits = struct.unpack_from("<I", finance, 9 + cash["source_offset"])[0]
    assert bits == cash["source_raw_bits"]
    assert struct.unpack("<f", struct.pack("<I", bits))[0] == total * 1000
    return dict(
        schema=1, status="passed", date_selector_schema=2, frames_checked=frames,
        minute_requests=456, actual_minute_requests=sum(row["requests"] for row in run_results),
        nonempty_requests=nonempty, empty_requests=456-nonempty, records_checked=records,
        unique_date_symbol_keys_per_node=len(by_node["hz"]), repeated_requests=repeated,
        runs=run_results, cross_node_differences=cross_differences, boundaries=boundaries,
        official_day_sha256=digest(day), official_open_close_samples_checked=day_checks,
        official_open_close_node_pairs=len(day_checks), official_day_differences=day_differences,
        official_day_missing_samples=day_missing,
        cash_reconciliation=dict(status="passed", total_excluding_fx_million=total,
                                 fx_million=cash["exchange_rate_effect"],
                                 reported_net_million=cash["reported_net_change"],
                                 source_bits=bits),
        scope="Fixed samples and exact replay only. Daily endpoints do not qualify all minute "
              "points, actual dates, continuous history, retention TTL or passive push.",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument("--captures", type=Path, default=Path(__file__).with_name("raw-captures.zip"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python -O disables validation")
    result = replay(args.source, args.captures)
    assert result["actual_minute_requests"] == result["minute_requests"]
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in (
        "status", "frames_checked", "minute_requests", "nonempty_requests",
        "empty_requests", "records_checked", "official_open_close_node_pairs")}))


if __name__ == "__main__":
    main()
