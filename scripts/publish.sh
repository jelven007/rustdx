#!/bin/bash
set -euo pipefail

# 发布顺序固定：先核心库，索引可见后再 CLI（CLI 依赖已发布的核心版本）。
CORE="jelven-rustdx-complete"
CLI="jelven-rustdx-cli"
export RUSTDX_SKIP_INTEGRATION_TESTS=1

if [ ! -f "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" ]; then
    echo "crates.io credentials not found; run cargo login first" >&2
    exit 1
fi

cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo publish -p "$CORE" --dry-run --locked

read -r -p "Publish $CORE and $CLI to crates.io? Type yes: " confirm
if [ "$confirm" != "yes" ]; then
    echo "Publish cancelled"
    exit 1
fi

cargo publish -p "$CORE" --locked
echo "Published: https://crates.io/crates/$CORE"

# 等核心库在索引可见后再发 CLI（cargo publish 自身会等待上传可用）。
cargo publish -p "$CLI" --locked
echo "Published: https://crates.io/crates/$CLI"
