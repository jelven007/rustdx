# 开发说明

<!-- lifecycle:registry -->
读者：开发者；责任：开发维护者。源码核验：2026-10-25；每90日及工具链、检查或构建入口变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

更新日期：2026-10-25。遵循根AGENTS.md及rust-engineer规则，Rust 2024 / MSRV 1.89。产品采用同步I/O、有界线程；改变并发模型须有明确需求。目录职责见[项目结构](../architecture/PROJECT_STRUCTURE.md)。

## 1. 构建和日常检查

以下命令从仓库根执行，使用已安装工具链；本机忽略的.cargo目录不是工程前置依赖。

```sh
cargo +1.89.0 build --workspace --all-targets --all-features --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --all-features --locked
cargo install --path apps/rustdx-cli --locked
cargo run -p jelven-rustdx-complete --example local_data
```

离线缓存齐全时加--offline；外部skip变量只控制测试，不控制业务启动。真实采样和数据库验收按[验证方法](../testing/VALIDATION.md)单独执行。Windows检查排除Unix runner，Linux/macOS检查完整工作区。

## 2. 源码定位

| 修改领域 | 路径 | 重点回归 |
| --- | --- | --- |
| 核心协议与网络 | crates/rustdx-core/src/tcp、pool.rs、operation.rs | network_budget、tcp_cancel、tcp_startup、daily_ip_pool、reliability |
| 行情接口与语义 | crates/rustdx-core/src/tcp/stock、client | eltdx_330_commands、protocol_decoders、finance_semantics、quote_precision、trade_models、transaction_pagination |
| 订阅与新命令 | 同上quote_subscription、recent_minutes、company_info_request、delisted_bars | quote_subscription、recent_minutes_fixtures、command_boundaries、extended_queries |
| 文件和计算 | crates/rustdx-core/src/file、data、calendar、limit、indicators.rs | file-day、fq、properties、规则/指标单测 |
| CLI采集与导入 | apps/rustdx-cli/src/cmd、io、quality.rs、paths.rs | cli、day_streaming、cli_deadline、database_acceptance |
| 单机版本发布 | apps/rustdx-runner/src | control_plane、wire_archive、tcp_startup |
| 外部接口 | tests/integration | opt-in网络与数据库用例 |

核心API路径保持rustdx_complete，应用path依赖指向crates/rustdx-core。不要把根虚拟Cargo.toml作为库依赖。新固定样本就近放各crate/tests，工程级原始证据放docs/testing/evidence。

## 3. 代码约束

新请求优先使用参数私有的checked类型，构包和解码分别验证。截断、错身份、超数量、非法日期/数值返回错误；未知尾部按该命令契约保留，不假设所有命令都拒绝尾字节。旧兼容入口的panic边界不能被推广到新接口。

证券身份保留market/code和前导零；字段单位、时间来源、价格口径与业务资格显式表达。财务value与candidate_value、TradeRecord与估算金额、requested_date与源日期分别处理。共同调用规则见[应用接入](APPLICATION_INTEGRATION.md)，签名与字段见[接口参考](../reference/COMMAND_API.md)。

NetworkPolicy在首次policy/stats/连接读取前配置；排队、预热、分页、重试、换站与空页复查消耗同一绝对deadline。同步计算和系统调用不能硬实时抢占；异步宿主使用有界阻塞工作线程。

所有权遵循结果类型：KlineData/IndexKlineData及其批量结果借用输入代码，跨线程/缓存持久化用拥有身份的模型。批量外层成功后继续逐项检查，分页集合检查complete/termination/atomic，订阅失效后显式重建。

输入摘要和解析使用同一私有字节快照；原子文件发布前核实行数/键/范围。数据库unknown不自动重试，acknowledged后全字段精确回读，不用浮点容差替代数据库验收。

## 4. 变更流程与验证选择

1. 确认受影响FR编号、调用方行为、兼容边界与恢复状态。
2. 在所属crate实现，复用现有预算/取消/留证，不增加旁路连接或无界队列。
3. 用独立来源或有意义的异常用例验证；固定真实样本保存来源/时间/参数/摘要。可逆低影响文档调整不增加重复实现的测试。
4. 执行fmt、Clippy和相关回归；结构或共享基础变更执行完整workspace检查。
5. 更新需求映射、方案、集成约定与受影响运营/运维文档；保存实际命令、退出码、跳过项和限制。

