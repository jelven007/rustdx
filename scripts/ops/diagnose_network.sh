#!/usr/bin/env bash
# 验证本项目的通达信握手及心跳；可传入指定的 IP:端口。
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
exec cargo run -p jelven-rustdx-complete --locked --example test_connection -- "$@"
