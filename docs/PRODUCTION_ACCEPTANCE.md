# 生产验收报告

文档更新：2026-10-22。下文按阶段保留证据，最新源码离线验证见末节；10月22日优化尚未做新一轮真实服务验收。首次在线验收日期：2026-10-10。环境：macOS 26.5.1 arm64、Rust 1.99.0、Colima、ClickHouse 25.8.33.6。代码基线为 `773c8a5`，并包含当次在线验收修复。

## 2026-10-10在线验收结论

当时版本的通达信行情、官网日线包、CSV、累计收益因子及 ClickHouse 主链路通过本机部署验收。东财公共接口存在出口/节点限流，不能作为唯一生产增量来源；MongoDB 未验收。最新源码上线前按[运维手册](RUNBOOK.md)在目标环境复核，不能用本节历史结果替代本轮验收。

## 在线行情

`cargo run --release --example production_smoke` 实测：

| 能力 | 结果 |
| --- | --- |
| 协议握手/心跳 | 通过；深市证券数24401 |
| 实时行情 | 2只 |
| 股票/指数日K | 各23根 |
| 前复权日K | 183根 |
| 当日/历史分时 | 各240点 |
| 当日/历史逐笔 | 各20笔 |
| 财务 | 600519，更新日2026-08-15 |
| 除权事件 | 45条 |
| 沪深证券列表 | 24401 / 27578 |
| 概念板块 | 41332条 |
| F10目录/正文 | 各16项 |

真实 workspace 测试串行通过，包括16台协议可用服务器、K线分页/批量一致性、5分钟窗口、指数、复权、分时多服务器、交易所名单和全部离线回归。2026-10-10为非交易日，“今日历史分时”对照未就绪，但上一交易日历史分时和当前分时均返回240点。

验收发现旧的第二、第三登录包会使当前服务器拒绝K线并返回错误码 `0x0320`。连接已改为单个标准握手包，修复后真实K线契约测试6/6通过。

## 全量文件

官网 `hsjday.zip`：

- 下载 526.2MiB，ETag `"6ac89ebd-20e2ae48"`，Last-Modified `2026-10-09 07:58:53 GMT`。
- ZIP校验及12451个文件解压成功；二次运行命中下载和解压缓存。
- 沪深京全量解析：12451文件、29816871源记录。
- 输出前按代码内日期升序规范化，重复日期保留源文件最后一条；最终29816859行。
- CSV 约1.8GiB，覆盖1990-12-19至2026-10-09，解析约26秒。
- 质量报告0错误；24835条警告主要为日历覆盖外、非A股历史源OHLC异常、2个空文件。

A股继续严格校验OHLC、非负成交数据和日期顺序。指数、基金等非A股历史源异常保留原值并告警；`--strict` 会拒绝任何警告。

## ClickHouse

本机服务由 `deploy/clickhouse/compose.yml` 定义，仅绑定127.0.0.1。

- 引擎：`ReplacingMergeTree(version)`，逻辑键 `(date, code)`。
- 全量写入：29816859行、12449个非空代码、约826.8MiB磁盘。
- `count()` 与 `count() FINAL` 都为29816859，写入前已无重复逻辑键。
- 重复导入样本：物理14452行、`FINAL` 7226行；合并后物理/逻辑均7226行。
- 全量因子：7226行；从ClickHouse读取前因子后增量接续成功。
- HTTP认证、DDL、CSV流式上传、边界查询均通过。
- 删除手工容器但保留数据卷后，使用仓库Compose定义重建成功；健康检查通过，全部验收表及数据完整保留。
- 集成回归会把同一CSV写入两次，断言`FINAL`和`OPTIMIZE ... FINAL`后的物理行数均为2。

查询对实时去重有要求时仍使用 `FINAL`。生产密码只写部署环境，不提交 `.env`。

## 东财

单页真实JSON已成功：`sz000001`，行情日2026-10-09，OHLC/成交额/成交量完整。系统DNS曾选到拒绝请求的CDN节点，已增加TLS域名校验下的备用IP和 `RUSTDX_EASTMONEY_IPS` 覆盖。

重复全市场验收触发502及连接中断。冷却后的最终单证券复测仍在约4秒内收到3个502和2个TLS提前断开；15秒请求总超时及多节点失败传播正常，说明公共接口有明显限流。生产要求：

1. 仅在收盘后运行一个 `east` 任务，不并发扫描。
2. 失败不得覆盖上次成功文件或数据库数据。
3. 监控质量报告和退出码；失败时用下一次官网日线包或通达信K线补数。
4. 不把东财作为唯一数据源或可用性SLA来源。

## 上线约束

