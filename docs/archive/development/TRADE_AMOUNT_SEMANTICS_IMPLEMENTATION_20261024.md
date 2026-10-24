# 成交明细金额语义修复实施记录

日期：2026-10-24。Rust 2024 / MSRV 1.89。对应[实施方案](TRADE_AMOUNT_SEMANTICS_IMPLEMENTATION_20261024.md)。金额合同版本为 `TRADE_AMOUNT_SCHEMA_VERSION = 1`，独立于成交事件分类版本。

## 已实现的接口与行为

新增 `trade_amount.rs`、`reported_turnover.rs`、`trade_amount_reconcile.rs` 和 `client/turnover.rs`，均从 `tcp::stock` 导出。网络查询复用同步 TCP、操作总时限、取消、请求节奏和 FrameObserver，没有引入异步运行时。

| 使用场景 | 接口 | 结果与限制 |
| --- | --- | --- |
| 明细页估算 | `TradePage::amount_estimates(&context)` | 与原始记录逐条对应，保留页偏移、记录位置、资格与不可用原因 |
| 增强兼容页估算 | `EnhancedHistoryTransactionPage::amount_estimates(&context)` | 重新解码 payload，检查公开 DTO 数值、索引、时间、状态与参考槽；不从浮点价格反推整数 |
| 集合汇总 | `TradeCollection::amount_estimate_summary(&context, options)` | 常规含撮合、盘后、显式启用的候选分别汇总；覆盖与金额来源独立 |
| 兼容集合汇总 | `TransactionCollection::amount_estimate_summary(&context, options)` | 使用不可变 `event_pages`，校验身份、页偏移、来源、兼容记录和记录范围；支持分页预算缩小后的页大小 |
| 源页解码 | `decode_reported_turnover(&request, payload, source)` | 保留原始量额编码、字节范围、wire index、完整 payload 与来源；本身不确认业务时段 |
| 已取源页组装 | `ReportedTurnover::from_pages(query, pages, profile)` | 校验分页方向、来源、日期和时间，筛选目标日期；推导可用性与采样状态 |
| 显式网络查询 | `Client::reported_turnover(market, code, date, scope, options)` | 按日期有界翻页，执行页数/条数/字节/总时限预算；失败返回错误 |
| 数量边界与差额 | `reconcile_trade_amounts(&trades, &context, &minute, daily)` | 按页倒序、页内原序恢复时间顺序，以精确累计股数及时间约束找唯一边界 |

`TradeAmountContext::a_share(query, unit, evidence)` 要求调用者明确确认 A 股源单位。代码范围只是受支持品种限制，不能自动证明原始数量单位。`LotsOf100Shares` 和 `Shares` 为显式选择；`TradeAmountContext::unknown` 保留未知单位，不能输出默认金额。

明细估算使用原始整数价格（分）乘受检股数，按 `i128` 分存储；JSON 中 `estimated_amount_fen` 为十进制字符串。候选可以取得带候选资格的逐条估算，但默认不纳入已支持成交汇总；显式启用后仍独立放在候选组件。快照、未知事件、单位缺失、空集合和只有候选的集合都不会变成完整时段的零金额。明确成交的零源数量可得到零估算。

汇总保存请求身份、日期依据、请求顺序的页来源以及现有 `TradeCoverage`。历史成交日期依据仍是 `RequestedDate`，没有把 0fc6 请求日期提升为响应日期回显。当日成交的本地日期观察标记为 `ObservedDateUnverified`。

## 源成交额与精度

`TurnoverProfile::a_share(market, code, date, endpoint, evidence)` 是调用者提供的特定证券、日期和端点证据：常规时段包含开收盘撮合、排除盘后，并确认周期量额单位。它不默认应用于其他证券、日期或服务器。没有匹配 profile 时保留原始源页，业务合计为不可用。

默认常规日线使用 category 9；可显式选 category 4。一分钟为 8，五分钟为 0。源金额按元保存，不乘股/手转换倍数；对账数量在该 profile 下，category 0/8/9 为股，category 4 为 100 股/手。`AfterHours` 没有已验证的独立源接口，直接返回不可用；`Combined` 可以取得常规组件，其组合源总额保持不可用。

量额 leaf 使用受检 `f32::from_bits` 再转 `f64`，拒绝 NaN、Inf、负值。原始 K 线共享解析核心保留量额 u32 和字节范围。旧 `KlineData.amount`、`IndexKlineData.amount` 同步使用该金额 leaf：零码由旧 helper 的异常微小值纠正为零，低值分支纠正为真实 binary32 值；正数实帧金额保持不变。旧 `vol_amount` 的其他数量用途未泛化修改，新源金额接口的数量直接解码原始 binary32。

源合计使用 Neumaier 补偿求和，保留 `.875` 元等源精度，不先舍入成分。每根金额的编码步长和汇总方法保留在结果中。对账的浮点元差额是诊断/展示值，明细估算的权威存储仍为整数分；补偿求和与编码步长不能消除供应商统计误差。

