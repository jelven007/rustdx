import hashlib
import json
from pathlib import Path

ROOT = Path("/Users/bytedance/Documents/rustdx")
path = ROOT / "config/release/offline-fixtures.json"
manifest = json.loads(path.read_text())


def bar(row):
    return {key: row[key] for key in ["date", "volume_shares", "close_price_milli", "amount_raw"]}


for row in manifest["files"]:
    if row["copy"]:
        continue
    source = ROOT / row["source"]
    original = source.read_bytes()
    if hashlib.sha256(original).hexdigest() != row["source_sha256"]:
        raise RuntimeError("independent original changed")
    data = json.loads(original)
    output = {}
    if "records" in data:
        output["records"] = [bar(value) for value in data["records"]]
    if "pages" in data:
        output["pages"] = [{"request": value["request"], "records": [bar(record) for record in value["records"]]}
                           for value in data["pages"]]
    destination = ROOT / row["path"]
    destination.write_text(json.dumps(output, separators=(",", ":")) + "\n")
    row["sha256"] = hashlib.sha256(destination.read_bytes()).hexdigest()
    row["bytes"] = destination.stat().st_size
path.write_text(json.dumps(manifest, indent=2) + "\n")
print(sum(row["bytes"] for row in manifest["files"]))