1. 目标机器运行 `production_smoke`，并执行一次小表写入/重复导入验证。
2. 首次部署直接创建新版表，不执行旧表迁移。
3. 日线正式任务必须保存 `--report`，检查状态、边界日期和错误数。
4. 内置精确休市表仅覆盖2024–2026；进入2027年前升级日历数据，否则严格连续性检查会拒绝范围外日期。
5. MongoDB和目标生产操作系统尚未做本轮真实验收；使用前独立验证。
6. 公共行情接口没有官方SLA，生产系统必须监控空帧率、失败率、数据日期及行数。

## 源码复查追加验收（1.12.1 / CLI1.8.1）

2026-10-10，复查基线00df6f4。本轮未重新执行2981万行全量导入，上述全量数据仍属1.12.0时期验收；新增行为在同一macOS/ClickHouse环境验证。

| 验证项 | 结果 |
| --- | --- |
| 离线workspace完整测试及文档测试 | 通过；核心140单测、CLI库3单测、CLI二进制17单测、CLI端到端3例、核心52文档测试；外部案例按变量跳过 |
| 批量池并发 | 本地协议服务器验证同Client重叠批次排队，1/2并行数变化均保留所有结果 |
| 空采集/坏文件 | 返回失败、保留旧CSV、报告failed及错误记录 |
| 入库失败端到端 | 模拟HTTP入库异常，CLI非零退出、中间CSV保留、报告failed；源文件仍记已解析行数 |
| ClickHouse响应 | HTTP500、200正文异常、异常header及截断正文均失败，保留URL已有参数 |
| ClickHouse真实小表 | 认证、DDL、完整元数据、流式写入及重复去重通过，FINAL/合并后均2行；错误排序键被拒绝 |
| 东财分页 | 0参数、重复证券及total变化拒绝，上限超过实际total正常结束；未重新请求限流中的公共接口 |
| Clippy / 格式 / Rust1.89 | workspace/all-targets检查通过，Clippy -D warnings |
| 两个源码包联合打包 | 离线临时注册表解包编译通过，CLI包包含LICENSE-MIT |
| crates.io 发布 | 先发 `jelven-rustdx-complete 1.12.1`，索引可见后发 `jelven-rustdx-cli 1.8.1`；CLI 验证步骤从注册表拉取核心库编译通过；发布用的受限 token 发布后已吊销 |
| 测试脚本/文档 | report模式真实保存完整日志并通过；shell语法及维护文档本地链接检查通过 |

复现命令：

```sh
bash scripts/run_comprehensive_test.sh report
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo +1.89.0 check --workspace --all-targets --locked
cargo package -p jelven-rustdx-complete -p jelven-rustdx-cli --offline
# 先加载部署目录的CLICKHOUSE_*环境配置，移除外部测试跳过变量
cargo test -p jelven-rustdx-cli --bin rustdx io::test_insert_clickhouse --locked -- --nocapture
```

发布命令（受限 token，publish-new + publish-update，仅限这两个 crate）：

```sh
cargo publish -p jelven-rustdx-complete --locked
cargo publish -p jelven-rustdx-cli --locked
```

CLI 依赖同时写注册表版本与 path，单独发布依赖已发布的核心库 1.12.1。本轮保留原MIT版权声明和技术协议对照，删除与当前维护无关的旧归档及错误概念样例。当时列出的历史规则和进程连接预算/时限已在10月22日源码实现；2027完整日历数据、当前源码在线复核及MongoDB验收仍待完成，见[架构评估](PROJECT_ANALYSIS.md)。

## 追加源码复查（2026-10-11，核心库1.12.2 / CLI1.8.2）

复查基线6065885、修复提交2fbbda6、发布提交9dfc024，macOS/aarch64。本轮使用本地文件和TCP/HTTP模拟服务，未重新执行外部行情、2981万行全量导入或MongoDB验证。本轮版本核心库1.12.2 / CLI1.8.2已发布到crates.io。

