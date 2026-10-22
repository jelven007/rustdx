# 本地优化验证方法

本轮针对Rust1.89及同步架构，全部离线检查设置 `RUSTDX_SKIP_INTEGRATION_TESTS=1`。该变量跳过外部服务，不能据通过结果声称生产通达信、ClickHouse或MongoDB已验收。

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo bench --bench indicators --locked
```

属性测试固定每项128个案例，覆盖随机二进制与极端偏移、布林带独立公式和KDJ价格缩放不变量；真实gbbq样本对照原解密器，并验证重复读取不会二次解密。网络预算使用独立测试进程和本地服务器；导入回归验证清单归档、SHA-256、行数/区间、迟到异常及子进程超时。

模糊测试目标独立于发布workspace：

```sh
cargo +nightly install cargo-fuzz --locked
cargo +nightly fuzz run checked_parsers -- -max_total_time=30 -max_len=4096
```

目标覆盖day/lc/gbbq、报价及K线的checked入口。CI使用nightly覆盖率插桩及AddressSanitizer。普通 `cargo run --manifest-path fuzz/Cargo.toml --bin checked_parsers -- -runs=1000` 只验证目标可运行，不等同于插桩测试。fuzz独立锁文件提交；语料及异常样本不提交，失败应保存最小复现并补回归。

本机优化版测量，输入100,000行、每组10次，Rust1.89 bench优化配置：

| 窗口 | SMA+RSI | 布林带+KDJ |
| --- | --- | --- |
| 20 | 41.79 ms | 41.21 ms |
| 200 | 31.64 ms | 43.61 ms |
| 2000 | 30.64 ms | 42.51 ms |

这是窗口扩展观测，没有宣称相对旧版的倍数加速。CI保存同一基准结果供趋势比较，不设置容易受共享runner噪声影响的硬毫秒门槛。布林带使用两栈中心化矩聚合（每条入栈/转栈/出栈各一次），KDJ使用单调队列；正常输入均O(n)，窗口状态O(period)，输出O(n)。SMA极端累加溢出可触发窗口重算。

2026-10-22最终结果：Rust1.89的workspace/all-features测试通过（156个核心库单测、22个CLI命令单测、4项属性/样本测试、52个文档测试，其他离线回归也通过）；Clippy all-targets/all-features拒绝警告及fmt/diff检查通过。本机nightly1.101.0、cargo-fuzz0.13.2、AddressSanitizer/覆盖率插桩执行544,705次，31秒，无崩溃。短时运行不代表穷尽所有报文。GitHub跨平台流水线已更新，本轮尚未在远端执行。
