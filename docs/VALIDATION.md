# 源码优化验证方法

更新日期：2026-10-22，优化源码截至1f9f739。本轮针对Rust1.89及同步架构，离线检查显式设置 `RUSTDX_SKIP_INTEGRATION_TESTS=1`。该变量跳过外部服务，不能据通过结果声称生产通达信、ClickHouse或MongoDB已验收。需求/开发和逐轮验收见[文档索引](README.md)。

```sh
cargo fmt --all -- --check
cargo build --workspace --all-targets --all-features --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked
cargo bench --bench indicators --locked
```

属性测试固定每项128个案例，覆盖随机二进制与极端偏移、布林带独立公式和KDJ价格缩放不变量；真实gbbq样本对照原解密器，并验证重复读取不会二次解密。网络预算使用独立测试进程和本地服务器；导入回归验证清单归档、SHA-256、行数/区间、迟到异常及子进程超时。

| 要求 | 检查位置/方法 | 不能由本地结果推导的事项 |
| --- | --- | --- |
| FR-01/02 日历与规则 | calendar/provider、limit/rules单测，查询及生效边界 | 2027完整休市表、逐证券历史状态及未来政策 |
| FR-03 标准口径 | data/model、builder单测 | 未适配来源/非股票源单位自动正确 |
| FR-04 网络治理 | `tests/network_budget.rs`独立进程、governor及本地协议回归 | 公共服务器可用性、跨进程预算及生产P95 |
| FR-05/06 文件与导入 | quality/sinks单测、CLI端到端、HTTP故障注入 | 真实数据库逐行正确或跨文件事务 |
| FR-07 安全解析 | checked入口、`tests/properties.rs`、真实gbbq夹具、fuzz | 所有可能报文已穷尽 |
| FR-08 名单/快照 | fetch_code/eastmoney本地回归及快照校验 | 当前公共接口限流或鉴权可用性 |
| FR-09/10 指标/工程 | 独立公式与不变量、bench、all-features/MSRV/Clippy | 第三方默认口径一致、旧版倍数加速 |
| FR-11 文档与交付 | Markdown链接/源码对照、Git合并与远端引用核验 | 源码推送等同crate发布或数据回退 |

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

2026-10-22最终结果：Rust1.89的workspace/all-features测试通过（156个核心库单测、4个CLI库单测、22个CLI命令单测、4例CLI端到端、4项属性/样本测试、52个文档测试，其他离线回归也通过）；Clippy all-targets/all-features拒绝警告及fmt/diff检查通过。本机nightly1.101.0、cargo-fuzz0.13.2、AddressSanitizer/覆盖率插桩执行544,705次，31秒，无崩溃。短时运行不代表穷尽所有报文。外部案例有些在skip变量下提前返回并被测试框架显示ok，该显示不计为在线验收。

GitHub CI配置包括Linux/macOS/Windows的构建/测试/Clippy/格式、Rust1.89测试、nightly fuzz及性能产物；本机结果不代表远端流水线已通过，远端状态应单独核验。

## 本轮文档检查

文档交付只修改Markdown，不重跑已验证且未改动的代码。核对所有本地Markdown目标、引用源码路径、四个优化提交的覆盖关系、清单状态/单位/时限及历史验收归属；执行 `git diff --check`。完整覆盖表见[docs/README.md](README.md)，两个回退版本与远端验证方式见[RELEASES.md](RELEASES.md)。
