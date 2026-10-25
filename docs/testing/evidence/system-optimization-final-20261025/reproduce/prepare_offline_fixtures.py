from pathlib import Path
import hashlib
import json
import shutil

ROOT = Path("/Users/bytedance/Documents/rustdx")
records = []


def copy(source, destination, transform=None):
    source, destination = ROOT / source, ROOT / destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise RuntimeError(f"new fixture already exists: {destination}")
    if transform:
        value = transform(json.loads(source.read_text()))
        destination.write_text(json.dumps(value, separators=(",", ":")) + "\n")
    else:
        shutil.copyfile(source, destination)
    records.append({"source": str(source.relative_to(ROOT)), "path": str(destination.relative_to(ROOT)),
                    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                    "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
                    "bytes": destination.stat().st_size, "copy": transform is None})


for name in ["tcp-live-0", "tcp-live-1"]:
    base = Path("docs/testing/evidence/acceptance-stage5-20261024") / name
    for p in (ROOT / base).rglob("*"):
        if p.is_file() and (p.relative_to(ROOT / base).as_posix() == "tcp-request.json"
                           or p.relative_to(ROOT / base).as_posix().startswith("raw/sh600000/")):
            copy(p.relative_to(ROOT), Path("apps/rustdx-cli/tests/fixtures/tcp-day-legacy") / name / p.relative_to(ROOT / base))

base = Path("docs/testing/evidence/enhanced-history-amount-20261024/wire")
for p in (ROOT / base).glob("*.bin"):
    copy(p.relative_to(ROOT), Path("crates/rustdx-core/tests/fixtures/trade-amount/wire") / p.name)

for start in [0, 1800, 3600, 4876]:
    base = Path("docs/testing/evidence/enhanced-history-0fc6-20261024") / f"live-start-{start}"
    for name in ["frame-01.request.bin", "frame-01.header.bin", "frame-01.wire.bin", "frame-01.payload.bin", "expected-records.json"]:
        copy(base / name, Path("crates/rustdx-core/tests/fixtures/enhanced-history") / f"live-start-{start}" / name)


def bar(row):
    return {key: row[key] for key in ["date", "volume_shares", "close_price_milli", "amount_raw"]}


def bars(data):
    if "records" in data:
        return {"records": [bar(row) for row in data["records"]]}
    return {"pages": [{"request": row["request"], "records": [bar(bar_row) for bar_row in row["records"]]}
                      for row in data["pages"]]}


for name in ["live-a", "live-b"]:
    base = Path("docs/testing/evidence/eltdx-3.3.0-commands-20261024") / name
    target = Path("crates/rustdx-core/tests/fixtures/eltdx-330-replay") / name
    proof = json.loads((ROOT / base / "independent-verification.json").read_text())
    copy(base / "independent-verification.json", target / "independent-verification.json")
    for frame in proof["frames"]:
        for suffix in ["request", "header", "wire", "payload"]:
            filename = f"{frame['stem']}.{suffix}.bin"
            copy(base / filename, target / filename)
    for p in (ROOT / base).glob("delisted-*.json"):
        copy(p.relative_to(ROOT), target / p.name, bars)

manifest = ROOT / "config/release/offline-fixtures.json"
manifest.parent.mkdir(parents=True, exist_ok=True)
manifest.write_text(json.dumps({"schema_version": 1, "scope": "Existing assertions only; originals unchanged.",
                                "derived_bars": "Only already asserted date, integer shares, close milli, amount raw retained; independent source SHA preserved.",
                                "files": records}, indent=2) + "\n")
print(json.dumps({"files": len(records), "bytes": sum(row["bytes"] for row in records)}))
