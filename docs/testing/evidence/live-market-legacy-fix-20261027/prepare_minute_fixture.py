import hashlib
import json
from pathlib import Path
import shutil

data = Path("/private/tmp/rustdx-live-market-20261027")
out = Path("/private/tmp/rustdx-live-market-fix-20261027/crates/rustdx-core/tests/fixtures/minute-boundary")
out.mkdir(exist_ok=True)
payloads = []
for path in (data / "focus-hz").glob("*.request.bin"):
    request = path.read_bytes()
    if request[10:12] == b"\xeb\x0f" and request[-6:] == b"000001":
        payloads.append(path.with_name(path.name.replace("request.bin", "payload.bin")))
assert len(payloads) == 1
shutil.copyfile(payloads[0], out / "response.bin")
reference = data / "references-late/sz000001-minutes.raw"
shutil.copyfile(reference, out / reference.name)
parsed = json.loads(reference.read_text())["data"]["sz000001"]["data"]
count = int.from_bytes(payloads[0].read_bytes()[:2], "little")
assert count == 209
rows = {x.split()[0]: x.split()[1:] for x in parsed["data"]}
controls, volumes = [], []
for index in range(count):
    minute = 571 + index if index < 120 else 781 + index - 120
    label = f"{minute // 60:02}{minute % 60:02}"
    price, cumulative, _ = rows[label]
    controls.append(dict(index=index, price_raw=round(float(price) * 100)))
    volumes.append(int(cumulative))
expected = dict(market=0, code="000001", date=20261027, decimal=2,
                source="https://web.ifzq.gtimg.cn/appstock/app/minute/query?code=sz000001",
                sha256=hashlib.sha256(reference.read_bytes()).hexdigest(), controls=controls,
                raw_cumulative_volumes=volumes, differing_indexes=[119,208],
                scope="Raw cumulative-volume comparison only; external volume unit remains unqualified.")
(out / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(dict(points=count, reference_sha256=expected["sha256"])))