fuzz在tools/fuzz独立执行；基准显式指定核心包。复制冻结夹具后重解，避免改写已有原始证据。采样使用全新目录，不用“非空”证明完整性、日期或实际盘中推送。

```sh
cd tools/fuzz
cargo +nightly fuzz run checked_parsers --fuzz-dir . -- -max_total_time=30 -max_len=4096
cargo +nightly fuzz run modern_parsers --fuzz-dir . -- -max_total_time=30 -max_len=65536
```

modern_parsers首字节选择17种路径，覆盖044d目录、F10目录/内容、板块元数据、
退市bar、副图两种布局、小图、近期分时、竞价、资金流、快照、分类报价、
0547 XOR/游标解码、特殊涨跌幅、当日分时及外层解压。它只测试解码，不等同订阅网络状态序列fuzz；
原checked_parsers覆盖文件和旧/增强逐笔/K线/财务。独立锁文件也须保留。

指标基准schema1记录1万/10万/100万行、20/200/2000窗口及批量/增量
EMA/MACD五样本中位数，保留为观察。CI性能门禁另用
`scripts/bench/run_indicator_comparison.py --baseline-ref <commit> --output /tmp/new-comparison`：
固定Rust1.89、release参数、共同harness和依赖锁，归档实际f64输入；同机AB/BA
交替七轮比较共同批量API，记录CPU/RAM、工具链、输入/源码/二进制SHA。
schema2比较拒绝不同会话、环境和依赖、非法统计、输出摘要变化及默认1.5倍以上回退。
新增增量API不套不存在的旧基线；计时不证明RSS、市场资格或生产吞吐。

CI独立contracts任务执行核心行为和Python故障回归、2.0/旧签名独立调用方、
实际crate打包/安全解包及下游消费；包检查跳过重复workspace检查，只作为包门禁。
远端检查归档绑定GitHub SHA；本地配置与验证不代表Actions已运行。

runner的`pipeline/facts.rs`负责冻结事实校验和应有键遍历，`database/capacity.rs`
负责发送前容量/回读预留；CLI `io/factors.rs`统一前因子输入/数据库读取校验。
控制锁、账本状态与发布事务顺序继续在原编排入口集中表达。

## 5. 工程交付

发布包自带README和许可证。检查实际包清单及解包编译，避免exclude漏掉src/data等源码。核心包与CLI依赖版本同步升级后，按[发布流程](../release/RELEASES.md)先核心后CLI。

目录移动、删除和保全摘要见[迁移记录](../archive/release/PROJECT_RESTRUCTURE_20261024.md)。源码构建通过不等于注册表发布、远端CI或目标生产通过。

## 6. 任务与文档交付流程

每项任务建立独立分支和工作目录，先合入最新main以继承版本；开发、提交和功能分支推送不改号。记录受影响FR、源码边界、消费方和失败状态，在同一PR更新[追踪矩阵](../requirements/TRACEABILITY.md)及[文档登记](../../config/documentation/registry.json)。

示例从core/examples/{usage,capture,validation,diagnostics}查找，Cargo名称保持原值。新示例必须在Cargo.toml显式注册。生产DB/systemd在deploy/production，隔离实验在deploy/acceptance；env模板在config/runtime。目录/脚本路径改变同步迁移说明，禁止修改旧报告或摘要来让新检查通过。

执行`python3 scripts/validation/check_documentation.py`、`python3 scripts/validation/check_documentation_lifecycle.py`和`python3 scripts/release/versioning.py check`；共享结构变更再执行全workspace及真实包解包消费。PR检查完成后合入main，CI先同步版本并重新检查同一提交，再创建标签。发布包上传是独立步骤，源码版本号不证明crates.io存在同版本。

新增自有Cargo/前端元数据登记versions.json；新增关键文档登记读者、责任、权威输入、核验日期、周期和变更触发器。当前文档保存稳定文件名，完成专项结果归archive，原始输入/日志归testing/evidence。留存源码三个history引用不裁剪运行归档或备份。
