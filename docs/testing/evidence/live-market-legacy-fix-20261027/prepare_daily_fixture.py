import hashlib
import json
from pathlib import Path
import shutil
import struct

data = Path("/private/tmp/rustdx-live-market-20261027")
out = Path("/private/tmp/rustdx-live-market-fix-20261027/crates/rustdx-core/tests/fixtures/daily-reference")
out.mkdir(exist_ok=True)
verification = json.loads((data / "focus-hz/independent-verification.json").read_text())
provenance = json.loads((data / "official-current/provenance.json").read_text())
entries = []
for archive in verification["daily_archives"]:
    market, code = archive["market"], archive["code"]
    symbol = ("sz" if market == 0 else "sh") + code
    day_path = data / f"official-current/{symbol}.day"
    member = next(x for x in provenance["members"] if x["file"] == day_path.name)
    assert hashlib.sha256(day_path.read_bytes()).hexdigest() == member["sha256"]
    shutil.copyfile(day_path, out / day_path.name)
    pages = []
    for request_path in sorted((data / "focus-hz").glob("*.request.bin")):
        request = request_path.read_bytes()
        if request[10:12] != b"\x2b\x05":
            continue
        m = struct.unpack_from("<H", request, 12)[0]
        c = request[14:20].decode()
        if (m, c) != (market, code):
            continue
        start, count = struct.unpack_from("<HH", request, 20)
        payload_path = request_path.with_name(request_path.name.replace("request.bin", "payload.bin"))
        name = f"{symbol}-{start:05}.payload.bin"
        shutil.copyfile(payload_path, out / name)
        pages.append(dict(start=start, count=count, file=name,
                          sha256=hashlib.sha256(payload_path.read_bytes()).hexdigest()))
    entries.append(dict(market=market, code=code, file=day_path.name,
                        sha256=member["sha256"], rows=archive["rows"], pages=pages,
                        differences=archive["different_rows"]))
(out / "manifest.json").write_text(json.dumps(dict(
    source_url=provenance["url"], endpoint="115.238.56.198:7709",
    captured_date="2026-10-27", symbols=entries, scope="Original day files and TCP payloads; differences are preserved."
), ensure_ascii=False, indent=2) + "\n")
print(json.dumps(dict(symbols=len(entries), rows=sum(x["rows"] for x in entries),
                     differences=sum(len(x["differences"]) for x in entries))))
