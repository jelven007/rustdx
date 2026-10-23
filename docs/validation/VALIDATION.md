# 源码优化验证方法

更新日期：2026-10-23，六步交叉验证见[NEXT_OPTIMIZATION.md](../history/NEXT_OPTIMIZATION.md)，第三轮见[OPTIMIZATION_ROUND3.md](../history/OPTIMIZATION_ROUND3.md)。本轮针对Rust1.89及同步架构，离线检查显式设置`RUSTDX_SKIP_INTEGRATION_TESTS=1`。该变量跳过外部服务；真实通达信、东财及隔离数据库验收单独执行和记录，不由离线ok推导。文档见[索引](../README.md)。

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
| FR-03 标准口径 | data/model、builder、EastCanonicalBar单测及东财/TDX同日比较 | 未核实品种/周期的源单位自动正确 |
| FR-04 网络治理 | network_budget/shared_network独立进程、HTTP/TCP取消及完整CLI时限回归 | 公共服务器长期可用性、跨主机出口及生产P95 |
| FR-05/06 文件与导入 | 确定性/因子跨块回归、实际RSS；quality/sinks故障注入、opt-in database_acceptance | 目标库其他历史/应有全市场数据、跨数据库事务及生产大规模负载 |
| FR-07 安全解析 | checked入口、kline_decoders独立pytdx样本、properties、真实gbbq夹具、fuzz | 所有可能报文已穷尽 |
| FR-08 名单/快照 | fetch_code/eastmoney本地回归及快照校验 | 当前公共接口限流或鉴权可用性 |
| FR-09/10 指标/工程 | 独立公式与不变量、bench、all-features/MSRV/Clippy | 第三方默认口径一致、旧版倍数加速 |
| FR-11 文档与交付 | Markdown链接/源码对照、Git合并与远端引用核验 | 源码推送等同crate发布或数据回退 |

模糊测试目标独立于发布workspace：

```sh
cargo +nightly install cargo-fuzz --locked
cargo +nightly fuzz run checked_parsers -- -max_total_time=30 -max_len=4096
```

目标覆盖day/lc/gbbq、报价、旧K线及新股票/指数独立解码（分钟与日线）的checked入口。CI使用nightly覆盖率插桩及AddressSanitizer。普通 `cargo run --manifest-path fuzz/Cargo.toml --bin checked_parsers -- -runs=1000` 只验证目标可运行，不等同于插桩测试。fuzz独立锁文件提交；语料及异常样本不提交，失败应保存最小复现并补回归。

## 2026-10-23交叉验证

Rust1.89 workspace/all-features、Clippy all-targets/all-features拒绝警告及格式检查通过；核心160单测、CLI库5单测、命令27单测，新增独立解码3项、分块端到端2项、54项普通文档用例及1项compile_fail，其他离线/属性回归通过。外部skip项不计作在线验收。新增解码路径ASan/覆盖率fuzz执行627,152次、31秒，无崩溃；临时日志`/tmp/rustdx-step6-fuzz.log`，短时运行不代表穷尽。

`tests/fixtures/kline`保存实际TDX响应字节、请求字节和独立pytdx期望CSV；股票日线、指数日线、股票5分钟线各3条。参考固定为pytdx提交2857fdad08534533610bd2aeca387f760c4baa42，原始参考源码摘要及每个样本摘要见[manifest](../../tests/fixtures/kline/manifest.json)。请求与pytdx struct.pack逐字节比较，日期/时间/OHLC/量额和指数涨跌家数精确比较；不只做新旧Rust实现互比。逐前缀截断、错解码器、超数量、多余字节、非法日期/时间及失败清空旧结果回归通过。生产无pytdx依赖。

复现独立期望（需要单独取得该固定提交及测试用six；脚本先核对参考源码摘要）：

```sh
python3 scripts/validation/verify_kline_pytdx.py --pytdx-source /path/to/pinned-pytdx --fixtures tests/fixtures/kline
cargo test --test kline_decoders
# 重新采集时使用新的目录，避免覆盖既有证据
cargo run --example protocol_capture -- /path/to/new-fixture-directory
```

2026日历以两交易所独立参考逐日核对，数据库对本次1040键精确回读，东财/TDX同日同股OHLC精确比较、量额采用声明的源编码容差；它们与数据库精确校验分别执行。实际RSS的版本二进制摘要、参数、数据和CSV字段摘要见[分块内存证据](evidence/day-memory-20261023.json)及[复现脚本](../../scripts/bench/measure_day_memory.py)。外部服务版本、字段及覆盖限制见[NEXT_OPTIMIZATION.md](../history/NEXT_OPTIMIZATION.md)。

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

10月22日文档交付只修改Markdown，该轮核对本地Markdown目标、源码路径、需求追踪、清单状态/单位/时限及历史验收归属，执行`git diff --check`。后续目录迁移另验证Rust路径、脚本、CI和干净检出；当前文档见[索引](../README.md)，两个回退版本见[RELEASES.md](../release/RELEASES.md)。

