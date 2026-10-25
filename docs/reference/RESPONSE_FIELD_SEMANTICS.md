# 接口返回字段解析状态

<!-- lifecycle:registry -->
读者：应用集成/测试；责任：协议维护者。源码核验：2026-10-25；每90日及字段解释或资格证据变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

核对日期：2026-10-24。对应当前工作区源码的 **26个同码命令（2项会话、24项业务）**，关联需求 FR-03、FR-07、FR-13。当前已经实现结构解码、部分单位换算和业务派生；字段语义、日期、数量单位及来源覆盖仍按各自证据范围判断，不能把26项入口覆盖理解为所有返回字段已完成业务核验。

本页维护字段解析状态、候选解释及其验证边界；完整签名、字段类型和参数限制见[26命令接口手册](COMMAND_API.md)。跨接口消费规则统一维护在[外部应用调用约定](../development/APPLICATION_INTEGRATION.md)，实际回归与实盘范围见[当前验收矩阵](../testing/PRODUCTION_ACCEPTANCE.md)。本次只更新文档，不新增采样或提升已有证据资格。

<a id="parsing-status"></a>
## 1. 状态定义与实际API

同一字段可以同时“已解码、已换算、业务含义仍为候选”。下表是文档分类，**不是所有DTO都具有的统一枚举**。

| 文档状态 | 当前能力 | 可得结论 |
| --- | --- | --- |
| 已结构解码 | 字节、整数、浮点、日期槽或GBK文本已按对应布局读取 | 布局和数值检查通过；不自动确认外部业务含义 |
| 已单位换算 | 按命令标尺、证券目录精度或显式源单位转换 | 可按所列口径读取；证据范围之外的品种不能自动沿用 |
| 已业务派生 | 从已解析字段计算涨跌幅、净额，或按版本化规则分类 | 公式/规则已实现；结果仍受输入单位、日期、分类资格限制 |
| 候选或推断 | 社区基线、兼容标尺、代码前缀或协议规则提供解释 | 必须保留依据及适用范围，不能标为交易所独立核验 |
| 未知或未提供 | 未知槽、未回显日期、缺精度或不可用业务值 | 保留raw、None或不可用原因，不补造业务值 |
| 范围内独立核验 | 字段与独立依据在指定证券、时点、原始值等条件下匹配 | 仅在该证据范围内成立；不扩展到其他证券、日期或字段 |

代码目前按数据对象分别暴露资格，不能给全部接口套用 `semantic_status`：

| 对象 | 真实字段/getter | 判定含义 |
| --- | --- | --- |
| `FinanceFieldValue / FinanceSnapshot` | `semantic_status()`、`value()`、`candidate_value()`、`unit()`、`evidence()`、`require_fields()` | `Verified / BaselineCandidate / Unknown`；默认业务值仅Verified，必需字段整组门禁同时校验报告期绑定 |
| `TradeRecord` | `event_kind()`、`phase()`、`direction()`、`qualification()` | 事件分类与成交资格分别读取；`ConfirmedByProfile`只表示协议配置规则确认 |
| `TradeCollection` | `coverage()`、`termination()`、`warnings()`、`counts()` | 已取得页的本地聚合资格，不主动联网；`SourceEnumerated`不是交易所全历史 |
| `MinuteSeries`（0feb） | `date_qualification()`、`require_trading_date()`、`with_date_reference()` | 解码默认Unverified；完整独立价格参照可绑定该次结果为IndependentlyReferenced，篡改字段/来源后失效 |
| `QuotePage / BasicQuote` | `precision / metadata_generation`、`scale`及价格Option | 表达价格换算依据或缺精度；没有全字段Verified枚举 |
| `CanonicalBar` | `provenance` | 保留请求、解码类型、源量/单位依据和可得网络来源；调用方指定单位不自动构成独立核验 |
| `ReportedTurnover / TradeEstimateSummary` | 源额读取 `availability()`；估算经 `regular_with_auctions() / after_hours() / candidates()`读取分量的 `availability()` | 源成交额与明细估算分开，分别读取来源/口径/不可用原因；Available不代表估算成为精确成交额 |
| 扫描集合/订阅状态 | `complete / termination / atomic`、`DepthSnapshot.status` | 约束结果完成程度或会话有效性，不替代字段语义核验 |
| `ResourceFile / StatsRow` | `require_complete()`、`complete/termination`、`candidate_values/fields_raw` | 资源读到末尾与候选统计语义分别检查；ZIP/CRC/GBK通过不能升级业务单位或日期 |

