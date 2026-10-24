import copy
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile

ROOT = Path("/Users/bytedance/Documents/rustdx")
sys.path.insert(0, str(ROOT / "scripts/validation"))
from verify_resource_contract import verify as resource
from verify_quote_push import verify as push

results = []
def case(label, original, verifier, mutation):
    with tempfile.TemporaryDirectory(prefix="resource-negative-") as temp:
        root = Path(temp) / "capture"
        shutil.copytree(original, root, copy_function=lambda a, b: __import__("os").link(a, b))
        mutation(root)
        try:
            verifier(root)
        except (AssertionError, ValueError, KeyError, IndexError):
            results.append({"case": label, "rejected": True, "source": "modified_copy_synthetic"})
            return
        raise RuntimeError(f"negative case accepted: {label}")

def rewrite(root, name, action):
    path = root / name
    value = json.loads(path.read_text())
    action(value)
    path.unlink()  # break the hard link before modifying
    path.write_text(json.dumps(value))

src = Path("/tmp/rustdx-resources-final-hz-20261024")
case("incomplete_download", src, resource,
     lambda root: rewrite(root, "zhb.zip.json", lambda d: d.update(complete=False, termination="ByteBudget")))
case("candidate_value_changed", src, resource,
     lambda root: rewrite(root, "zhb.stats.json", lambda d: d["members"][0]["rows"][0]["candidate_values"]["Stat"].update(pe_ttm=999)))
case("missing_cfg_row", src, resource,
     lambda root: rewrite(root, "zhb.stats.json", lambda d: d["members"][0]["rows"].pop()))
def damage(root):
    path = root / "frame-0001.payload.bin"
    data = bytearray(path.read_bytes())
    data[0] ^= 1
    path.unlink()
    path.write_bytes(data)
case("declared_length_or_payload_corruption", src, resource, damage)
old = ROOT / "docs/testing/evidence/quote-push-validation-20261024/hz-diagnostic"
case("missing_final_symbol", old, push,
     lambda root: rewrite(root, "final-snapshot.json", lambda d: d["states"].pop()))
case("wrong_final_connection", old, push,
     lambda root: rewrite(root, "final-snapshot.json", lambda d: d.update(connection_id=0)))
case("wrong_final_generation", old, push,
     lambda root: rewrite(root, "final-snapshot.json", lambda d: d.update(generation=1)))
case("promote_closed_capture", old, push,
     lambda root: rewrite(root, "summary.json", lambda d: d.update(status="passed")))
Path("/tmp/rustdx-resource-negative.json").write_text(json.dumps(results, indent=2) + "\n")
print(json.dumps(results, indent=2))
