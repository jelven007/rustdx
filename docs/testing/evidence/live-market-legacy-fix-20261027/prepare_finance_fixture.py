import hashlib
import json
from pathlib import Path
import shutil
import struct

data = Path("/private/tmp/rustdx-live-market-20261027")
out = Path("/private/tmp/rustdx-live-market-fix-20261027/crates/rustdx-core/tests/fixtures/finance-q3")
out.mkdir(exist_ok=True)
pdf = data / "pingan-2026q3.pdf"
assert hashlib.sha256(pdf.read_bytes()).hexdigest() == "40257a9c9bedab90335b30c61e5c4812a18cfcdae5ead78ba475b676866dc31a"
shutil.copyfile(pdf, out / pdf.name)
payload = (data / "focus-hz/frame-042.payload.bin").read_bytes()
assert len(payload) == 145
(out / "response.bin").write_bytes(payload)
report = json.loads((data / "finance-q3-verification.json").read_text())
rows = []
for row in report["nodes"][0]["comparisons"]:
    value = row["reference_value"]
    page = row["pdf_page"]
    if row["field"] == "total_shares":
        value = 19_405_918_198
        page = 4
        text = (data / "pingan-pages/pages/page_0004.txt").read_text()
        assert "19,405,918,198" in text and "□是 √否" in text
    text_path = data / f"pingan-pages/pages/page_{page:04}.txt"
    shutil.copyfile(text_path, out / text_path.name)
    expected_bits = struct.unpack("<I", struct.pack("<f", value / row["candidate_scale"]))[0]
    raw_bits = struct.unpack_from("<I", payload, 9 + row["offset"])[0]
    assert raw_bits == row["raw_bits"]
    matches = row["field"] != "cash_flow_net_change"
    assert (raw_bits == expected_bits) == matches
    rows.append(dict(field=row["field"], offset=row["offset"], multiplier=row["candidate_scale"],
                     official=value, expected_bits=expected_bits, raw_bits=raw_bits,
                     verified=matches, pdf_page=page,
                     text_sha256=hashlib.sha256(text_path.read_bytes()).hexdigest()))
expected = dict(schema=1, market=0, code="000001", updated_date=20261024,
                report_period="2026-09-30", reference_url=report["reference_url"],
                reference_pdf_sha256=report["reference_pdf_sha256"],
                payload_sha256=hashlib.sha256(payload).hexdigest(), fields=rows,
                scope="Exact binary32 checks against the issuer disclosure; no source value substitution.")
(out / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(dict(verified=sum(r["verified"] for r in rows), conflicts=1,
                     payload_sha256=expected["payload_sha256"])))