目标日期必须与 wire bar date 对齐。取得目标日完整的 240/48 个受支持分钟标签或一根日线、且来源采样在该日 15:01 起或后续日期，才标记该源时段可用；盘中或来源采样时间缺失为部分取得。这个状态表示供应商时段取得，不是交易所认证或源保留范围的证明。

## 对账与冻结样本验收

夹具直接引用既有[捕获目录](../../testing/evidence/enhanced-history-amount-20261024)，未重新生成期望。测试检查请求、响应帧、解压结果与保留 payload；另核对冻结清单全部 66 个文件的 SHA256。期望来自已保存的独立 Python 归因结果。

| `sz300308 / 20261023` | 验收值 |
| --- | --- |
| 原始记录 | 4876 条；87 条快照；4744 条支持常规事件；45 条支持盘后事件 |
| 常规数量与估算 | 478818 手 / 47881800 股；27918010839 元 |
| 盘后数量与估算 | 181 手 / 18100 股；11088603 元，独立组件 |
| 常规日线源额（9、4） | 27918479360 元 |
| 一分钟源合计 | 27918479958.875 元；240 个精确数量边界 |
| 五分钟源合计 | 27918479888 元；48 个精确数量边界 |
| 估算−一分钟 | −469119.875 元 |
| 一分钟−日线 | 598.875 元 |
| 估算−日线 | −468521 元，持续可见 |
| 收盘原始事件 | 4094 手、612.63 元；保持 409400 股，没有写回 409445 股推断候选 |
| legacy 0fb5 | 所有共同 raw 字段保持相同；4789 个非快照事件仍是候选 |

离线没有 `TradeSource` 的明细仍可计算数量边界，`full_scope_available` 保持 false，来源状态为 `MissingOrArchived`。同连接实帧回放可取得 `SourceEnumerated` 和完整时段对账；跨连接对账返回错误。对齐结果 `Exact` 仅描述数量前缀，不把估算提升为实际成交额。

不匹配、重复零量前缀造成的多候选、部分覆盖、未知事件、错日期/周期、不同来源、源量的非整数股数都被测试。对齐不会拆记录、去重、补股或分配分钟金额。

## 调用与迁移

新调用方式见 [transaction_amount 示例](../../../crates/rustdx-core/examples/transaction_amount.rs)。调用者传入独立单位/时段证据后，示例展示兼容集合估算、分钟与日线源额、组合源额不可用原因和对账结果。

```rust
use rustdx_complete::tcp::stock::*;

let query = TradeQuery::history(0, "300308", 20261023)?;
let context = TradeAmountContext::a_share(
    query, TradeQuantityUnit::LotsOf100Shares, "independent unit evidence",
)?;
let estimate = trades.amount_estimate_summary(&context, TradeEstimateOptions::default())?;
// Integer fen is serialized as a string; candidates remain separate.
let estimated_fen = estimate.regular_with_auctions().estimated_amount_fen();
```

旧 `TransactionData`、增强 DTO 和现有分页接口没有新增必填字段。外部历史价格×数量字段应迁移命名为 `estimated_amount_*`，并保存合同版本、单位、资格、时段和覆盖。独立源金额保存为 `reported_amount_*`，同时保存原始 category、wire 日期、采样时点和原始编码；展示时可格式化两位小数，存储不舍入。

源不可用是 `None + 原因`，不能回退为估算或零。原有乘积数据没有源补采时，不能批量重命名为真实成交额。

## 工程验证

验证使用仓库私有 `.cargo` 中的 Rust 1.89.0，`--offline --locked`。实际命令、结果与日志索引记录在[机器证据](../../testing/evidence/trade-amount-implementation-20261024.json)。全工作区测试显式设置 `RUSTDX_SKIP_INTEGRATION_TESTS=1`，排除现网行情/官网集成依赖；新网络场景由本地同步 TCP mock 与冻结实帧回放覆盖。

| 检查 | 实际结果 |
| --- | --- |
| `cargo +1.89.0 fmt --all -- --check` | 通过 |
| `cargo +1.89.0 clippy --offline --workspace --all-targets --all-features --locked -- -D warnings` | 通过，零警告；包含新增示例 |
| 金额三组测试及既有五组相关测试 | 52 项通过；新金额集成用例 15 项 |
| `RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --offline --workspace --all-features --locked` | 54 组结果汇总：455 passed / 0 failed / 24 ignored；含新增单元测试及 doctest |
| 冻结输入摘要与改动空白检查 | 66 个捕获文件 SHA256 匹配；`git diff --check` 通过 |

455 是测试框架的通过计数，包含通过上述环境开关直接返回的现网集成用例；不能将其解读为执行了现网查询。执行日志保存在[验证日志目录](../../testing/evidence/trade-amount-implementation-20261024)。

第一版仍不提供每条聚合记录的实际金额、交易所逐笔金额、独立盘后源额或未验证品种金额。这些限制均由独立类型、资格或可用性表达。
