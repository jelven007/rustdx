#!/usr/bin/env python3
"""Offline independent stdlib replay of nine 7709 commands; no third-party SDK."""
import collections
import datetime
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import zlib
from recent_minutes_selector import date_from_selector


class Reader:
    def __init__(self, data):
        self.data, self.offset = data, 0

    def take(self, length):
        end = self.offset + length
        assert end <= len(self.data), "truncated field"
        result = self.data[self.offset:end]
        self.offset = end
        return result

    def scalar(self, fmt):
        return struct.unpack("<" + fmt, self.take(struct.calcsize("<" + fmt)))[0]

    def varint(self):
        byte = self.scalar("B")
        negative, value, shift = byte & 64, byte & 63, 6
        while byte & 128:
            byte = self.scalar("B")
            value |= (byte & 127) << shift
            shift += 7
            assert shift <= 69
        value = -value if negative else value
        assert -(2**63) <= value < 2**63
        return value


def equal(actual, expected):
    """Compare only independently decoded fields, including every raw integer."""
    if isinstance(expected, dict):
        for key, value in expected.items():
            equal(actual[key], value)
    elif isinstance(expected, list):
        assert len(actual) == len(expected)
        for left, right in zip(actual, expected):
            equal(left, right)
    elif isinstance(expected, float):
        assert math.isfinite(expected)
        assert math.isclose(actual, expected, rel_tol=1e-7, abs_tol=1e-10), (actual, expected)
    else:
        assert actual == expected, (actual, expected)


def packed_amount(raw):
    exponent, high, middle, low = raw >> 24, (raw >> 16) & 255, (raw >> 8) & 255, raw & 255
    a, b, c, d = 2*exponent-127, 2*exponent-134, 2*exponent-142, 2*exponent-150
    high_term = 2.0**b * (128 + (high & 127)*2) if high > 128 else high * (2.0**b if b >= 0 else 2.0**(-b))
    bump = 1 if high & 128 else 0
    return 2.0**a + high_term + middle*2.0**(c+bump) + low*2.0**(d+bump)


def quote(reader, decimals, refresh):
    market = reader.scalar("B")
    code = reader.take(6).decode("ascii")
    assert code.isdigit()
    active = reader.scalar("H")
    raw = [reader.varint() for _ in range(5)]
    divisor = 10**decimals[(market, code)] if (market, code) in decimals else None
    prices = [None if divisor is None else (raw[0] + (v if i else 0))/divisor for i, v in enumerate(raw)]
    clock = {"update_cursor": reader.scalar("I"), "time_raw": None} if refresh else {"time_raw": reader.varint(), "update_cursor": None}
    status, volume, current = [reader.varint() for _ in range(3)]
    amount = reader.scalar("I")
    inside, outside, after, opening = [reader.varint() for _ in range(4)]
    bids, asks = [], []
    for _ in range(5 if refresh else 1):
        bid, ask, bv, av = [reader.varint() for _ in range(4)]
        bids.append({"price": None if divisor is None else (raw[0]+bid)/divisor, "volume_raw": bv, "price_delta_raw": bid})
        asks.append({"price": None if divisor is None else (raw[0]+ask)/divisor, "volume_raw": av, "price_delta_raw": ask})
    return {
        "market_raw": market, "code": code, "active_raw": active, "price_fields_raw": raw,
        **dict(zip(["price", "previous_close", "open", "high", "low"], prices)), **clock,
        "status_or_unknown_raw": status, "total_volume_raw": volume, "current_volume_raw": current,
        "amount_raw": amount, "amount": packed_amount(amount), "inside_volume_raw": inside,
        "outside_volume_raw": outside, "after_outside_raw": after, "open_amount_raw": opening,
        "bids": bids, "asks": asks,
    }


