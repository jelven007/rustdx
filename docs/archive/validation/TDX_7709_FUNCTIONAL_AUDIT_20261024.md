# 通达信 7709 接口功能与约束核对

核对日期：2026-10-24。对象为 rustdx 当前工作区源码，HEAD 为 `408e30e`，包含尚未提交的修改。参考清单为用户提供的 21 项能力以及仓库保存的 eltdx 3.3.0 源码、命令表和方法文档。

## 1. 核对范围与判断口径

核对覆盖公开 API、实际请求编码、响应解码、批量与分页封装、参数校验和已有验证证据。报告中的“部分实现”表示相应基础业务已有代码，但尚未完整满足参考清单的参数、字段或调用语义；“未实现”表示当前核心库没有对应请求及业务入口。Python SDK 的名称不要求在 Rust 中逐字复制。

按命令号统计，21 项中有 9 项存在同码实现，4 项通过其他命令实现相近业务，8 项没有对应主动 TCP 能力。这不是完整功能覆盖率。同码接口仍存在单只与批量、旧字段与新字段、复权方式和分页语义的差异。按本报告的功能口径，自动握手基本满足目标，12 项属于基础或部分实现，8 项未接入。[1](#source-1)[2](#source-2)[3](#source-3)

| 对照口径 | 数量 | 命令 |
| --- | ---: | --- |
| 同码已有实现 | 9 | `000d`、`000f`、`0010`、`044e`、`052d`、`053e`、`06b9`、`0fb4`、`0fc5` |
| 其他命令承担相近业务 | 4 | 心跳 `044e`、目录 `0450`、当日分时 `051d`、历史逐笔 `0fb5` |
| 没有对应主动 TCP 能力 | 8 | `0452`、`051b`、`052b`、`0547`、`054b`、`054c`、`056a`、`0fd1` |

源码是判断当前实现的直接证据。eltdx 源码和文档证明其 SDK 的实现及约束，不构成通达信官方完整协议规范；相应服务端上限、状态枚举和覆盖范围没有在本轮重新进行线上独立验证。[3](#source-3)

## 2. 21 项功能逐项结果

| # | 参考命令与能力 | 当前入口或实现 | 功能判断 | 尚缺能力或限制 |
| --- | --- | --- | --- | --- |
| 1 | `0x0004` 心跳保活 | `Client::heartbeat` / `Tcp::heartbeat`，实际发送 `0x044e` | 部分实现，基础保活可调用 | 没有 `0x0004`；连接池是在借出空闲连接时探活，没有后台定时心跳。长时间持有 Client 的调用者需自行周期调用。 |
| 2 | `0x000d` 连接握手 | `Client::new/with_config` → `Tcp` 建连 → `PACK1` | 已实现自动初始化 | 重连也会重新握手。未提供单独的 `session.handshake()` 或结构化握手结果；不影响基础自动初始化能力。 |
| 3 | `0x000f` 股本变迁 | `Client::xdxr(market, code)` / `Xdxr` | 部分实现 | 只构造单只证券请求；没有 75 只默认分批、1–200 可调批量或 `CapitalChangeBatch`。类别 15 无专门语义处理；本地复权只消费类别 1。 |
| 4 | `0x0010` 财务批量 | `Client::finance` / `FinanceRequest` | 部分实现 | 只查询单只；解码数量仅允许 0 或 1。DTO 有资产、股本、利润等字段，但未公开 EPS；对应位置在本库和 eltdx 中的命名存在差异，业务含义待独立验证。 |
| 5 | `0x044d` 代码表 | `Client::stocks` / `stock::SecurityList`，实际发送 `0x0450` | 部分实现，旧目录可查询 | 旧请求与 29 字节记录，默认按 1000 条聚合；没有新版 1600 条可调分页、37 字节记录及返回 DTO 中的 category/board。普通 TCP 不支持北交所；没有沪深北全 A 股列表便捷入口。 |
| 6 | `0x044e` 市场代码数量 | `Client::stock_count(market)` | 部分实现，沪深总数可查 | 只接受市场 0/1。结果为证券总数，不是 A 股数；没有 `a_share_count` 便捷方法。已有 `Symbol::is_a_share`，可供调用者本地过滤，但没有完整沪深北目录支撑。 |
| 7 | `0x0452` 特殊涨跌停表 | 没有对应请求和查询 API | 未实现 | 本地 `limit` 模块计算普通证券涨跌停，不提供服务端特殊表扫描、start_index 分页或索引构建。 |
| 8 | `0x051b` 分时副图 | 没有对应请求和查询 API | 未实现 | 没有副图选择器、买卖力道或成交对比双序列。 |
| 9 | `0x052b` 退市日线 | 没有对应请求和查询 API | 未实现 | 本地 `.day` 解析或普通 K 线查询不能算作该接口实现；没有 T 前缀规范化和退市接口自动翻页。 |
| 10 | `0x052d` K 线 / 周期线 | `bars/index_bars`、`bars_range/index_bars_range`、`k`、`k_batch/bars_batch`、`k_adjusted` | 部分实现，常见周期与区间/批量已具备 | 有 800 根上限、周期映射和本地日线前/后复权；没有服务端复权模式、定点锚点或 bar_time 开关。旧 38 字节请求、u16 偏移；短页就停，不等同于参考接口翻到空页的语义。 |
| 11 | `0x0537` 当日分时 | `Client::minute`，首选 `0x051d`，必要时回退 `0x0fb4` | 部分实现，旧分时可查询 | DTO 只有 price/vol；没有时间标签、均价、批量查询或 include_raw 高层选项。未接入 `0x0537` 布局。 |
| 12 | `0x053e` 旧版批量行情 | `Client::quotes` / `QuotesRequest` | 部分实现，五档快照已具备 | 显式 market/code 列表，单次最多 80；五档价量已返回。没有 trading_status_raw 和原始尾部字段，报价按固定比例还原，未接入目录驱动的多小数品种规则。 |
| 13 | `0x0547` 增量 / 推送 | 没有对应请求、游标或推送队列 | 未实现 | 没有 refresh、poll_push、首帧基线及增量合并；也没有“最多 100 个代码”的该命令校验。重复调用旧快照不会获得这一能力。 |
| 14 | `0x054b` 分类行情榜单 | 没有对应请求和查询 API | 未实现 | 没有服务端 category、sort_by、ascending 排序和排行自动分页。调用者本地排序不等价于完整服务端榜单。 |
| 15 | `0x054c` 基础行情快照 | 没有对应请求和查询 API | 未实现 | 当前快照走 `0x053e`；尚未实现新版一档布局。 |
| 16 | `0x056a` 集合竞价过程 | 没有对应请求和查询 API | 未实现 | 没有当日/历史秒级过程、虚拟匹配量及未匹配量。该接口的 09:25 过程快照仍是虚拟撮合；正式开盘撮合应从 `0x0fc5/0x0fc6` 成交明细中取 09:25 且 `status_raw != 8` 的记录，`status_raw == 8` 归为竞价快照。[7](#source-7) |
| 17 | `0x06b9` 服务器文件 | `BlockInfoChunk` / `Client::block` | 部分实现，板块文件分块下载已具备 | 高层固定 30000 字节块并校验完整性；低层旧格式仅 100 字节文件名，超长会截断。没有通用 resources.read、300 字节路径格式、read_stats 或 tdxstat 统计解析封装。 |
| 18 | `0x0fb4` 指定日期历史分时 | `Client::history_minute(market, code, date)` | 部分实现 | 单只、u32 日期，返回 price/vol；没有日期字符串或日期类型便捷输入、批量、时间/均价字段。该入口未校验日期真实性。可用历史深度仍由服务器决定。 |
| 19 | `0x0fc5` 当日逐笔 | `Client::transaction` / `TransactionRequest` | 部分实现 | 仅单只单页；上限校验为 2000，与参考 1800 不一致。没有自动全量翻页、批量、TradePage 及成交/竞价/正式撮合/盘后分类。 |
| 20 | `0x0fc6` 历史逐笔增强 | `Client::history_transaction`，实际发送旧 `0x0fb5` | 部分实现，旧历史逐笔可查 | 日期校验已有，但没有 `0x0fc6` 增强布局、自动翻页、事件分类和批量。旧 num 补为 0，不是新接口笔数字段。 |
| 21 | `0x0fd1` 小走势图 | 没有对应请求和查询 API | 未实现 | 没有 selector、window、基准价或轻量价格序列入口。 |

逐项依据：Client 实际公开方法和单只/批量调用路径，[1](#source-1)；请求构造及响应数量校验，[2](#source-2)；真实请求头及各 DTO，[4](#source-4)；连接与连接池行为，[5](#source-5)；参考 SDK 的命令、方法与限制，[3](#source-3)。

## 3. 参数与分页约束

下表中的“参数探测”运行当前编译库的公开构造器，不建立行情连接，不声称服务端会接受这些请求。探测只判断客户端是否拒绝或怎样编码参数。

| 约束 | 当前结果 | 证据及影响 |
| --- | --- | --- |
| K 线每页 1–800 | 已执行校验 | 参数探测：0 拒绝、800 接受、801 拒绝。解码器也拒绝返回数量超过请求和多余尾部。 |
| K 线完整分页到空页 | 与参考语义不同 | `pagination::collect` 遇 count < 800 就结束，满页偏移加 800；最多 82 页。若旧服务端短页后仍有更早数据，会提前结束。本轮未验证旧服务端短页是否一定是终点。 |
| K 线分页升序、去重和区间过滤 | 已实现 | 汇总后按时间排序、去重、闭区间过滤；历史页必须继续向更早时间推进。相应现有回归通过。 |
| 成交明细每页最多 1800 | 未按参考约束实现 | 当前当日和旧历史构造器均接受 1801、2000，拒绝 2001。1800 是参考 SDK 明确约束；本轮没有独立测定各旧协议服务器的实际极限。 |
| 成交明细自动翻到空页 | 未实现 | 高层只有 start/count 单页入口；没有 page_size、max_pages、全量翻页合并。 |
| 增量刷新最多 100 代码 | 不存在相应接口 | 尚无 `0x0547`；不能把旧快照的 80 上限作为该约束已实现。 |
| 股本变迁批量 1–200、默认 75 | 未实现 | 单只 Xdxr 请求，没有批量参数或多块批量结果模型。 |
| 新目录最多 1600 | 未实现新版目录 | 旧 `stocks` 按 1000 页聚合并对总数、重复和提前空页进行校验；这些是旧接口规则。 |
| 沪深北市场 0/1/2 | 仅普通 TCP 的 0/1 已接入 | 参数探测：行情构造器接受 0、1，拒绝 2。Symbol 能保存 bj，但 `tdx_market` 明确拒绝映射。 |
| 历史日期必须有效 | 校验不统一 | 旧历史逐笔会验证 YYYYMMDD；历史分时把 20260230 直接写入请求。 |
| 文件单块 1–60000 | 高低层不统一 | 高层块大小为 30000；低层 BlockInfoChunk 会编码 0、60000、60001，未作该范围拒绝。 |
| 文件路径 ASCII、最多 300 字节 | 未实现参考格式及校验 | 旧低层 filename 字段只有 100 字节；参数探测中 301 字节输入被写成 100 字节，没有错误。元信息请求的文件名字段另为 40 字节。 |
| 原始响应保留选项 | 低层可留证，高层未统一 | 部分旧对象保存 response，FrameObserver 可保存完整请求/响应；高层 owned DTO 不等于 include_raw 选项或每条 record_hex。 |

依据：[2](#source-2)[4](#source-4)[5](#source-5)[6](#source-6)。

## 4. 数据语义上的实际缺口

### 4.1 竞价快照与正式撮合

按 eltdx 的事件分类实现，先判断 `status_raw == 8`，满足时归为 AuctionSnapshot，即使时间为 09:25 也不是正式撮合。只有 `status_raw != 8` 且 `time_minutes == 9 * 60 + 25` 的记录才归为 OpeningMatch；其余归为 Trade。[7](#source-7)

`0x056a` 查询虚拟撮合过程，其 09:25 过程快照不能当作正式成交。正式开盘撮合来自 `0x0fc5/0x0fc6` 成交明细，使用参考 SDK 的 `opening_matches` 或正式撮合便捷入口时，也须遵循上述状态与时间条件。[3](#source-3)[7](#source-7)

本库 TransactionData 只有 time、price、vol、num、buyorsell。解码器保留未知 buyorsell 数值，包括 8，但没有 event_kind、opening_matches、actual_trades、auction_snapshots 或 after_hours_trades。保留状态原值是已有能力，正确分类和筛选仍未实现。对混合记录直接求成交量或成交额，业务层可能把快照计入成交；缺少 `0x056a` 也意味着无法恢复完整竞价数量过程。[2](#source-2)[4](#source-4)

### 4.2 复权能力与股本类别

`k_adjusted` 只对日线在本地计算 Qfq/Hfq，调整 OHLC，保留量额。Adj 枚举只有两种模式；没有服务端 adjust 编码、固定日期锚点及分钟/周/月等周期的一体化复权入口。算法筛选 `category == 1`，没有将其他股本类别自动纳入复权。不能把“可以计算日线前复权”表述为“支持参考接口所有复权模式”。[1](#source-1)[6](#source-6)

Xdxr 保留类别值并按旧分支解读四组字段，但没有对类别 15 的新语义专门建模。参考 SDK 采用的股本字段解释也与本库部分旧分支不同；这部分属于待核实的解码语义，不能仅凭命令号一致判为完整兼容。[3](#source-3)[4](#source-4)

### 4.3 成交量单位与分钟时间

当前 KlineData.vol 保存解码值，标准适配器要求显式 VolumeUnit。既有真实样本证明：所测股票/ETF日线 `9/1` 为手，日线 `4/1` 和 5 分钟 `0/1` 为股/份。指数分钟量未得到确定业务单位。全部周期统一乘或除 100 会产生错误量纲；现有显式适配机制可避免部分混用，但尚无覆盖全部品种/周期的自动单位规则。[8](#source-8)

当前没有 bar_time 开关，也没有在 KlineSeries 型结果中保留原始周期参数、时间标签语义与锚点。既有本库样本的 5 分钟时间为 09:35 至 15:00；参考 SDK 对 start/end 的解释不应直接套用并再偏移。完整时间口径需要以具体请求及原始响应校验。[3](#source-3)[8](#source-8)

### 4.4 目录、精度与财务字段

旧证券目录已经返回 volunit、decimal_point 和 pre_close；不能说本库完全没有这些字段。但 QuoteData 的价格还原固定除以 100，未使用目录的小数位驱动价格比例。对 ETF、债券等多小数品种，这项能力尚未完整接入和验证。[4](#source-4)

Symbol::is_a_share 和 limit::board_of 已有本地分类逻辑，但不是代码表 DTO 的 category/board 字段，也不是服务端特殊涨跌停表。把底层辅助函数与完整查询 API 分开统计，才能判断功能是否真正可直接调用。[4](#source-4)[9](#source-9)

FinanceInfoData 没有 EPS 字段；同一字段位置，本库命名 zhigonggu，参考 SDK 命名 eps_raw。现有独立 pytdx 夹具证明本库旧字段与该参考实现一致，但不能消除与增强语义的冲突。当前不应直接承诺可从该 DTO 取得已核实 EPS 计算 PE。[2](#source-2)[3](#source-3)[10](#source-10)

### 4.5 推送、帧匹配与采集流程

当前受管 TCP 流程是发送请求后读取一帧，没有未配对推送队列、消息路由、游标状态和增量合并。现有 decode_frame 校验压缩体长度和解压长度，没有在该函数中匹配命令号或消息号。FrameObserver 是留证回调，不是推送订阅接口。[5](#source-5)

runner 当前从冻结的本地 day 目录执行采集/发布流程；没有把这些 TCP 查询接口接成采集 Job。库级查询实现、协议级功能补齐和持续采集流程属于三个不同的完成条件。[11](#source-11)

## 5. 本轮验证与证据边界

本轮通过仓库自带的 Rust 1.89.0 工具链重新编译并运行以下现有测试：

```sh
CARGO_HOME=/Users/bytedance/Documents/rustdx/.cargo \
RUSTUP_HOME=/Users/bytedance/Documents/rustdx/.cargo/rustup \
RUSTDX_SKIP_INTEGRATION_TESTS=1 \
.cargo/bin/cargo +1.89.0 test -p jelven-rustdx-complete \
  --lib --test kline_decoders --test protocol_decoders --offline --locked
```

| 测试集合 | 通过 | 失败 | 证明范围 |
| --- | ---: | ---: | --- |
| 核心库现有单元测试 | 160 | 0 | 包括本地计算、分页、连接池和协议解析回归；部分外部服务案例函数内跳过。 |
| kline_decoders | 4 | 0 | 真实股票日线、指数日线、股票 5 分钟夹具及独立 pytdx 期望；请求/解码、异常输入和显式单位。 |
| protocol_decoders | 6 | 0 | 真实旧快照、单只财务、当日及旧历史逐笔夹具；字段、身份、数量、截断、未知方向和本地传输回放。 |

总计框架报告 170 通过、0 失败。外部 TCP 实盘测试被环境变量关闭；部分本地传输案例使用回放或模拟端点，不能计为真实主站验收。新增参数探测只调用公开构造器和读取编码结果，不产生模拟行情数据，也不联网。测试通过说明现有实现按既有断言工作，不能证明尚缺命令或参考接口语义已补齐。[10](#source-10)

本轮未修改 Rust 业务源码，未重新测定服务器 1800/1600/60000 等极限，未完成 21 项新接口的线上验收。既有真实采样只用于明确其已验证范围，没有扩展成全市场或长期运行结论。[8](#source-8)

## 6. 建议补齐顺序

1. **现有接口的约束与语义**：核实并统一逐笔页大小；补历史分时日期校验、文件参数拒绝策略；明确短页/空页分页规则；保留状态原值并提供正式撮合、竞价快照和盘后分类；补足量与时间语义的接口说明。
2. **现有业务的批量与目录**：接入新版证券目录和必要的市场验证，提供本地分类与 A 股列表/计数；补财务、股本的批量请求及多记录解码，核实 EPS 和类别 15。
3. **新增行情与竞价能力**：接入基础快照、增量刷新/队列及集合竞价过程；请求构造、对应解码器和推送状态管理一起实现。
4. **其余扩展**：分类榜单、特殊限制表、通用统计文件、服务端与定点复权、退市日线、副图和小走势图。每项按真实请求/响应夹具、边界校验和限定范围线上样本验收。

以上为基于缺口影响的实施建议，不代表这些改动已经执行。按清单验收时，应分别记录“源码实现”“客户端参数/回放验证”“限定范围线上验证”，避免把一种完成状态代替其他状态。

## 7. 来源

<a id="source-1"></a> **[1] 当前高层 Client API。** [client.rs](../../../crates/rustdx-core/src/tcp/stock/client.rs#L43-L579)；包括批量 K 线、日线复权、分时、逐笔和单只财务/股本。源码实现证据。

<a id="source-2"></a> **[2] 当前不可变请求与独立解码器。** [requests.rs](../../../crates/rustdx-core/src/tcp/stock/requests.rs#L1-L401)、[kline_request.rs](../../../crates/rustdx-core/src/tcp/stock/kline_request.rs#L29-L333)。参数数量、市场、日期和响应校验证据。

<a id="source-3"></a> **[3] 固定 eltdx 3.3.0 参考资料。** [命令表](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/COMMANDS_7709.md)、[限制常量](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/limits.rs)、[K 线方法](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-K线周期线.md)、[corporate.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/corporate.rs)。社区 SDK 的源码与文档证据。

<a id="source-4"></a> **[4] 当前具体命令与结果类型。** [目录](../../../crates/rustdx-core/src/tcp/stock/security_list.rs)、[当日分时](../../../crates/rustdx-core/src/tcp/stock/minute_time.rs)、[历史分时](../../../crates/rustdx-core/src/tcp/stock/history_minute_time.rs)、[当日逐笔](../../../crates/rustdx-core/src/tcp/stock/transaction.rs)、[旧历史逐笔](../../../crates/rustdx-core/src/tcp/stock/history_transaction.rs)、[财务](../../../crates/rustdx-core/src/tcp/stock/finance_info.rs)、[股本](../../../crates/rustdx-core/src/tcp/stock/xdxr.rs)、[五档快照](../../../crates/rustdx-core/src/tcp/stock/quotes.rs)、[文件块](../../../crates/rustdx-core/src/tcp/stock/block_info.rs)。

<a id="source-5"></a> **[5] 当前连接管理、保活和留证。** [tcp/mod.rs](../../../crates/rustdx-core/src/tcp/mod.rs#L111-L565)、[basic.rs](../../../crates/rustdx-core/src/tcp/basic.rs)、[pool.rs](../../../crates/rustdx-core/src/pool.rs#L103-L191)、[evidence.rs](../../../crates/rustdx-core/src/tcp/evidence.rs)。

<a id="source-6"></a> **[6] 当前分页与复权算法。** [pagination.rs](../../../crates/rustdx-core/src/tcp/stock/client/pagination.rs#L1-L57)、[adjustment.rs](../../../crates/rustdx-core/src/tcp/stock/client/adjustment.rs#L23-L87)。

<a id="source-7"></a> **[7] 参考 SDK 竞价事件分类。** [trades.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/trades.rs#L148-L156)、[正式撮合方法文档](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-当日0925正式撮合.md)。业务枚举为社区实现规则，本轮未独立实盘确认。

<a id="source-8"></a> **[8] 既有真实 K 线单位与时间验证。** [KLINE_VOLUME_UNITS_20261024.md](KLINE_VOLUME_UNITS_20261024.md)、[capture.json](../../testing/evidence/kline-volume-20261024/capture.json)、[results.json](../../testing/evidence/kline-volume-20261024/results.json)。范围限于报告所列证券、周期、服务器与时间。

<a id="source-9"></a> **[9] 本地标准化与涨跌停辅助能力。** [data.rs](../../../crates/rustdx-core/src/data.rs#L14-L107)、[model.rs](../../../crates/rustdx-core/src/data/model.rs#L7-L163)、[limit.rs](../../../crates/rustdx-core/src/limit.rs)、[rules.rs](../../../crates/rustdx-core/src/limit/rules.rs)。

<a id="source-10"></a> **[10] 本轮重新运行的既有测试及真实夹具身份。** [protocol_decoders.rs](../../../crates/rustdx-core/tests/protocol_decoders.rs)、[kline_decoders.rs](../../../crates/rustdx-core/tests/kline_decoders.rs)、[协议夹具清单](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)、[K 线夹具清单](../../../crates/rustdx-core/tests/fixtures/kline/manifest.json)。测试汇总与参数探测见本报告第 3、5 节。

<a id="source-11"></a> **[11] 当前 runner 采集范围。** [pipeline.rs](../../../apps/rustdx-runner/src/pipeline.rs#L96-L128)、[既有范围验证](TDX_ONLY_VALIDATION_20261024.md)。业务流程源码及此前限定范围验证。
