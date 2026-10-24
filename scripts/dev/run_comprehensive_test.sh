#!/bin/bash
# 自动定位仓库根目录；实时行情须显式选择 live。
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."

case "${1:-offline}" in
    offline)
        RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --locked
        ;;
    quick)
        RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --lib --locked
        ;;
    live)
        unset RUSTDX_SKIP_INTEGRATION_TESTS
        RUSTDX_LIVE_TEST=1 cargo test -p jelven-rustdx-complete --test comprehensive_test --locked -- --nocapture --test-threads=1
        ;;
    report)
        report_dir="${RUSTDX_REPORT_DIR:-${TMPDIR:-/tmp}/rustdx-reports}"
        mkdir -p "$report_dir"
        report="$report_dir/comprehensive-test-$(date +%Y%m%d-%H%M%S)-$$.log"
        # pipefail保留cargo失败状态，报告包含真实完整输出。
        RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --locked 2>&1 | tee "$report"
        echo "测试日志：$report"
        ;;
    help|--help|-h)
        echo "用法：$0 [offline|quick|live|report]"
        ;;
    *)
        echo "未知选项：$1" >&2
        exit 1
        ;;
esac