## 第三轮最终验收

Rust1.89整workspace all-targets/all-features、文档测试、Clippy拒绝警告与fmt通过：核心160单测、CLI库9单测、命令31单测及全部离线端到端/属性/示例目标，普通文档55项/compile_fail2项，18项网络文档示例保持ignored。最后ClickHouse查询配置修复后重新执行CLI全目标回归、workspace Clippy/fmt及release构建，均通过；日志`/tmp/rustdx-round3-final.log`与`/tmp/rustdx-round3-formatting-fix.log`。

真实官方ZIP的128个历史文件、779,200行，由独立Python struct解码逐行精确比较，1/4线程CSV一致；两库各两次全键/全字段回读、最终行数与重复去重通过。真实服务TCP断连/确认丢失/SIGKILL四项恢复通过；额外1,040键、6/8字段/复权与错误表/重复索引验收通过。最后东财两股在线CSV与保留夹具SHA相同；固定pytdx快照/财务/逐笔独立重解通过。ASan/覆盖率fuzz最终执行512,262次、31秒，无崩溃；日志`/tmp/rustdx-round3-final-external.log`。全部最终命令退出0，实际数据库日志`/tmp/rustdx-round3-databases-512.log`。

最终release二进制SHA-256=`50e7593dd9e9f25207bb1cf8c0f312532a65499c8f0dcb6d6f4267a82cdaad5d`，与大规模/故障JSON一致。精确指标、两次线程池阻塞失败及成功恢复、平台限制见[第三轮记录](../history/OPTIMIZATION_ROUND3.md)。固定Linux镜像配置及[数据库CI](DATABASE_ACCEPTANCE.md)已生成并做YAML/XML语法核对，尚未推送执行；本机实验ClickHouse不能替代稳定Linux/生产验收。

## 目录与需求整理验收（2026-10-23）

基线50c996b；本次整理文档/脚本/参考数据/夹具/SQL目录，更新路径与配置说明，重写需求2.0，保持Cargo包/API及同步有界架构。Rust改动为引用路径和不再依赖缺失assets的Excel示例；本次未重新运行真实网络、数据库或fuzz，既有证据保留原字节及轮次归属。

| 验证 | 结果与范围 |
| --- | --- |
| Rust1.89 workspace | fmt、Clippy all-targets/all-features `-D warnings`、test all-targets/all-features、workspace doc均退出0；构建/测试使用offline/locked，外部案例显式跳过 |
| 文档与配置 | 全部本地Markdown目标存在；FR-01至14唯一连续；Python AST、bash -n、5份YAML与ClickHouse XML语法通过 |
| 迁移内容 | day/gbbq二进制、SQL、两份参考日历和8份验收JSON共13文件SHA-256不变 |
| 独立重解 | 两个迁移后pytdx脚本先核对固定参考源码摘要；在临时副本重解，7组期望CSV逐字节相同，无新网络采样 |
| 干净检出 | 从Git暂存索引导出，仅版本化文件；核心file-day/fq/properties/kline_decoders/protocol_decoders、CLI calendar_datasets/cli/day_streaming及local_data示例通过 |
| 脚本控制流 | 用mock Cargo从外部工作目录调用dev/ops，根目录定位正确；report目录覆盖有效，退出17通过pipefail传播；不计作真实网络诊断 |
| 源码包 | 两crate `cargo package --list`通过；核心实际打包57文件并验证编译，解包all-features检查通过；CLI清单源码在临时精简workspace与该核心包共同编译通过 |

打包时发现裸`data`排除规则会漏掉`src/data/model.rs`，已改为仅排除根目录`/data`，重查清单与核心包编译通过；运行配置/数据/文档/部署/脚本/测试不进入核心包。四份原被`*.csv`忽略的protocol期望已按既有manifest摘要补入Git，干净检出回归确实读取这些文件。

完整联合离线打包额外尝试失败：本机crates.io缓存没有`jelven-rustdx-complete`索引，CLI上传包准备不能解析该注册表依赖。为核查目录没有漏掉源码，另按CLI包清单复制源文件，与实际解包的核心组成临时workspace，仅在临时目录补原workspace依赖并重建锁文件（移除了tests-integration成员），共有依赖版本不变后编译通过。仓库Cargo.lock未改；这不能写成CLI上传包或发布dry-run通过。正式发包仍须新版本/依赖及可用注册表环境。

检查日志为`/tmp/rustdx-layout-checks.log`、`/tmp/rustdx-layout-clean-checks.log`、`/tmp/rustdx-layout-package-checks.log`，状态/退出码另保存在同前缀JSON；这些本机日志不作为长期备份。目录映射与需求的持久结果见[项目结构](../architecture/PROJECT_STRUCTURE.md)、[需求主文档](../requirements/REQUIREMENTS.md)。
