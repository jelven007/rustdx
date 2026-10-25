#!/bin/bash
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."

# 发布顺序固定：先核心库，索引可见后再 CLI（CLI 依赖已发布的核心版本）。
CORE="jelven-rustdx-complete"
CLI="jelven-rustdx-cli"
export RUSTDX_SKIP_INTEGRATION_TESTS=1

if [ ! -f "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" ]; then
    echo "crates.io credentials not found; run cargo login first" >&2
    exit 1
fi

# A clean source tree, MSRV checks and both unpacked packages are mandatory.
# The CLI package is compiled against the extracted new core before any upload.
review_dir="$(mktemp -d "${TMPDIR:-/tmp}/rustdx-release-review.XXXXXX")"
rmdir "$review_dir"
python3 scripts/release/preflight.py --output "$review_dir"
python3 scripts/release/release_gate.py --preflight "$review_dir/preflight.json" \
    --output "$review_dir/release-manifest.json"
cargo publish -p "$CORE" --dry-run --locked

read -r -p "Publish $CORE and $CLI to crates.io? Type yes: " confirm
if [ "$confirm" != "yes" ]; then
    echo "Publish cancelled"
    exit 1
fi

# Do not upload different bytes after an interactive wait.
python3 - "$review_dir/preflight.json" "$review_dir/release-manifest.json" <<'PY'
import importlib.util
import json
import sys
from pathlib import Path
sys.path.insert(0, "scripts/release")
spec = importlib.util.spec_from_file_location("preflight", "scripts/release/preflight.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
report = json.loads(Path(sys.argv[1]).read_text())
gate = json.loads(Path(sys.argv[2]).read_text())
_, current = module.inventory()
if not gate["publication_qualified"] or current != report["source_sha256"] or gate["head"] != report["head"] or module.git("status", "--porcelain=v1", "-z"):
    raise SystemExit("source changed or preflight incomplete; rerun the review")
for package in report["packages"]:
    if module.sha((Path(sys.argv[1]).parent / package["crate"]).read_bytes()) != package["sha256"]:
        raise SystemExit("reviewed package changed")
PY

# Re-query reruns/cancellations after the interactive wait, before the first upload.
python3 scripts/release/release_gate.py --preflight "$review_dir/preflight.json" \
    --output "$review_dir/release-manifest-confirmed.json"
cargo publish -p "$CORE" --locked
echo "Published: https://crates.io/crates/$CORE"

# 等核心库在索引可见后再发 CLI（cargo publish 自身会等待上传可用）。
cargo publish -p "$CLI" --dry-run --locked
cargo publish -p "$CLI" --locked
echo "Published: https://crates.io/crates/$CLI"