<a id="command-fields"></a>
## 2. 26命令字段状态矩阵

“已解析”描述当前代码行为；未知字段、推断字段和兼容输出在同一行单独列出。逐条完整声明仍以接口手册为准。

| 命令 | 返回模型与主要字段 | 当前解析状态 | 业务边界与待核实项 |
| --- | --- | --- | --- |
| [0004](COMMAND_API.md#cmd-0004) | `HeartbeatAck`：`server_date_raw / server_date / reserved_raw / raw_payload` | 日期整数解码，合法日期转Option | 日期零值/非法值解释为None并保留raw；心跳日期不证明证券交易日期或正在交易 |
| [000d](COMMAND_API.md#cmd-000d) | `Client`会话、连接身份 | 握手流程与响应身份检查已实现 | 未提供完整 `HandshakeInfo`业务模型，不输出已核实主站名称/产品/会话时段 |
| [000f](COMMAND_API.md#cmd-000f) | `XdxrData`：身份、日期、category、四个复用槽 | 按类别解码；category=1有分红/送配解释 | 其他类别不得套分红单位；6/15等增强语义和逐槽raw模型仍有差距 |
| [0010](COMMAND_API.md#cmd-0010) | `FinanceSnapshot / FinanceRaw`：31槽、日期、原编码、单位、资格及证据 | raw已解码；24个命名槽有候选单位；默认10字段有范围内独立证据 | 其余命名槽为候选，7槽Unknown；默认证据只适用于固定平安银行样本，详见第3节 |
| [02c5](COMMAND_API.md#cmd-02c5) | `BlockMetadata`：size、MD5及raw/来源 | 大小和MD5字符串已解码；verify_content检查完整长度/32hex/实际MD5，保留reserved_raw与尾部 | 单独metadata查询不代表验算；MD5一致性不证明文件业务权威性 |
| [02cf](COMMAND_API.md#cmd-02cf) | `CompanyInfoDirectory`：栏目名、文件名、start/length及raw | 固定152字节目录、GBK文本、字节范围已解码 | 栏目名不构成结构化财报字段；市场2仅参数支持，网络范围未验收 |
| [02d0](COMMAND_API.md#cmd-02d0) | `CompanyInfoTextChunk / CompanyInfoText`：raw字节、text、chunks、complete | 按目录范围读齐字节后GBK解码 | 是栏目文本；未自动抽取财务指标、报告期、公告事件或行业事实 |
| [044d](COMMAND_API.md#cmd-044d) | `SecurityDirectoryData`：code/name、volume_unit、decimal、previous_close、raw槽、category/board | 37字节记录、完整16字节名称、整数/f32已解码 | category/board为前缀推断；volume_ratio_base为社区候选解释；unknown3_raw未知 |
| [044e](COMMAND_API.md#cmd-044e) | `u16`证券目录条数 | 总数量已解码，用于044d范围检查 | 包括多类证券，不是A股数/上市公司数；不确认目录是交易所完整名单 |
| [0452](COMMAND_API.md#cmd-0452) | `SpecialLimitRecord`：market_raw/code_number_raw/code、upper/lower_price及bits | 代码格式化和上下限f32已解码 | 仅主站特殊表，缺席不证明无限价；规则生效日与适用制度须另核实 |
| [051b](COMMAND_API.md#cmd-051b) | `IntradayAuxPoint`：整数双序列或previous_day/current_day浮点双序列 | selector=11采用浮点布局，其他采用整数布局 | 序列名为供应商/社区解释，量单位与未命名selector仍待核实；没有源日期/时间戳 |
| [052b](COMMAND_API.md#cmd-052b) | `DailyBar`（旧名`DelistedBar`）：响应日期、OHLC、股数/手数、金额及原始槽 | 价格/1000，volume_shares为u32整数股，volume_lots=股/100；金额为非负有限binary32 | 两节点3只在市A股的24条日线全部字段与官网day精确一致；旧金额算法已修复，原历史材料不改；未知尾部保留，市场2/完整历史未验收 |
| [052d](COMMAND_API.md#cmd-052d) | `KlineData / IndexKlineData`：dt/OHLC/vol/amount，指数家数；可转CanonicalBar | 价格/1000；金额按非负有限IEEE754 binary32；标准适配显式源量单位 | vol依品种/category保留源值；dt为bar时点，非逐笔时间；区间/源成交额资格另检查 |
| [0537](COMMAND_API.md#cmd-0537) | `TodayMinuteSeries / TodayMinutePoint`：price/average_price/volume_raw、合成time_label、raw | 价格按目录精度；均价额外除100；量保留raw | trading_date=None，标签按索引合成；240点不证明日期、全会话交易或全品种单位 |
| [053e](COMMAND_API.md#cmd-053e) | `QuoteData / QuotePage`：OHLC、累计量额、五档、涨跌/涨速、servertime及精度 | 价格按目录精度；change/change_percent本地计算，speed按/100换算，时间字符串解析 | QuoteData无market/raw全槽；部分辅助槽被跳过；量单位非全品种核验，servertime不是逐笔成交时间 |
| [0547](COMMAND_API.md#cmd-0547) | `QuoteRefresh / QuotePush / DepthSnapshot`：BasicQuote五档、游标、接收时点、会话状态 | 五档按目录精度；游标与累计行情更新已解析，维护全证券基线 | 游标不是时间戳；raw状态/尾部未全面命名；Synchronized不代表实际盘中主动推送已验收 |
| [054b](COMMAND_API.md#cmd-054b) | `CategoryQuote`：BasicQuote一档及rise_speed/short_turnover/opening_rush等 | 有目录的价格换算；上述三个有符号指标/100；额外浮点及raw保留 | 指标窗口/分母和未命名槽仍受供应商语义限制；缺精度价格None；动态榜单不构成冻结截面 |
| [054c](COMMAND_API.md#cmd-054c) | `SnapshotPage / BasicQuote`：身份、可选价格、一档、原始量额/时间槽、tail_raw | 基础报价结构与目录价格换算已实现 | time_raw未自动形成成交时间戳，status_or_unknown_raw等保留；缺失证券另列missing_codes |
| [056a](COMMAND_API.md#cmd-056a) | `AuctionPoint`：minute/second、price及bits、匹配量、未匹配绝对量/符号 | 报文时分秒和f32价格已解码；有符号数量拆绝对值/符号 | 是竞价过程快照，含09:25；正负号未升级为交易所确认买卖侧，量单位/实际日期未全面核实 |
| [06b9](COMMAND_API.md#cmd-06b9) | `BlockFile/BlockRecord`及`FileContentChunk/ResourceFile/TdxStatsResource` | 板块检查MD5/严格成分；通用300字节路径按声明块长读取，短/空块终止；统计ZIP检查大小/CRC/GBK/唯一键，保留全部原始列 | 板块前4字节/代码第7字节仍未知；通用尾部保留、不带声明hash/原子性；统计命名槽/单位/日期为候选，市场键不能自动套行情枚举；分类非权威 |
| [0fb4](COMMAND_API.md#cmd-0fb4) | `MinuteTimeData`：price/vol | 兼容价格差值累加/100，量保留源值 | 没有均价/日期/时间戳资格；辅助字段跳过，未接目录精度，不推定ETF价格正确 |
| [0fc5](COMMAND_API.md#cmd-0fc5) | `TradePage / TradeRecord`：原始价量/笔数/状态、事件/阶段/方向/资格 | raw已保留，price兼容/100；版本化事件分类已实现 | num不是稳定ID，量未自动换股；协议规则确认不等于独立业务核验；当日无响应交易日 |
| [0fc6](COMMAND_API.md#cmd-0fc6) | `TradePage / EnhancedHistoryTransactionPage`：增强raw、参考字节、事件视图 | 独立增强布局、笔数/状态/事件分类已实现 | 价格/量/事件资格同0fc5；请求日期不是回显日期；筛选视图不替代raw页和原始偏移 |
| [0fd1](COMMAND_API.md#cmd-0fd1) | `SparklineSeries / SparklinePrice`：base_price、price及bits、参数回显 | f32基准价/价格点和证券/selector回显已解析 | window不等于点数；无真实时间、OHLC或成交量，不转为标准K线 |
| [0feb](COMMAND_API.md#cmd-0feb) | `MinuteSeries`：昨收/今开f32、分钟价/均价/量、请求selector、日期资格 | 分钟价按目录精度，均价额外除100，raw及完整页保留；日期schema2编码负YYYYMMDD | 旧编码错位已修复；默认日期门禁Unsupported，调用方完整独立价格参照可资格化该次绑定结果；不证明量/均价/合成标签 |
| [0ffc](COMMAND_API.md#cmd-0ffc) | `MoneyFlowDaily`：date_raw、total_amount、两套四档净额、比例及raw | 日期槽/浮点/分档已解码；净额与百分点公式已实现 | 净额沿total源单位；分档为供应商统计，不证明“主力”真实身份或精确交易所资金流 |

<a id="finance-fields"></a>
## 3. 0010：31个财务字段与证据范围

依据：[原始模型](../../crates/rustdx-core/src/tcp/stock/finance_raw.rs)、[语义模型与默认证据](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs)、[独立字段期望](../../crates/rustdx-core/tests/fixtures/protocol/finance.semantic.expected.json)。

下表偏移以 `FinanceRaw::body()` 的136字节主体为基准，不包含数量、市场和代码。所有浮点槽先保留f32位模式，再扩展为f64换算；“候选元/股”等单位是当前基线定义，不证明所有字段已有独立核验。表中“默认有证据”只有在市场0、代码000001、更新日期20260815及**该字段原始位模式完全匹配**时才产生Verified。

| 主体偏移 | `FinanceField` | 当前业务解释 | 候选单位/倍率 | 默认独立证据 |
| --- | --- | --- | --- | --- |
| 0 | `CirculatingShares` | 流通股本 | 股，×10000 | 无 |
| 16 | `TotalShares` | 总股本 | 股，×10000 | 有，限样本命中 |
| 20 | `Slot01` | 未知槽 | Unknown，不换算 | 无 |
| 24 | `Slot02` | 未知槽 | Unknown，不换算 | 无 |
| 28 | `Slot03` | 未知槽 | Unknown，不换算 | 无 |
| 32 | `Slot04` | 未知槽 | Unknown，不换算 | 无 |
| 36 | `Slot05` | 未知槽 | Unknown，不换算 | 无 |
| 40 | `ReportedEps` | 来源披露每股收益 | 元/股，×1 | 有，限样本命中 |
| 44 | `TotalAssets` | 总资产 | 元，×1000 | 有，限样本命中 |
| 48 | `CurrentAssets` | 流动资产 | 元，×1000 | 无 |
| 52 | `FixedAssets` | 固定资产 | 元，×1000 | 无 |
| 56 | `IntangibleAssets` | 无形资产 | 元，×1000 | 无 |
| 60 | `ShareholderCount` | 股东数量 | 计数，×1 | 无 |
| 64 | `CurrentLiabilities` | 流动负债 | 元，×1000 | 无 |
| 68 | `LongTermLiabilities` | 长期负债 | 元，×1000 | 无 |
| 72 | `CapitalReserve` | 资本公积 | 元，×1000 | 无 |
| 76 | `NetAssets` | 净资产/权益候选槽 | 元，×1000 | 有，限样本命中 |
| 80 | `OperatingRevenue` | 营业收入 | 元，×1000 | 有，限样本命中 |
| 84 | `Slot17` | 未知槽 | Unknown，不换算 | 无 |
| 88 | `Receivables` | 应收款项 | 元，×1000 | 无 |
| 92 | `OperatingProfit` | 营业利润 | 元，×1000 | 有，限样本命中 |
| 96 | `InvestmentIncome` | 投资收益 | 元，×1000 | 无 |
| 100 | `OperatingCashFlow` | 经营现金流 | 元，×1000 | 有，限样本命中 |
| 104 | `CashFlowNetChange` | 现金流净变动 | 元，×1000 | 无 |
| 108 | `Inventory` | 存货 | 元，×1000 | 无 |
| 112 | `ProfitBeforeTax` | 利润总额 | 元，×1000 | 有，限样本命中 |
| 116 | `AfterTaxProfit` | 税后利润候选槽 | 元，×1000 | 无 |
| 120 | `NetProfit` | 净利润候选槽 | 元，×1000 | 有，限样本命中 |
| 124 | `RetainedProfit` | 未分配利润 | 元，×1000 | 无 |
| 128 | `NetAssetsPerShare` | 每股净资产 | 元/股，×1 | 有，限样本命中 |
| 132 | `Slot29` | 未知槽 | Unknown，不换算 | 无 |

固定样本全部命中时为10个Verified、14个BaselineCandidate、7个Unknown；其他证券/更新版本在没有适用独立profile时为24个BaselineCandidate、7个Unknown。`value()`对Candidate/Unknown返回None；`candidate_value()`对已知单位提供显式基线换算，对Unknown返回None。候选值与已验证值数值相同也不改变资格。

`FinanceProfile::verify`记录逐字段预期raw_bits，调用方负责其独立依据；代码只能检查范围与位模式匹配，不能审计外部证据真实性。未知槽不能通过该入口升级。样本通过的标准值仍保留源f32精度，不替换成公告显示的十进制整数。

| 元信息 | 当前解析/赋值来源 | 解释边界 |
| --- | --- | --- |
| `province() / industry()` | 主体4/6偏移的原u16编码 | 没有自动映射为已核实行政区/行业名称 |
| `updated_date_raw() / updated_date()` | 主体8偏移的源日期槽 | 更新日不是报告期；0为未知，非法非零日期解码失败 |
| `ipo_date_raw() / ipo_date()` | 主体12偏移的源日期槽 | 主站上市日期字段不替代经公告核实的历史上市阶段/证券状态 |
| `report_period` | 命中profile且至少一个字段Verified才由证据提供 | 默认样本为2026-06-30，不能从更新日或财务等式推断 |
| `eps_basis` | 一般为reported_unspecified，默认样本EPS命中后为basic_and_diluted_interim | 不自动作为TTM、年化或其他证券的基本/稀释口径 |
| `net_profit_basis / net_assets_basis` | 一般unspecified；默认样本相应字段命中后为attributable_to_bank_shareholders / consolidated_total_equity | 两者口径不同；不能把所有权益槽解释为归母权益 |
| `semantic_schema_version / profile_version` | 当前为1 / tdx-0010-field-units-v1 | 描述本库规则版本，不是财报版本或外部核验完成标志 |

旧 `finance_legacy / FinanceInfoData` 保留历史字段和换算口径，没有新字段资格契约，不能与标准值按同名字段直接混用。

<a id="trade-fields"></a>
## 4. 0fc5/0fc6：事件分类与金额资格

依据：[事件模型](../../crates/rustdx-core/src/tcp/stock/trade_models.rs)、[增强历史布局](../../crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs)、[明细金额估算](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs)、[052d源成交额](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs)。

`TradeRecord`保留 `time_minutes / price_delta_raw / price_raw / volume_raw / trade_count_raw / status_raw / tail_raw / byte_range / protocol`，通过getter读取。当前 `TRADE_SEMANTIC_SCHEMA_VERSION=1` 将原始状态和时分映射到事件、阶段、方向及资格：

| 原始条件 | 当前分类/资格 | 业务解释边界 |
| --- | --- | --- |
| status=8，任意时分 | AuctionSnapshot、AuctionProcess、方向Unknown、ExcludedByProfile | 优先于09:25/15:00边界；保留原记录，不进入默认已执行成交视图 |
| 非8且09:25 | OpeningMatch、Opening | 事件类型与成交资格独立；仍按状态/协议决定qualification |
| 非8且15:00 | Trade、ClosingMatch | 时分分类本身不能把未知状态升级成已确认成交 |
| status=0/1/2，位于09:25、15:00或常规窗口 | 0fc5/0fc6为ConfirmedByProfile；方向分别Buy/Sell/Neutral | 常规窗口为09:30–11:30和13:00–15:00之前；只是当前协议规则支持 |
| status=5且15:05–15:30 | Trade、AfterHoursFixedPrice；0fc5/0fc6为ConfirmedByProfile，方向Unknown | 不根据盘后状态推断买卖方向 |
| 其他status=0/1/2/5 | Candidate，阶段/事件由实际时分规则决定 | 不能仅看到熟悉的状态值就纳入已确认成交 |
| 其余状态 | qualification=Unknown，方向Unknown | 未知状态保留，不自行改名或过滤后继续按筛选条数分页 |

旧0fb5缺少笔数槽，`trade_count_raw()`为None，兼容 `TransactionData.num=0`只是缺失占位。其受支持时间/状态最多Candidate。默认 `confirmed_trades()`仅选ConfirmedByProfile；`actual_trades`等兼容视图名称不改变独立核验边界。`TradeCounts`计记录条数，不是原始笔数槽之和。

价格getter当前按/100兼容尺度，数量保持源raw；没有自动接入053e目录精度或统一乘100。原始事件页、严格网络枚举 `TransactionCollection` 与本地聚合 `TradeCollection` 是不同对象。页的raw_count包含竞价和未知记录；时间/排序/覆盖约束见[分页消费规则](../development/APPLICATION_INTEGRATION.md#3-分页完成状态与失败)。

| 金额结果 | 当前能力 | 不可替代的边界 |
| --- | --- | --- |
| `TradeEstimateSummary` | 显式指定数量单位后，按已支持事件计算price×quantity，分别统计常规/竞价、盘后及候选分量 | `AmountOrigin::RecordPriceTimesQuantity`是聚合末价估算；缺单位/资格时有Unavailable或Partial，不补零 |
| `ReportedTurnover` | 由052d原始amount槽聚合，`AmountOrigin::VendorReported`；根据profile和采样检查可用性 | 源金额与明细估算分开；AfterHours无独立源，Combined不补造盘后成交额，未确认profile不输出假业务合计 |

<a id="market-fields"></a>
## 5. 行情、K线、目录与时间字段

依据：[五档解析](../../crates/rustdx-core/src/tcp/stock/quotes.rs)、[基础报价](../../crates/rustdx-core/src/tcp/stock/extended_quote.rs)、[当日分时](../../crates/rustdx-core/src/tcp/stock/today_minute.rs)、[近期分时](../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)、[竞价过程](../../crates/rustdx-core/src/tcp/stock/auctions.rs)、[标准行情模型](../../crates/rustdx-core/src/data/model.rs)。

| 字段组 | 已实现处理 | 资格/未提供内容 |
| --- | --- | --- |
| 053e/054c/0547/054b价格 | 基价+差值按目录PriceScale还原；支持decimal 2/3/4 | 054b可保留缺精度价格None，其他对应严格入口拒绝缺精度；目录来源不等于全品种业务验收 |
| 053e涨跌额/涨跌幅 | `change=price-last_close`；昨收>0时 `change_percent=change/last_close×100` | 为本地派生；昨收为0时兼容输出0，不能把该占位结果作为真实“涨跌幅为0” |
| 053e涨速/服务器时间 | speed为有符号原值/100；servertime按社区公式格式化为时分秒 | 不提供响应交易日；异常时间字符串为空；不是逐笔成交时间或完整带日期时间戳 |
| `BasicQuote`状态/时间/游标 | `time_raw`与`update_cursor`按命令分别保存，数量/金额槽及tail_raw保留 | `active_raw / status_or_unknown_raw / after_outside_raw / open_amount_raw`没有统一已核实业务解释；游标不转为时间 |
| 054b额外指标 | rise_speed、short_turnover、opening_rush按有符号原值/100；two_minute_amount等为源f32 | 公式窗口、分母、单位和extra/depth等raw槽不能仅凭名字确定 |
| 052d与052b OHLC | 各自差值布局还原为千分元价格；052d区分股票/指数解码 | 052b有原始price/delta/record；普通KlineData不保留全部raw，需相应来源模型或原帧归档 |
| 052d/052b日期 | 读取响应bar日期槽；052b为UTC+8日期零时，052d按category生成dt | 日bar的时间部分是解析约定，不能解释为真实开盘成交时刻；期末bar是否闭合另核实 |
| 标准K线与量 | CanonicalBar保存源量、源单位依据、原category/request及解码类型；VolumeUnit显式转换为股 | 标准模型不携带指数上涨/下跌家数；仅有Period不足以还原category 4/9或7/8 |
| 0537/0feb分钟价/均价 | 第一条提供基值，后续差值相对第一条；均价比目录价格额外除100 | 不是逐条相对前一分钟累计；volume_raw不自动换股；0feb头部昨收/今开直接解码f32 |
| 0537/0feb time_label | 按索引合成09:31–11:30、13:01–15:00会话标签 | 不是报文时间；索引完整不证明交易时刻或实际日期 |
| 0feb日期 | 请求selector保留，server_trading_date初始None；资格和绑定私有 | 默认Unsupported；独立同证券/日/标尺/全部价格点匹配后绑定请求、节点、正文与字段，公开修改使门禁/序列化失效；非空与240点不提升资格 |
| 056a时分秒/符号 | minute_raw/second_raw生成time_label；unmatched_signed_raw取绝对值和signum | 日期未回显；符号不升级买卖侧，09:25过程点不自动当作0fc6正式开盘撮合 |
| 044d名称/精度/分类 | name读取16字节GBK去NUL，volume_unit/decimal/previous_close读取源字段 | volume_unit不自动证明所有行情量槽用同一单位；category/board为前缀提示；volume_ratio_base候选解释与unknown3_raw继续保留 |
| 000f四个复用槽 | category=1为每十股红利、每股配股价、每十股送转/配股比例 | 同一槽在其他类别可能为股本/特殊比例，不能全表套category=1计算 |

0feb旧非空日期冲突已定位为ordinal请求编码错误，修复样本通过同日0fb4和带日期日线对照；原始错位材料保留，见[修复报告](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。响应仍没有日期回显，通用日期门禁的消费要求由[接入规则第2节](../development/APPLICATION_INTEGRATION.md#2-价格数量金额和未知值)定义。0537、0fb4、0fc5/0fc6及056a也不能用本地获取日或请求日补造响应交易日；这些类型没有统一的 `require_trading_date` getter。

<a id="flow-f10-fields"></a>
## 6. 资金流、F10与板块解释边界

0ffc依据[解码源码](../../crates/rustdx-core/src/tcp/stock/money_flow.rs)保留21个u32槽、total_amount位模式和16个u16分档。`date_raw`来自响应并检查有效性/重复；total_amount来自raw[1]的非负有限f32。其余未命名raw槽不自动变成业务指标。

令 `b=buckets_raw`，`d[j]=b[2j]-b[2j+1]`（j为0至7），四档i按超大、大、中、小排列：

| 派生字段 | 当前公式 | 口径 |
| --- | --- | --- |
| `net_amounts[i]` | `d[2i] × total_amount / 50000` | 第一套各档净额，沿total_amount源单位 |
| `active_buy_net_amounts[i]` | `d[2i+1] × total_amount / 50000` | 第二套各档主买净额，同一源单位 |
| `main_net / main_ratio` | `(d[0]+d[2]) × total_amount / 50000`；`(d[0]+d[2])/500` | 第一套超大+大；比例为百分点 |
| `active_buy_net / active_buy_ratio` | `(d[1]+d[3]+d[5]+d[7]) × total_amount / 50000`；该差值和/500 | 第二套四档和；比例为百分点 |

负净额合法；main_ratio=5.0表示5个百分点。以上是供应商分档算术，不提供已核实资金账户身份、大小单阈值或统一人民币币种证明，不能与本库逐笔估算额视为同一数据。

F10的02cf已解释栏目名、GBK文件名和字节范围；02d0已完成该范围读取、原字节拼接和文本解码。`CompanyInfoText.complete`只表示指定字节范围已读齐，**不表示财务指标、报告期、公司事件等文本内容已结构化或核验**。兼容 `f10()`只交付栏目名与文本；需要raw/分块来源时读取CompanyInfoText。

02c5与06b9提供主站文件元数据、经全文件MD5验算的下载和严格板块成分解析；`block_type`是供应商类别，`BlockRecord.code`没有市场。两节点概念/指数原字节、MD5及全部字段的独立核对见[新证据](../testing/evidence/block-integrity-fix-20261024/independent.json)。通用资源另有300字节路径和声明块长合同，统计ZIP原始槽与候选映射另行核验，见[资源修复](../archive/development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md)。成分文本不能替代点时上市名单/官方行业分类；完整接口和语义边界见[06b9](COMMAND_API.md#cmd-06b9)。

<a id="evidence-boundaries"></a>
## 7. 当前证据与后续验收

| 字段范围 | 已有依据入口 | 当前未关闭范围 |
| --- | --- | --- |
| 财务与价格精度 | [实施记录](../archive/development/FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)、[独立财务期望](../../crates/rustdx-core/tests/fixtures/protocol/finance.semantic.expected.json) | 其他证券/更新版本、候选槽单位、7个未知槽及报告口径 |
| 成交事件/金额 | [事件模型记录](../archive/development/TRADE_EVENT_MODEL_IMPLEMENTATION_20261024.md)、[金额语义记录](../archive/development/TRADE_AMOUNT_SEMANTICS_IMPLEMENTATION_20261024.md)、[金额差异分析](../archive/validation/ENHANCED_HISTORY_AMOUNT_DIFFERENCE_20261024.md) | 交易所逐笔完整性、全品种价量单位、末价估算误差及独立盘后源额 |
| 近期分时/竞价 | [实际合同报告](../archive/validation/LIVE_COMMAND_CONTRACT_VALIDATION_20261024.md)、[非空夹具与窗口报告](../archive/validation/NONEMPTY_FIXTURES_PUSH_WINDOW_20261024.md)、[日期编码修复](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md) | 0feb通用日期资格/完整保存窗口，竞价量单位/精确侧别及跨证券日期对应 |
| 0547推送 | [推送验证](../archive/validation/QUOTE_PUSH_VALIDATION_20261024.md) | 实际连续交易窗口中被动推送、状态变化与游标推进共同成立 |
| 26同码命令/F10/退市/资源 | [26命令报告](../archive/validation/ELTDX_3_3_0_26_COMMANDS_IMPLEMENTATION_20261024.md)、[资源合同修复](../archive/development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md) | 市场2、完整增强合同、供应商统计业务单位/日期与其他资源格式；通用下载不带声明hash/原子性 |

字段升级为已核验时，须记录证券/市场、命令/参数、日期/报告口径、源单位、原请求/响应、节点/获取时点、独立参考、原始位模式或允许误差及适用范围。profile命中、浮点误差范围、公式一致性和实际日期真实性分别检查；一个维度通过不能替代其他维度。

工程回归应覆盖候选值不进入默认业务值、未知/缺失不补零、错误位模式或日期不命中旧证据、原始事件不因筛选丢失、源量单位显式、分页不完整不升级、0feb日期门禁和订阅失效状态。真实业务验收还须取得相应独立样本；本页没有新增“全部字段已验证”结论。证据写入新记录并遵循[固定证据管理](../testing/EVIDENCE.md)，不得改写冻结字节或历史摘要。
