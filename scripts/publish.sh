#!/bin/bash
set -euo pipefail

PACKAGE="jelven-rustdx-complete"
export RUSTDX_SKIP_INTEGRATION_TESTS=1

if [ ! -f "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" ]; then
    echo "crates.io credentials not found; run cargo login first" >&2
    exit 1
fi

cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo publish -p "$PACKAGE" --dry-run --locked

read -r -p "Publish $PACKAGE to crates.io? Type yes: " confirm
if [ "$confirm" != "yes" ]; then
    echo "Publish cancelled"
    exit 1
fi

cargo publish -p "$PACKAGE" --locked
echo "Published: https://crates.io/crates/$PACKAGE"