| 验证项 | 结果 |
| --- | --- |
| 离线workspace | 通过；核心143单测、CLI库3单测、CLI二进制18单测、CLI端到端4例、可靠性6例及既有回归 |
| 文档测试 | 核心51通过、18忽略；移除了一例依赖错误行业映射的旧断言 |
| 复权及涨跌停 | 首日锚点、停牌连续派息、非法事件、半分向上舍入、主板代码范围通过 |
| 证券完整性与恢复 | 空页、重复、超总量失败；完整分页成功；短心跳清空旧结果；请求和重连握手相继失败后按预算恢复 |
| 指标 | EMA/MACD缺口位置与恢复、非法参数、SMA溢出恢复、KDJ窗口收盘越界用例通过 |
| 输出/输入保护 | 相对与绝对路径、`..`、符号链接、重复写锁、day/east报告及保留文件冲突用例通过；原输入内容保持 |
| 缓存/前因子 | 来源对象存活期间缓存拒绝再次获取，释放后可用；空/重复/非法因子拒绝；原HTTP续传及ZIP失败保护回归通过 |
| macOS端到端稳定性 | 修正本地测试服务accept后继承非阻塞状态，明确切换为阻塞读取 |
| 构建检查 | Clippy `-D warnings`、格式和Rust1.89 workspace/all-targets通过 |
| 文档/脚本 | 本地Markdown链接及shell语法通过；在仓库外目录调用诊断脚本，本地模拟心跳123/0分别退出0/1 |
| 发布门禁 | 版本更新后重跑离线workspace、格式、Clippy及核心库dry-run通过；两个源码包联合打包、解包编译通过 |
| crates.io发布 | 核心库1.12.2先发布并确认索引可见，再发布CLI1.8.2；CLI从注册表下载核心库编译通过 |
| 注册表核验 | 两个版本均可用且未撤回，下载包SHA-256与注册表一致，包内Git提交为9dfc024且无脏标记；CLI依赖为`^1.12.2`并包含LICENSE-MIT |
| 发布凭据 | publish-new + publish-update仅限这两个crate；发布后已吊销，本地临时凭据已删除 |

指标release基准：10万行SMA+RSI，10轮，周期20/200/2000分别约39.8/33.6/33.4ms。增加补偿求和后仍保持常规窗口O(n)整体计算；这里只记录本机合成数据耗时，不作为生产延迟或提升比例承诺。

复现命令：

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo +1.89.0 check --workspace --all-targets --locked
cargo bench --bench indicators
bash -n scripts/diagnose_network.sh scripts/run_comprehensive_test.sh scripts/publish.sh
```

外部测试使用环境变量跳过，不计为本轮在线验收。当前Rust仍提示依赖proc-macro-error2的未来兼容性警告，现有检查通过；后续升级依赖时复核。行为变化见[迁移说明](MIGRATION.md)。

## 源码优化离线验证（2026-10-22）

基线904b33e，源码提交7cd02e7、5fb5b70、401265c、1f9f739；macOS/aarch64、Rust1.89。全部离线检查设置 `RUSTDX_SKIP_INTEGRATION_TESTS=1`，本地TCP/HTTP模拟仍执行，跳过的外部案例不计为在线通过。Cargo版本保持1.12.2/1.8.2，本轮优化尚未发新包。

| 项目 | 本轮证据 |
| --- | --- |
| workspace/all-features | 核心156单测、CLI库4单测、CLI命令22单测、CLI端到端4例、属性/样本4项、核心52文档测试及其他离线回归通过 |
| 严格日历与规则 | 外部休市表、搜索越界、规则生效日期/上市阶段、无幅度限制与非法输入区分通过 |
| 标准行情与名单 | 市场/来源/周期/价格口径、量单位和非法数值回归；名单有界读取故障回归通过 |
| 进程网络治理 | 独立进程验证主连接/池共享配额、重连峰值、批次释放、握手/慢正文总体时限；轮换/冷却回归通过 |
| 导入与恢复 | 每次清单归档、同快照摘要、CSV回读/行数/区间、输入重叠拒绝、迟到HTTP异常、子进程非零及超时通过 |
| 二进制解析 | 随机字节/极端偏移、真实gbbq与原解密器对照、重复读取不二次解密通过 |
| Clippy / 格式 | all-targets/all-features -D warnings、fmt和差异检查通过 |
| 模糊测试 | nightly1.101.0、cargo-fuzz0.13.2，ASan/覆盖率544,705次、31秒，无崩溃 |
| 性能 | 100,000行、10轮，布林带+KDJ窗口20/200/2000为41.21/43.61/42.51ms；不宣称相对旧版加速倍数 |

本轮未重新验收通达信/交易所/东财实时可用性、全量ClickHouse或MongoDB；跨平台CI配置已扩展，但本机检查只证明macOS结果，远端状态独立核验。真实2981万行导入仍属于10月10日证据。

当前上线前还需目标机smoke、隔离小表写入/重复去重、目标逻辑键及数值核对、数据库故障恢复演练。清单unknown不证明无写入，acknowledged不证明逐行正确；恢复见[IMPORTS.md](IMPORTS.md)和[RUNBOOK.md](RUNBOOK.md)。日历只有2024–2026，规则核实截至2026-10-22，2027/未来政策需有来源续期。

完整复现命令和CI门禁见[VALIDATION.md](VALIDATION.md)，源码合并与两个历史快照见[RELEASES.md](RELEASES.md)。