def name_for(command, request, refresh_index, date_schema=1):
    if command == 0x0452:
        return "special-limits" if request[12:14] == b"\0\0" else "special-limits-next"
    if command == 0x051B:
        return f"aux-{request[-1]}"
    if command == 0x0FD1:
        return f"sparkline-{struct.unpack_from('<H', request, 38)[0]}"
    if command == 0x0FEB:
        date = date_from_selector(struct.unpack_from("<I", request, 12)[0], date_schema)
        return f"recent-{date}-{request[17:23].decode('ascii')}"
    if command == 0x056A:
        date, _, _, start, count = struct.unpack_from("<IIIII", request, 20)
        return f"auction-page-{start}" if count == 40 else ("auction-history" if date else "auction-current")
    if command == 0x0FFC:
        return f"money-{request[12]}-{request[14:20].decode('ascii')}"
    if command == 0x054C:
        return "snapshots"
    if command == 0x054B:
        _, sort, start, _, reverse = struct.unpack_from("<HHHHH", request, 12)
        return "category-next" if start else f"category-{'change-' if sort == 14 else ''}{str(reverse == 2).lower()}"
    if command == 0x0547:
        return f"refresh-{refresh_index}"
    return None


def verify(root):
    summary = json.loads((root / "summary.json").read_text())
    date_schema = summary.get("recent_minutes_date_schema_version", 1)
    counts, records, decimals = collections.Counter(), collections.Counter(), {}
    directory_checked, refresh_index, results = 0, 0, []
    for request_path in sorted(root.glob("frame-*.request.bin")):
        stem = str(request_path).removesuffix(".request.bin")
        request, header = request_path.read_bytes(), Path(stem + ".header.bin").read_bytes()
        body = Path(stem + ".wire.bin").read_bytes()
        assert len(header) == 16 and header[:4] == bytes.fromhex("b1cb7400")
        packed, unpacked = struct.unpack_from("<HH", header, 12)
        assert packed == len(body)
        payload = zlib.decompress(body) if packed != unpacked else body
        assert len(payload) == unpacked and payload == Path(stem + ".payload.bin").read_bytes()
        command = struct.unpack_from("<H", request, 10)[0]
        assert struct.unpack_from("<H", header, 10)[0] == command
        if command in {0x0452,0x051B,0x0FD1,0x0FEB,0x056A,0x0FFC,0x054C,0x054B,0x0547}:
            assert request[6:8] == request[8:10]
            assert struct.unpack_from("<H", request, 6)[0] == len(request)-10
            assert header[5:9] == request[1:5]
        counts[f"0x{command:04x}"] += 1
        if command == 0x044D:
            market, start, limit = struct.unpack_from("<HII", request, 12)
            reader = Reader(payload)
            count = reader.scalar("H")
            assert count <= limit <= 1600
            expected = json.loads((root / f"directory-{market}.json").read_text())
            for i in range(count):
                row = reader.take(37)
                code = row[:6].decode("ascii")
                decimal = row[28]
                equal(expected[start+i], {"code":code,"decimal":decimal,"volume_unit":struct.unpack_from("<H",row,6)[0],
                    "previous_close":struct.unpack_from("<f",row,29)[0],"unknown3_raw":list(row[33:37])})
                assert (market,code) not in decimals
                decimals[(market,code)] = decimal
                directory_checked += 1
            assert reader.offset == len(payload)
            continue
        name = name_for(command, request, refresh_index, date_schema)
        if name is None:
            continue
        if command == 0x0547:
            refresh_index += 1
        rust = json.loads((root / f"{name}.json").read_text())
        rust_rows = rust["page"]["records"] if "page" in rust else rust["records"]
        reader, decoded = Reader(payload), []
        if command == 0x0452:
            count = reader.scalar("H")
            for _ in range(count):
                market, code, upper, lower = reader.scalar("B"), reader.scalar("I"), reader.scalar("I"), reader.scalar("I")
                decoded.append({"market_raw":market,"code_number_raw":code,"code":f"{code:06}",
                    "upper_price":struct.unpack("<f",struct.pack("<I",upper))[0],"lower_price":struct.unpack("<f",struct.pack("<I",lower))[0],
                    "upper_bits":upper,"lower_bits":lower})
        elif command == 0x051B:
            count = reader.scalar("H")
            for index in range(count):
                if request[-1] == 11:
                    a, b = reader.scalar("I"), reader.scalar("I")
                    decoded.append({"VolumePair":{"index":index,"previous_day":struct.unpack("<f",struct.pack("<I",a))[0],
                        "current_day":struct.unpack("<f",struct.pack("<I",b))[0],"previous_bits":a,"current_bits":b}})
                else:
                    decoded.append({"IntegerPair":{"index":index,"series_a":reader.varint(),"series_b":reader.varint()}})
        elif command == 0x0FD1:
            assert payload[0] == request[12] and payload[2:8] == request[14:20]
            assert payload[24] == request[36]
            equal(rust, {"market_raw":payload[0],"code":payload[2:8].decode("ascii"),
                "selector_echo":payload[24],"parameter_raw":struct.unpack_from("<I",payload,26)[0],
                "capacity_raw":struct.unpack_from("<H",payload,34)[0],"base_bits":struct.unpack_from("<I",payload,36)[0]})
            reader.take(40)
            count = reader.scalar("H")
            for _ in range(count):
                bits = reader.scalar("I")
                decoded.append({"bits":bits,"price":struct.unpack("<f",struct.pack("<I",bits))[0]})
        elif command == 0x0FEB:
            count, close, opening = reader.scalar("H"), reader.scalar("I"), reader.scalar("I")
            market, code = request[16], request[17:23].decode("ascii")
            divisor = 10**decimals[(market,code)]
            equal(rust,{"date_selector_raw":struct.unpack_from("<I",request,12)[0],"server_trading_date":None,
                "previous_close_bits":close,"open_bits":opening})
            base = None
            for index in range(count):
                p, a, v = reader.varint(), reader.varint(), reader.varint()
                pr, ar = (p,a) if base is None else (base[0]+p,base[1]+a)
                if base is None:
                    base = (pr,ar)
                decoded.append({"index":index,"price_field_raw":p,"average_field_raw":a,"volume_raw":v,
                    "price_raw":pr,"average_raw":ar,"price":pr/divisor,"average_price":ar/(100*divisor)})
        elif command == 0x056A:
            count = reader.scalar("H")
            start = struct.unpack_from("<I",request,32)[0]
            for index in range(count):
                minute, bits, matched, unmatched = reader.scalar("H"), reader.scalar("I"), reader.scalar("I"), reader.scalar("i")
                reserved, second = reader.scalar("B"), reader.scalar("B")
                decoded.append({"index":start+index,"minute_raw":minute,"second_raw":second,
                    "time_label":f"{minute//60:02}:{minute%60:02}:{second:02}","price_bits":bits,
                    "price":struct.unpack("<f",struct.pack("<I",bits))[0],"matched_volume_raw":matched,
                    "unmatched_signed_raw":unmatched,"unmatched_volume_raw":abs(unmatched),
                    "unmatched_sign":(unmatched>0)-(unmatched<0),"reserved_raw":reserved})
            equal(rust,{"process_snapshots":True,"server_trading_date":None})
        elif command == 0x0FFC:
            raw_header = reader.take(40)
            assert raw_header[0] == request[12] and raw_header[2:8] == request[14:20]
            count = struct.unpack_from("<H",raw_header,38)[0] or (len(payload)-40)//88
            equal(rust,{"header_raw":list(raw_header),"reported_count_raw":struct.unpack_from("<H",raw_header,38)[0]})
            for _ in range(count):
                date = reader.scalar("I")
                raw = [reader.scalar("I") for _ in range(21)]
                total = struct.unpack("<f",struct.pack("<I",raw[1]))[0]
                buckets = [half for packed in raw[10:18] for half in (packed & 65535,packed >> 16)]
                differences = [a-b for a,b in zip(buckets[::2],buckets[1::2])]
                main, active = sum(differences[:4:2]), sum(differences[1::2])
                decoded.append({"date_raw":date,"raw":raw,"total_amount_bits":raw[1],"total_amount":total,
                    "buckets_raw":buckets,"net_amounts":[d*total/50000 for d in differences[::2]],
                    "active_buy_net_amounts":[d*total/50000 for d in differences[1::2]],
                    "main_net":main*total/50000,"main_ratio":main/500,
                    "active_buy_net":active*total/50000,"active_buy_ratio":active/500})
        else:
            if command == 0x0547:
                reader = Reader(bytes(b ^ 0x93 for b in payload))
            else:
                equal(rust,{"header_raw":reader.scalar("H")})
            count = reader.scalar("H")
            for _ in range(count):
                record = quote(reader,decimals,command == 0x0547)
                # Independent fixed layout observed in these captures; this
                # deliberately does not reuse the Rust marker partitioner.
                tail = reader.take(46 if command == 0x0547 else 56)
                record["tail_raw"] = list(tail)
                if command == 0x054B:
                    t = Reader(tail)
                    status, rise, turnover, amount, opening = t.scalar("H"),t.scalar("h"),t.scalar("h"),t.scalar("f"),t.scalar("h")
                    pair, speed, depth = t.take(10),t.scalar("f"),t.scalar("f")
                    metadata, active2 = t.take(24),t.scalar("H")
                    decoded.append({"quote":record,"status_or_sort_raw":status,"rise_speed_raw":rise,"rise_speed":rise/100,
                        "short_turnover_raw":turnover,"short_turnover":turnover/100,"two_minute_amount":amount,
                        "opening_rush_raw":opening,"opening_rush":opening/100,"extra_pair_raw":list(pair),
                        "volume_rise_speed":speed,"depth_raw":depth,"extra_metadata_raw":list(metadata),"active2_raw":active2})
                else:
                    decoded.append(record)
        assert reader.offset == len(reader.data), (name,reader.offset,len(reader.data))
        equal(rust_rows,decoded)
        records[f"0x{command:04x}"] += count
        results.append({"name":name,"opcode":f"0x{command:04x}","records_checked":count})
    quotes = json.loads((root / "quotes-053e.json").read_text())["records"]
    snapshots = json.loads((root / "snapshots.json").read_text())["page"]["records"]
    refresh = json.loads((root / "refresh-0.json").read_text())["page"]["records"]
    price_checks = []
    for snap in snapshots:
        old = next(row for row in quotes if row["code"] == snap["code"])
        update = next(row for row in refresh if row["code"] == snap["code"])
        for key in ["price","open","high","low"]:
            equal(snap[key],old[key])
            equal(update[key],snap[key])
        price_checks.append({"code":snap["code"],"price":snap["price"],"matches_053e_and_0547":True})
    pages = [json.loads((root / f"auction-page-{start}.json").read_text())["page"]["records"] for start in [0,40,80]]
    all_auction = json.loads((root / "auction-current.json").read_text())["page"]["records"]
    equal(pages[0]+pages[1],all_auction)
    assert not pages[2]
    return {"method":"Independent Python stdlib offsets, 64-bit signed varints, IEEE bits, XOR and arithmetic; no SDK execution",
        "endpoint":json.loads((root / "summary.json").read_text())["endpoint"],
        "wire_frames_checked":sum(counts.values()),"command_frames":dict(counts),"command_records":dict(records),
        "directory_rows_checked":directory_checked,"results":results,"price_checks":price_checks,
        "auction_pagination":[len(page) for page in pages],
        "scope":"Sampled parameters only; empty 0feb frames validate transport/header only; no trading-hour push or exchange-date proof"}


if __name__ == "__main__":
    root = Path(sys.argv[1])
    sources = sorted(p for p in root.iterdir() if p.is_file() and p.name not in {"independent-verification.json","sha256-manifest.json"})
    digests = {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest = root / "sha256-manifest.json"
    if manifest.exists():
        assert json.loads(manifest.read_text()) == digests, "captured source files changed"
    result = verify(root)
    manifest.write_text(json.dumps(digests,indent=2) + "\n")
    (root / "independent-verification.json").write_text(json.dumps(result,ensure_ascii=False,indent=2) + "\n")
    print(json.dumps(result,ensure_ascii=False,indent=2))
