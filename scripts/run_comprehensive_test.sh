#!/bin/bash
# 从仓库根目录运行；实时行情须显式选择 live。
set -euo pipefail

if [ ! -f Cargo.toml ]; then
    echo "请在仓库根目录运行" >&2
    exit 1
fi

case "${1:-offline}" in
    offline)
        RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --locked
        ;;
    quick)
        RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --lib --locked
        ;;
    live)
        unset RUSTDX_SKIP_INTEGRATION_TESTS
        RUSTDX_LIVE_TEST=1 cargo test --test comprehensive_test --locked -- --nocapture --test-threads=1
        ;;
    report)
        mkdir -p reports
        report="reports/comprehensive-test-$(date +%Y%m%d-%H%M%S)-$$.log"
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
