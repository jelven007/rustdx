# rustdx 当前 26 命令接口手册：业务含义与使用约束

<!-- lifecycle:registry -->
读者：应用集成者；责任：API维护者。源码核验：2026-10-25；每90日及签名、参数或字段变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

核对日期：2026-10-24。范围为 eltdx 3.3.0 注册的 **2项会话 + 24项业务命令**，展示 rustdx **当前源码实际可用接口**；包含高层 Client、独立请求/解码器、结果字段及相关本地处理。当前工作区包含未提交源码，不能用已发布 crate 版本推定所有新增方法均存在。[cite:1](#cite-1)[cite:6](#cite-6)

推荐从 `rustdx_complete::tcp::stock::Client` 调用。本手册按源码签名与校验定义接口，协议社区资料解释候选业务语义，真实帧和专项报告界定已验收范围；同opcode覆盖不等于所有市场、字段和实盘契约已验收。本次目录重组更新源码和证据导航，不增加协议行为。

核心源码位于 `crates/rustdx-core`，第8节的 `src/...` 以该crate为基准。外部应用先按[共同调用约定](../development/APPLICATION_INTEGRATION.md)处理身份、单位、完成状态、批量错误、订阅及共享预算；当前工程验收见[验收矩阵](../testing/PRODUCTION_ACCEPTANCE.md)。

返回字段的已解码、已换算、派生、候选、未知及范围内独立核验状态见[字段解析状态矩阵](RESPONSE_FIELD_SEMANTICS.md#command-fields)。该文档列出26命令的语义边界、31个财务槽和实际资格getter；本手册继续维护完整接口声明。字段名、解码成功、非空结果或complete都不自动代表所有业务含义已核实。

准确名称说明：完整成交事件单页是 `trade_page / history_trade_page`；当前源码没有 `Client::transaction_page / history_transaction_page`。`finance` 返回 `FinanceSnapshot`；旧财务视图须显式用 `finance_legacy`。低层接口与高层方法名、默认值和参数顺序不保证与 eltdx Python API 相同。

## 1. 阅读方法与公共约束

### 1.1 参数类型和身份

| 参数 | Rust 类型与含义 | 使用约束 |
| --- | --- | --- |
| market | `u16`；0深市、1沪市、2北交所市场表达 | 大部分行情/目录接口仅允许0/1；F10/052b允许0/1/2；允许表达2不等于已实测 |
| code | `&str`；证券代码 | 大部分接口六位ASCII字母数字；F10/052b是trim后六位ASCII数字；不能通用地自动加市场前缀或T前缀 |
| stocks | `&[(u16,&str)]` | 身份键必须是市场+代码；沪深可能有相同六位代码；单帧和批量上限分别计算 |
| date / begin / end | `u32`或`Option<u32>`；YYYYMMDD | 必须是有效日期，区间是闭区间；None含义由接口规定，不是所有命令的统一“今日” |
| start | `u16`或`u32` | 证券目录、K线/成交/退市、竞价的偏移类型不同；F10 offset是字节数 |
| count | `u16`或`u32` | 请求条数而不是保证返回条数；返回数小于请求数不总是结束 |
| timeout / page_delay | `Duration` | timeout是操作预算；page_delay是客户端节流建议，不是主站QPS承诺 |
| max_parallel | `usize` | 0自动选择线程数，再受请求数与连接预算约束；不是可以开启任意连接数 |

`market_of(code)` 仅按六位数字前缀推断常见沪深市场，对沪市000001指数、北交所及其他特殊品种不完整。业务调用显式指定market，不用它作为证券身份权威来源。目录的 category/board 同样是前缀提示，并非交易所回显的上市分类。[cite:1](#cite-1)

### 1.2 所有权、结果和错误

新目录/行情/分时/事件/F10/退市 DTO 大多拥有返回数据；但 `KlineData<'a> / IndexKlineData<'a> / BatchKline<'a> / BatchBars<'a>` 的 code 仍借用输入字符串，输入生命周期必须覆盖结果使用期。不能将所有 Client 返回值统称完全 owned。

| 返回形式 | 含义 | 调用者必须检查 |
| --- | --- | --- |
| `io::Result<T>` | 网络、输入或解码错误可以传播 | 处理Err；不要转换成成功空Vec而丢失失败 |
| `crate::Result<T>` | 本库错误类型，部分目录/板块兼容入口采用它 | 和io::Result不是相同类型，示例用统一错误边界 |
| `Vec<T>` | 当前可得结果或兼容视图 | 空Vec不证明停牌/无交易，Vec本身通常无独立完整性状态 |
| `io::Result<Vec<io::Result<T>>>` | 批量外层批次结果、内层每请求独立结果 | 外层Ok不等于每证券成功；按输入顺序逐项检查 |
| `Vec<BatchKline/BatchBars>` | K线批量结果 | 每项的result独立；不要丢掉失败证券 |
| `Option<T>` | 未提供、未知、不可用或缺精度等明确状态 | None不是数值0，不可用`unwrap_or(0)`给业务值 |
| `CommandCollection / DelistedBarsCollection` | 保留页及终止原因的分页结果 | 必须读取complete/termination/atomic；预算不足也可能Ok但不完整 |
| `TransactionCollection` | 严格成交枚举成功结果 | 预算/来源/排序失败返回Err；成功仍只说明观察到主站空页 |
| `DepthSnapshot` | 订阅状态与保留行情 | 先检查status，Invalid中旧quote不可当作当前有效行情 |

常见错误：InvalidInput参数非法，Unsupported未验证精度或类型，NotFound无目录/财务/精度，NotConnected会话或有效池不可用，WouldBlock节点冷却/连接预算需求，TimedOut总体时限耗尽，InvalidData响应语义或布局不合法，UnexpectedEof截断，ConnectionAborted分页来源变化。实际error kind以各接口源码为准，不能对所有命令强行统一。[cite:1](#cite-1)[cite:4](#cite-4)

### 1.3 连接、批量与默认预算

| 设置 | 当前默认值 | 含义 |
| --- | ---: | --- |
| TcpConfig.timeout | 5秒 | 连接/读写等待基础上限，仍受总体deadline约束 |
| TcpConfig.auto_reconnect | 0 | 默认不自动重连重试 |
| TcpConfig.retry_delay_ms | 0 | 线性退避基数；不是自动请求频率 |
| TcpConfig.recheck_empty | false | 只在适用查询路径复查空结果；方式随接口不同 |
| Tcp空闲心跳 | 30秒 | 空闲时0004，同一连接帧门控 |
| NetworkPolicy.max_connections | 8 | 默认进程内全部受管连接共享，不是每个Client8个 |
| acquire_timeout | 5秒 | 等待连接许可预算 |
| connect_timeout | 30秒 | 建连/选址等总体预算 |
| request_timeout | 15秒 | 采用此策略的完整请求预算 |
| pagination_timeout | 120秒 | 采用此策略的分页预算 |
| batch_timeout | 300秒 | 批量整体预算 |
| endpoint_cooldown | 10秒 | 失败主站冷却 |
| SharedNetworkPolicy | 默认None | 可显式配置同机共享许可目录；默认不自动限制其他进程 |

策略须在进程首次使用网络预算前通过 `configure_network` 设置一次；已初始化返回AlreadyExists。OperationContext的总体deadline/取消可以进一步缩短所有查询，不会因重试或延迟无限延长。

当日有效IP池是选择主站的前提；显式TcpConfig.ip也须在所选池中通过当日三轮探测。启动接口见 `tcp::startup::initialize/initialize_with_config/initialize_from_env`，IP池具有list/add/remove/update/replace/refresh等操作。默认每日刷新安排与三轮探测不等于证券业务可用性或实盘推送验收。[cite:4](#cite-4)

批量通常先检查参数，再复用同一Client批次锁与有界线程池；多个批次排队，批次内并发。当前Client池每批归还后不保留空闲连接（max_idle=0），避免耗尽其他Client许可。默认8连接下，保留主连接时该批理论worker上界是7，实际仍受其他受管连接/共享许可限制。查询需要至少2连接预算；订阅额外占1个许可。

### 1.4 价格、量、额与日期口径

| 数据 | 当前处理 | 不能作的推断 |
| --- | --- | --- |
| 053e/0537/0feb/054c/0547/054b价格 | 按证券目录decimal换算；当前支持2/3/4位 | 不能所有证券固定/100；无精度不能输出假价格 |
| 052d与052b价格 | 千分元整数还原 | 不能据此给所有原始price整数统一标尺 |
| 0fc5/0fc6/旧0fb5价格 | 兼容/100；保留raw | 不能推广到其他未核实品种或称目录精度已接入 |
| 052b成交量 | 变长整数股数；lots=shares/100 | 不能套052d量槽解码 |
| 052d成交量 | 解码后的源量；周期/证券不同可有股/手差别 | 不可统一乘100；CanonicalBar要求显式VolumeUnit |
| 快照/分时/成交数量 | 常有raw或历史手数注释 | 未核实品种不能把注释当作全品种转换规则 |
| 052d/052b金额 | 当前源码用非负有限IEEE754 binary32 | 052b旧指数算法漏隐含首位，已修复；不能套报价金额算法 |
| 报价金额 | 对应报价命令的协议编码 | 编码正确不自动确认全部外部币种/范围 |
| 0ffc净额/比例 | 净额沿total_amount单位；比例是百分点 | 不可把5.0理解为500%或0..1比例，不可混同交易所精确资金流 |
| 财务字段 | 逐字段单位、Verified/Candidate/Unknown | 不可全表统一×10000或将EPS视作TTM |
| requested_date | 调用者请求日期 | 不是服务器响应日期真实性证明 |
| fetched_at / observed_date | 本地观察/获取时间 | 不是证券成交时间或财报报告期 |
| 0537/0feb time_label | 按分钟索引合成会话标签 | 不是报文里的真实时间戳 |
| 052b date / 052d dt | 解析响应实际日期槽 | 普通日线dt的时间部分是解析约定，不能解释为真实开盘成交时间 |

特别区分：`TradePage` 是原始事件页，`TransactionCollection` 是高层严格网络枚举结果，`TradeCollection` 是已取得页的本地校验聚合。后者不主动联网。`SourceEnumerated` 只描述主站可枚举范围，不等于交易所完整历史。[cite:3](#cite-3)

### 1.5 使用频率说明

各节给出的秒级/日级频率是小关注池的起始建议，**不是通达信官方QPS上限、数据刷新周期或服务承诺**。请求预算须计算冷目录加载、分片、分页、重试和空页复查；全市场不能按“每证券每秒一请求”直接扩张。历史/目录/F10/板块应尽量缓存，盘中只刷新所需末端。[cite:6](#cite-6)


## 2. 26 个命令快速索引

| 命令 | 业务 | 高层入口（完整签名见分节） |
| --- | --- | --- |
| [0004](#cmd-0004) | 心跳与连接保活 | `heartbeat` |
| [000d](#cmd-000d) | 会话握手与客户端初始化 | `new`、`with_config`、`with_quote_metadata`、`reconnect`、`retry` |
| [000f](#cmd-000f) | 除权除息与股本变迁 | `xdxr` |
| [0010](#cmd-0010) | 基础财务快照 | `finance`、`finance_snapshot`、`finance_legacy` |
| [02c5](#cmd-02c5) | 板块文件元数据 | `block_metadata`、`block_metadata_batch` |
| [02cf](#cmd-02cf) | F10 公司资料目录 | `company_info_directory`、`company_info_directory_batch`、`f10_categories` |
| [02d0](#cmd-02d0) | F10 栏目正文与完整资料 | `company_info_chunk`、`company_info_text`、`company_info_text_batch`、`f10`、`f10_with_options` |
| [044d](#cmd-044d) | 完整证券目录与交易精度 | `security_directory_page`、`security_directory`、`stocks`、`stocks_legacy` |
| [044e](#cmd-044e) | 市场证券总数量 | `stock_count` |
| [0452](#cmd-0452) | 服务端特殊涨跌停表 | `special_limits`、`special_limits_all` |
| [051b](#cmd-051b) | 分时副图双序列 | `intraday_aux`、`intraday_aux_batch` |
| [052b](#cmd-052b) | 在市/退市证券不复权整数股数日线 | `daily_bars_range_with_options`、`delisted_bars`、`delisted_bars_scan`、`delisted_bars_all`、`delisted_bars_batch`、`delisted_bars_all_batch` |
| [052d](#cmd-052d) | 普通证券/指数 K 线与本地复权 | `bars`、`index_bars`、`bars_range`、`index_bars_range`、`bars_range_with_options`、`index_bars_range_with_options`、`k`、`k_batch`、`bars_batch`、`k_batch_stream`、`bars_batch_stream`、`k_adjusted`、`reported_turnover` |
| [0537](#cmd-0537) | 当日完整分时与兼容视图 | `minute_series`、`minute`、`minute_legacy` |
| [053e](#cmd-053e) | 五档行情快照 | `quotes`、`quotes_with_metadata` |
| [0547](#cmd-0547) | 游标刷新、主动推送与五档状态 | `subscribe_depth` |
| [054b](#cmd-054b) | 服务端分类行情排行 | `category_quotes`、`category_quotes_all` |
| [054c](#cmd-054c) | 一档基础快照与分片批量 | `snapshots`、`snapshots_batch` |
| [056a](#cmd-056a) | 当日/历史竞价过程 | `auction_series`、`auction_series_all`、`auction_series_batch` |
| [06b9](#cmd-06b9) | 通用资源、统计ZIP与板块文件 | `file_content`、`resource_file`、`resource_stats`、`block`、`block_file` |
| [0fb4](#cmd-0fb4) | 指定日期历史分时兼容查询 | `history_minute` |
| [0fc5](#cmd-0fc5) | 当日原始成交事件与完整枚举 | `trade_page`、`transaction`、`transaction_all` |
| [0fc6](#cmd-0fc6) | 增强历史成交事件 | `history_trade_page`、`history_transaction`、`enhanced_history_transaction`、`history_transaction_all`、`enhanced_history_transaction_all` |
| [0fd1](#cmd-0fd1) | 轻量价格走势图 | `sparkline`、`sparkline_batch` |
| [0feb](#cmd-0feb) | 近期分时日期选择 | `recent_minutes`、`recent_minutes_batch` |
| [0ffc](#cmd-0ffc) | 日资金流向与分档净额 | `money_flow_daily`、`money_flow_daily_batch` |


## 3. 逐命令接口、业务和约束


<a id="cmd-0004"></a>
### 0004：心跳与连接保活

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn heartbeat(&mut self) -> std::io::Result<crate::tcp::HeartbeatAck>;
```

业务含义：验证已建立连接还能收到主站响应，同时取得服务器给出的日期字段。它不查询证券行情，也不证明当前正在交易。[cite:4](#cite-4)

结果 `HeartbeatAck`：`server_date_raw:u32`、`server_date:Option<NaiveDate>`、`reserved_raw:[u8;6]`、`raw_payload:Vec<u8>`。非法或零日期保留原值、日期解释为 None。

使用约束：无证券参数；通过 `client.tcp.set_heartbeat_interval(Some(Duration))` 设置空闲心跳，None 关闭。当前 Tcp 默认空闲间隔 30 秒。后台心跳与业务查询共享帧所有权；心跳失败令连接失效。0547 订阅会关闭该连接的自动心跳，避免抢读推送。手动心跳的频率由使用者控制，建议优先采用空闲保活。[cite:4](#cite-4)


签名依据：[heartbeat](../../crates/rustdx-core/src/tcp/stock/client.rs#L109)。


<a id="cmd-000d"></a>
### 000d：会话握手与客户端初始化

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn new() -> std::io::Result<Self>;

pub fn with_config(config: &TcpConfig) -> std::io::Result<Self>;

pub fn with_quote_metadata(
        config: &TcpConfig,
        metadata: Arc<QuoteMetadataStore>,
    ) -> std::io::Result<Self>;

pub fn reconnect(&mut self) -> std::io::Result<()>;

pub fn retry<T>(
        &mut self,
        f: impl FnMut(&mut Tcp) -> std::io::Result<T>,
        attempts: usize,
    ) -> std::io::Result<T>;
```

业务含义：创建或重建一条可查询行情的主站会话。`Client::new/with_config` 在成功返回前完成标准握手；通常无需手动构造 000d。[cite:1](#cite-1)[cite:4](#cite-4)

结果：`Client` 持有受管 `Tcp`、批次锁/连接池及目录精度缓存；重连后连接标识改变。当前未暴露 eltdx 的完整 HandshakeInfo（主站名称、产品标识、会话时段等）。

使用约束：`with_config` 的显式地址必须属于所选当日有效 IP 池；`with_quote_metadata` 显式共享缓存，默认客户端彼此隔离。`retry(f, attempts)` 是调用级重试，TcpConfig 还可配置传输自动重连；不要将两层重试叠加成无界请求。分页中途重连通常会令整次枚举失败。建议建连/重连时握手一次，复用客户端。[cite:4](#cite-4)


签名依据：[new](../../crates/rustdx-core/src/tcp/stock/client.rs#L48) · [with_config](../../crates/rustdx-core/src/tcp/stock/client.rs#L53) · [with_quote_metadata](../../crates/rustdx-core/src/tcp/stock/client.rs#L73) · [reconnect](../../crates/rustdx-core/src/tcp/stock/client.rs#L104) · [retry](../../crates/rustdx-core/src/tcp/stock/client.rs#L114)。


<a id="cmd-000f"></a>
### 000f：除权除息与股本变迁

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn xdxr(&mut self, market: u16, code: &str) -> std::io::Result<Vec<super::XdxrData>>;
```

业务含义：取得一个证券的权息或股本变动事件，用于复权因子、历史股本分析等。事件日期与报告期不同；返回空表仅表示该主站未返回事件。[cite:5](#cite-5)

结果 `Vec<XdxrData>`：`market:u8`、`code:String`、`date:u32`、`category:u8`，以及四个 f32 复用槽 `fh_qltp / pgj_qzgb / sg_hltp / pg_hzgb`。category=1 时分别表示每十股现金红利、每股配股价、每十股送转比例、每十股配股比例。其他类别下槽位表示前/后流通盘、前/后总股本或特殊比例，不可一律当作分红。

使用约束：高层限制市场 0/1、六位 ASCII 字母数字；每次只发一个证券的请求，当前没有 eltdx 多证券批量报文入口。类别 6/15 等增强语义及逐槽 raw 模型仍存在差距；不应把所有类别按 category=1 做复权。旧 `Xdxr::new` 对代码长度有 panic 边界，高层已先校验。建议每日缓存并在权息公告后刷新，依据日期选择有效事件。[cite:5](#cite-5)


签名依据：[xdxr](../../crates/rustdx-core/src/tcp/stock/client.rs#L701)。


<a id="cmd-0010"></a>
### 0010：基础财务快照

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn finance(&mut self, market: u16, code: &str) -> std::io::Result<FinanceSnapshot>;

pub fn finance_snapshot(
        &mut self,
        market: u16,
        code: &str,
    ) -> std::io::Result<FinanceSnapshot>;

pub fn finance_legacy(&mut self, market: u16, code: &str) -> std::io::Result<FinanceInfoData>;
```

业务含义：取得主站保存的最新基础财务槽和元信息。它不是任意报告期财报查询，也不是完整三张财务报表。[cite:3](#cite-3)

推荐结果 `FinanceSnapshot`，通过 `raw()` 读取 `FinanceRaw`，通过 `field(FinanceField)`、`fields()` 读取 31 个财务槽。元信息包括 `report_period:Option<NaiveDate>`、`semantic_schema_version`、`profile_version`、`eps_basis`、利润/净资产口径、来源和获取时间。字段提供 `raw_bits/raw_value/unit/semantic_status/value/candidate_value/evidence`。

使用约束：`value()` 只给独立核验并命中证券、更新日期、原始位模式的标准业务值；其余候选值为 None。`candidate_value()` 是显式采用基线换算，不能升级为 Verified。候选数量乘 10000、金额乘 1000、每股金额/数量计数乘 1，未知槽不换算；具体字段证据与报告期必须同读。EPS 不自动视作 TTM 或年化。默认独立证据局限于已保存的平安银行样本及已验证字段；其他证券或更新版本不会凭数值接近自动验证。逐槽偏移、候选单位、10个默认证据字段与7个未知槽见[财务字段状态](RESPONSE_FIELD_SEMANTICS.md#finance-fields)。

`finance_snapshot` 是高层同义入口；`finance_legacy` 返回 `FinanceInfoData`，沿用历史名称与单位。无数据时高层返回 NotFound。当前每次单证券，并未实现 eltdx 的真正多证券 0010 报文。建议按日缓存，财报更新后重取；不得将 legacy 财务单位混入标准值。[cite:3](#cite-3)


签名依据：[finance](../../crates/rustdx-core/src/tcp/stock/client.rs#L663) · [finance_snapshot](../../crates/rustdx-core/src/tcp/stock/client.rs#L683) · [finance_legacy](../../crates/rustdx-core/src/tcp/stock/client.rs#L692)。


<a id="cmd-02c5"></a>
### 02c5：板块文件元数据

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn block_metadata(&mut self, filename: &str) -> io::Result<BlockMetadata>;

pub fn block_metadata_batch(
        &self,
        filenames: &[&str],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<BlockMetadata>>>;
```

业务含义：查询板块定义文件的大小与服务器保存的 MD5，用于决定是否需要下载以及需要读取多少字节。元数据本身不返回成分股。[cite:2](#cite-2)

结果 `BlockMetadata`：`size:u32`、`md5:String`、`md5_raw:[u8;32]`、`reserved_raw:u8`、`terminator_raw:u8`、`trailing_payload:Vec<u8>`、来源与完整原始正文。

使用约束：文件名 trim 后 1–40 字节 ASCII、不得包含 NUL；MD5 从正文偏移 5 开始，不是偏移 4。单独查询保留服务器hash，不代表已验算文件。`BlockMetadata::verify_content(&[u8])->io::Result<String>`检查完整长度、原始32位十六进制MD5并返回小写实际值，接受声明大写，不改原字段；`block_file/block`下载自动执行该检查。空字节也必须声明正确MD5。批量以文件为结果单位，保留输入顺序与各文件独立错误；最多 10 万项。建议每日或文件变更时查询，再走06b9。[cite:2](#cite-2)


签名依据：[block_metadata](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L10) · [block_metadata_batch](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L13)。


<a id="cmd-02cf"></a>
### 02cf：F10 公司资料目录

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn company_info_directory(
        &mut self,
        market: u16,
        code: &str,
    ) -> io::Result<CompanyInfoDirectory>;

pub fn company_info_directory_batch(
        &self,
        stocks: &[(u16, &str)],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<CompanyInfoDirectory>>>;

pub fn f10_categories(
        &mut self,
        market: u16,
        code: &str,
    ) -> std::io::Result<Vec<CompanyInfoCategoryItem>>;
```

业务含义：列出证券的 F10 栏目，给出栏目名、服务器文件名、正文开始偏移和字节长度。常见公司概况/财务/经营栏目属于文本资料，不等于结构化财务数据。[cite:2](#cite-2)

完整结果 `CompanyInfoDirectory` 包含 request、entries、trailing_payload、source、raw_payload。每个 `CompanyInfoEntry` 的全部业务字段是 `name:String / filename:String / start:u32 / length:u32`，并保留 64 字节 name_raw、80 字节 filename_raw。`f10_categories` 返回相同业务字段的兼容 `Vec<CompanyInfoCategoryItem>`。

使用约束：显式市场 0/1/2，代码 trim 后六位 ASCII 数字；市场 2 的参数支持不等于已完成网络验收。每条响应固定 152 字节，文件名是 GBK，不能按 UTF-8 字节原样重发。start/length 是文件字节范围，并非行号/字符数。建议每次完整刷新 F10 时先取目录；空目录不证明公司无资料。[cite:2](#cite-2)


签名依据：[company_info_directory](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L30) · [company_info_directory_batch](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L37) · [f10_categories](../../crates/rustdx-core/src/tcp/stock/client.rs#L792)。


<a id="cmd-02d0"></a>
### 02d0：F10 栏目正文与完整资料

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn company_info_chunk(
        &mut self,
        request: &CompanyInfoTextRequest,
    ) -> io::Result<CompanyInfoTextChunk>;

pub fn company_info_text(
        &mut self,
        request: CompanyInfoTextRequest,
        options: CompanyInfoReadOptions,
    ) -> io::Result<CompanyInfoText>;

pub fn company_info_text_batch(
        &self,
        requests: &[CompanyInfoTextRequest],
        options: CompanyInfoReadOptions,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<CompanyInfoText>>>;

pub fn f10(&mut self, market: u16, code: &str) -> std::io::Result<Vec<(String, String)>>;

pub fn f10_with_options(
        &mut self,
        market: u16,
        code: &str,
        options: CompanyInfoReadOptions,
    ) -> io::Result<Vec<(String, String)>>;
```

业务含义：按目录指定的文件与字节范围读取栏目文本。`company_info_chunk` 发一帧，`company_info_text` 读取一个完整范围，`f10/f10_with_options` 返回该证券全部 `(栏目名, 文本)`。[cite:2](#cite-2)

请求：`CompanyInfoTextRequest::new(market, code, filename, offset, length)`。单块结果 `CompanyInfoTextChunk` 保存 request、10 字节 header_raw、declared_length:u16、content_raw、trailing_payload、source 与 raw_payload；完整结果 `CompanyInfoText` 保存 request、text:String、拼齐的 content_raw、chunks 和 complete。

使用约束：市场/代码同 02cf；文件名 trim 后须可无损编码 GBK、编码长度 1–80 字节、无 NUL；length>0，offset+length 不得溢出 u32。单帧 length 可表达大范围，但不能保证一帧返回全部。完整读取默认 30000 字节/块，上限 60000；默认最多 64 MiB、4096 块，时限继承分页策略。

按实际返回字节推进，短块继续；达到目录长度前返回空块、GBK 无效、连接/主站/北京时间观察日变化、预算耗尽均报错。先拼字节再解码，避免中文双字节被分块切开；正文换行和 NUL 保留。`f10_with_options` 的全部栏目共用总字节/总块预算和时限，零长度栏目保留空串。批量按请求范围返回独立结果。建议按日或资料变更刷新并缓存正文。[cite:2](#cite-2)


签名依据：[company_info_chunk](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L54) · [company_info_text](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L60) · [company_info_text_batch](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L68) · [f10](../../crates/rustdx-core/src/tcp/stock/client.rs#L786) · [f10_with_options](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L87)。


<a id="cmd-044d"></a>
### 044d：完整证券目录与交易精度

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn security_directory_page(
        &mut self,
        market: u16,
        start: u32,
        count: u16,
    ) -> std::io::Result<super::SecurityDirectoryPage>;

pub fn security_directory(
        &mut self,
        market: u16,
    ) -> std::io::Result<Vec<super::SecurityDirectoryData>>;

pub fn stocks(&mut self, market: u16) -> crate::Result<Vec<SecurityListData>>;

pub fn stocks_legacy(&mut self, market: u16) -> crate::Result<Vec<SecurityListData>>;
```

业务含义：查询市场的代码、名称、价格精度、交易单位等证券元数据，是快照/分时正确换算价格的基础。一个市场目录包含指数、股票、基金、债券等，不等于 A 股名单。[cite:1](#cite-1)

单页 `SecurityDirectoryPage`：market、start:u32、request_count:u16、records、raw_payload、source_endpoint。记录 `SecurityDirectoryData`：market、code、完整 16 字节 GBK 名称、volume_unit:u16、decimal:u8、previous_close:f32、volume_ratio_base:f32、unknown0_raw/previous_close_raw/unknown3_raw，以及 category/board。

使用约束：市场仅 0/1，每页 count=1–1600，start 为 u32；高层完整目录先读 044e 总数，按实际条数推进，拒绝重复代码、提前空页、跨连接/跨日。`stocks` 返回映射后的兼容 `SecurityListData` 并更新精度缓存；`security_directory` 返回完整记录但不单独发布到 Client 缓存。需要缓存时使用 `refresh_quote_metadata` 或 `stocks`。

category/board 是**代码前缀提示**，不是响应中的权威分类字段。Star/Chinext/Main 等分类不能作为上市状态或规则依据，Unknown 保留不认识的代码。decimal 当前可归一化范围为 2/3/4，其余仍可保留在目录中但行情解码可能 Unsupported。`stocks_legacy` 明确发送 0450（不属于本 26 项），只有旧 29 字节记录与 1000 条分页。建议开盘前加载完整目录，并在目录变更或跨日时更新。[cite:1](#cite-1)


签名依据：[security_directory_page](../../crates/rustdx-core/src/tcp/stock/client.rs#L739) · [security_directory](../../crates/rustdx-core/src/tcp/stock/client.rs#L732) · [stocks](../../crates/rustdx-core/src/tcp/stock/client.rs#L753) · [stocks_legacy](../../crates/rustdx-core/src/tcp/stock/client.rs#L748)。


<a id="cmd-044e"></a>
### 044e：市场证券总数量

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn stock_count(&mut self, market: u16) -> std::io::Result<u16>;
```

业务含义：取得主站当前市场代码表的总条数，可用于完整目录读取的预期条数校验。它不是股票数、A 股数或上市公司数。[cite:1](#cite-1)[cite:5](#cite-5)

结果为 `u16`。高层市场限制 0/1；底层是 `SecurityCount::new(market)` 加 Tdx 接收。建议启动/目录刷新时查询一次；同一天结果也可能因主站名单更新变化，不能长期当作恒定总量。


签名依据：[stock_count](../../crates/rustdx-core/src/tcp/stock/client.rs#L723)。


<a id="cmd-0452"></a>
### 0452：服务端特殊涨跌停表

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn special_limits(&mut self, start: u16) -> io::Result<SpecialLimitPage>;

pub fn special_limits_all(
        &mut self,
        options: CommandScanOptions,
    ) -> io::Result<SpecialLimitCollection>;
```

业务含义：查询主站列出的特殊上下限价条目，用于补充本地涨跌停规则。它是例外表，不能当作所有证券的完整限制清单。[cite:1](#cite-1)

`SpecialLimitPage = CommandPage<SpecialLimitRecord>`；每条返回 market_raw:u8、code_number_raw:u32、格式化 code:String、upper_price/lower_price:f32 和原始浮点位模式。`special_limits(start)` 的 start:u16 是表内条目偏移，没有 count 参数；`special_limits_all` 使用 CommandScanOptions。

使用约束：扫描按实际条数推进，短页也继续到空页；预算/偏移结束交付 complete=false，重复记录、跨来源报错。表中没有某证券不表示无涨跌停，也不证明适用哪套交易所规则。建议开盘前缓存，需要变更复核时刷新。[cite:1](#cite-1)


签名依据：[special_limits](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L25) · [special_limits_all](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L28)。


<a id="cmd-051b"></a>
### 051b：分时副图双序列

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn intraday_aux(
        &mut self,
        market: u16,
        code: &str,
        kind: IntradayAuxKind,
    ) -> io::Result<IntradayAuxSeries>;

pub fn intraday_aux_batch(
        &self,
        stocks: &[(u16, &str)],
        kind: IntradayAuxKind,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<IntradayAuxSeries>>>;
```

业务含义：取得分时副图数据。`IntradayAuxKind::BuySellStrength` 编码 selector=0，用于主站委买委卖力道类双序列；`VolumeComparison` 编码 11，为前日/当日量比较双序列；`Raw(u8)` 可表达未命名选择器。[cite:1](#cite-1)[cite:6](#cite-6)

结果 `IntradayAuxSeries.page.records` 是 `IntradayAuxPoint::IntegerPair {index,series_a:i64,series_b:i64}`，或 `VolumePair {index,previous_day:f32,current_day:f32,previous_bits,current_bits}`。

使用约束：市场0/1、六位 ASCII 字母数字；无 start/count/date 参数，无分页或日期回显，仅序列索引；不能给每个点虚构服务器时间。只有 selector=11 按浮点布局，其他选择器走整数布局，但格式可解析不等于指标语义已验证。量比较单位保留为主站原单位。建议关注证券每 3–5 秒起步，按实际响应负载调整。[cite:1](#cite-1)


签名依据：[intraday_aux](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L34) · [intraday_aux_batch](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L190)。


<a id="cmd-052b"></a>
### 052b：不复权整数股数日线

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn daily_bars_range_with_options(
        &mut self,
        market: u16,
        code: &str,
        begin: Option<u32>,
        end: Option<u32>,
        options: BarRangeOptions,
    ) -> io::Result<BarRangeCollection<DailyBar>>;

pub fn delisted_bars(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
    ) -> io::Result<DelistedBarsPage>;

pub fn delisted_bars_scan(
        &mut self,
        request: DelistedBarsRequest,
        options: CommandScanOptions,
    ) -> io::Result<DelistedBarsCollection>;

pub fn delisted_bars_all(
        &mut self,
        market: u16,
        code: &str,
        options: CommandScanOptions,
    ) -> io::Result<DelistedBarsCollection>;

pub fn delisted_bars_batch(
        &self,
        requests: &[DelistedBarsRequest],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<DelistedBarsPage>>>;

pub fn delisted_bars_all_batch(
        &self,
        stocks: &[(u16, &str)],
        options: CommandScanOptions,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<DelistedBarsCollection>>>;
```

业务含义：读取主站不复权日 OHLC、整数成交股数和金额。旧SDK名称是退市日线，但两实际节点同时支持所列在市A股。线协议没有category或日期过滤参数；新range入口在客户端按begin/end过滤并保留分页状态，不查询分钟线或进行复权。[cite:2](#cite-2) 在市范围依据见[24条新样本核对](../testing/evidence/tcp-day-volume-fix-20261024/independent.json)。

单页 `DelistedBarsPage` 保存 request、records、source、raw_payload；完整 `DelistedBarsCollection` 保存原始 pages、按日期升序合并的 records、raw_record_count、termination、complete、atomic=false。每条 `DelistedBar` 保存 date_raw/date/time、open/close/high/low、四个 price_milli:i64、四个 delta_raw:i64、last_close_price_milli、volume_raw/volume_shares:u32、volume_lots:f64、amount_raw:u32、amount:f64、unknown_suffix_raw:[u8;4]、record_raw。

使用约束：单页/旧扫描支持市场0/1/2、trim 后六位数字；start:u16，count=1–800。range入口支持市场0/1及规范六位数字，市场2未验收。价格千分元、日日期为 UTC+8 零时；成交量是变长整数**股数**，手数=股数/100；金额按非负有限IEEE754 binary32解码，旧TDX指数算法漏掉隐含首位已修复。`DailyBar/DailyBarsRequest/DailyBarsPage/decode_daily_bars`保留通用命名和旧名称兼容。

range入口复用`BarRangeOptions`，从start=0按raw数量推进，短页继续；空页按配置复查，越过begin或观察到空页才`complete=true`。页预算/行数/字节/偏移终止返回不完整状态；跨连接/观察日期、冲突或不推进报错。`BarRangeCollection.command=0x052b`，category=9只是日线分类，报文没有category槽。保存全部页范围与收据，再检查业务日期是否缺失；完整枚举不证明交易所全部历史。

完整扫描按实际条数前进，短页继续直到空页；相同日期且 OHLC/量/额/尾部相同的重叠合并，原始页保留；同日冲突或整页无新增日期报错。默认预算同 CommandScanOptions，预算/偏移结束 complete=false。`delisted_bars_scan` 可从自选 start 开始，complete=true 只说明该起点到主站空页已枚举；从0开始也不证明交易所历史完整。建议首次全量建档，之后低频按需补取；三证券沪深样本已通过两节点独立核验，市场2未实测。[cite:2](#cite-2)[cite:7](#cite-7)


签名依据：[delisted_bars](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L134) · [delisted_bars_scan](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L143) · [delisted_bars_all](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L150) · [delisted_bars_batch](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L161) · [delisted_bars_all_batch](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs#L173)。


<a id="cmd-052d"></a>
### 052d：普通证券/指数 K 线与本地复权

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn bars<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        start: u16,
        count: u16,
    ) -> std::io::Result<Vec<super::KlineData<'a>>>;

pub fn index_bars<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        start: u16,
        count: u16,
    ) -> std::io::Result<Vec<super::IndexKlineData<'a>>>;

pub fn bars_range<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::KlineData<'a>>>;

pub fn index_bars_range<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        category: u16,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::IndexKlineData<'a>>>;

pub fn k<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::KlineData<'a>>>;

pub fn k_batch<'a>(
        &self,
        stocks: &[(u16, &'a str)],
        begin: Option<u32>,
        end: Option<u32>,
        max_parallel: usize,
    ) -> std::io::Result<Vec<BatchKline<'a>>>;

pub fn bars_batch<'a>(
        &self,
        stocks: &[(u16, &'a str)],
        category: u16,
        begin: Option<u32>,
        end: Option<u32>,
        max_parallel: usize,
    ) -> std::io::Result<Vec<BatchBars<'a>>>;

pub fn bars_range_with_options<'a>(
        &mut self, market: u16, code: &'a str, category: u16,
        begin: Option<u32>, end: Option<u32>, options: BarRangeOptions,
    ) -> std::io::Result<BarRangeCollection<KlineData<'a>>>;

pub fn index_bars_range_with_options<'a>(
        &mut self, market: u16, code: &'a str, category: u16,
        begin: Option<u32>, end: Option<u32>, options: BarRangeOptions,
    ) -> std::io::Result<BarRangeCollection<IndexKlineData<'a>>>;

pub fn bars_batch_stream<'a>(
        &self, stocks: &[(u16, &'a str)], category: u16,
        begin: Option<u32>, end: Option<u32>, options: BarBatchStreamOptions,
        consume: impl FnMut(BatchBarRange<'a>) -> std::io::Result<()>,
    ) -> std::io::Result<BatchStreamSummary>;

pub fn k_batch_stream<'a>(
        &self, stocks: &[(u16, &'a str)],
        begin: Option<u32>, end: Option<u32>, options: BarBatchStreamOptions,
        consume: impl FnMut(BatchBarRange<'a>) -> std::io::Result<()>,
    ) -> std::io::Result<BatchStreamSummary>;

pub fn k_adjusted<'a>(
        &mut self,
        market: u16,
        code: &'a str,
        adj: Adj,
        begin: Option<u32>,
        end: Option<u32>,
    ) -> std::io::Result<Vec<super::KlineData<'a>>>;

pub fn reported_turnover(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        scope: TurnoverScope,
        options: TurnoverOptions,
    ) -> io::Result<TurnoverOutcome>;
```

业务含义：单页或日期区间的 OHLCV、源成交额；指数解码另带上涨/下跌家数。本地 `k_adjusted` 组合不复权日线和 000f 权息信息调整价格。[cite:1](#cite-1)[cite:3](#cite-3)

普通结果 `KlineData<'a>`：dt、借用 code、open/close/high/low:f64、vol/amount:f64；指数 `IndexKlineData<'a>` 增加 up_count/down_count:u16。`k_batch` 每证券 `BatchKline<'a>`，`bars_batch` 每证券 `BatchBars<'a>`，内含独立 result。需要明确单位的标准模型用 `KlineRequest::recv_canonical_stock/index(tcp, VolumeUnit)`，其provenance保存原始请求、解码类型、量/单位依据和网络获取来源。

使用约束：市场0/1、六位 ASCII 字母数字，start:u16，count=1–800，category=0..11。当前周期见公共参数表，**4 与 9 都能表达日线，量单位可能不同**。区间 begin/end 为闭区间 YYYYMMDD，None 不设界；旧Vec按短页停止，返回Vec不足以证明完整历史。新状态入口短页默认按raw数继续，检查complete/termination/coverage；原始/实际窗口和每页请求、重复数量均保留。相同时间戳内容冲突、来源变化、非法日期或不推进均报Err，同连接复查空页。

`BarRangeOptions`默认page_size800、max_pages1024、max_rows65536、max_bytes64MiB、timeout120秒、stop_on_short_page=false。分页/行/字节/偏移预算耗尽返回不完整集合；收到但字节预算拒收的页accepted=false，raw行/字节仍计量。完成仅表示本来源观察到空页或越过begin，不证明停牌/缺页、交易所全历史或盘中冻结快照。

`BarBatchStreamOptions { range, delivery }`中的delivery为 `BatchStreamOptions`，默认总100万行/256MiB JSON、每项65536行/32MiB JSON、300秒。按输入顺序逐证券回调，每项结果包含区间状态；只预取最多并行数个项目。慢消费者等待计入同一deadline，回调错误/总量耗尽终止，先前已交付内容由应用标记保存；JSON计量不等于RSS。扩展命令亦提供 `*_batch_stream`，完整签名见[流式命令源码](../../crates/rustdx-core/src/tcp/stock/client/stream_queries.rs)，消费约定统一见[应用接入](../development/APPLICATION_INTEGRATION.md)。

价格按 /1000 解码；vol 仍为源值，不能一律乘100。当前源码052d与052b金额槽均按非负有限IEEE754 binary32解码；其他行情金额不能套用。052d category9样本整数手数已丢股尾数，category4的binary32股量也可能量化；需要精确股数时使用已核实范围的052b日线入口。股票/指数不能共用错误的响应解码器。`k_adjusted` 仅支持本地日线 Adj::Qfq/Hfq，价格调整、量额不调整；后复权基点为主站可得历史首日，未覆盖全部服务器/N周期/定点复权合同。

`reported_turnover` 是该命令上的源成交额聚合；没有合适 TurnoverProfile 时保留原始条但业务合计可能 Unavailable，AfterHours 无独立源，Combined 不伪造盘后额。它不等于从明细价×量计算的估算额。建议历史回补一次建档，盘中只刷新末端，按周期结束更新；详见补充处理接口。[cite:3](#cite-3)


签名依据：[bars](../../crates/rustdx-core/src/tcp/stock/client.rs#L171) · [index_bars](../../crates/rustdx-core/src/tcp/stock/client.rs#L223) · [bars_range](../../crates/rustdx-core/src/tcp/stock/client.rs#L205) · [index_bars_range](../../crates/rustdx-core/src/tcp/stock/client.rs#L250) · [k](../../crates/rustdx-core/src/tcp/stock/client.rs#L272) · [k_batch](../../crates/rustdx-core/src/tcp/stock/client.rs#L315) · [bars_batch](../../crates/rustdx-core/src/tcp/stock/client.rs#L348) · [k_adjusted](../../crates/rustdx-core/src/tcp/stock/client.rs#L389) · [reported_turnover](../../crates/rustdx-core/src/tcp/stock/client/turnover.rs#L15)。


<a id="cmd-0537"></a>
### 0537：当日完整分时与兼容视图

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn minute_series(
        &mut self,
        market: u16,
        code: &str,
    ) -> std::io::Result<super::TodayMinuteSeries>;

pub fn minute(&mut self, market: u16, code: &str) -> std::io::Result<Vec<MinuteTimeData>>;

pub fn minute_legacy(
        &mut self,
        market: u16,
        code: &str,
    ) -> std::io::Result<Vec<MinuteTimeData>>;
```

业务含义：取得当日分时线的价格、均价和分钟原始量。`minute_series` 返回完整字段；`minute` 映射成 price/vol 兼容视图。[cite:1](#cite-1)

`TodayMinuteSeries`：market、code、trading_date:Option<u32>、points、reserved_raw、scale、metadata_generation、raw_payload、source_endpoint、fetched_at。每点保存 index、time_label、price、average_price、volume_raw:i32、两种原始变长字段、累计价格整数和 byte_range。

使用约束：市场0/1、六位 ASCII 字母数字，最多240点，没有用户 count/start/date。时间标签按主板会话合成为09:31–11:30、13:01–15:00；响应没有日期/时间回显，trading_date=None。价格按证券目录精度，均价多两位；量不隐式换算股数。首次自动加载当日目录，可用 refresh_quote_metadata 预热。`minute_legacy` 显式使用051d，必要时回退今日0fb4，属于兼容路径。建议每3–5秒起步；休市空值不直接判定停牌。[cite:1](#cite-1)


签名依据：[minute_series](../../crates/rustdx-core/src/tcp/stock/client.rs#L461) · [minute](../../crates/rustdx-core/src/tcp/stock/client.rs#L447) · [minute_legacy](../../crates/rustdx-core/src/tcp/stock/client.rs#L478)。


<a id="cmd-053e"></a>
### 053e：五档行情快照

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn quotes(&mut self, stocks: &[(u16, &str)]) -> std::io::Result<Vec<QuoteData>>;

pub fn quotes_with_metadata(&mut self, stocks: &[(u16, &str)]) -> std::io::Result<QuotePage>;
```

业务含义：一次取得少量证券的现价、OHLC、累计量额、内外盘、五档买卖价量、涨跌/涨速等，是点时快照而非逐笔事件流。[cite:1](#cite-1)

`quotes` 返回 `Vec<QuoteData>`；其全部业务字段为 code、price、last_close、open、high、low、vol、cur_vol、amount、s_vol、b_vol、change、change_percent、speed、servertime、bid1..5/ask1..5、对应 bid/ask 量。`quotes_with_metadata` 返回 `QuotePage`，另外保留 requested/returned/missing_codes（含市场）、逐证券 QuotePrecision、metadata_generation、fetched_at、source_endpoint。

使用约束：单次1–80个不同 (market,code)，市场0/1、六位ASCII字母数字；`quotes([])` 返回空Vec，而完整页/请求构造器拒绝空列表。高层没有 quotes_batch，需调用者按80拆分或采用有分片入口的054c。结果市场识别应按 QuotePage returned_codes，不能仅用无市场的 code 合并沪深同码证券，也不要假定服务器按输入序返回。

价格必须使用目录decimal=2/3/4；旧 QuotesRequest.recv_legacy 明确恢复历史/100行为。QuoteData注释量单位写手，但对非已验证品种仍不能自动推广；金额为行情命令的协议编码。`recheck_empty` 可等300ms并重连重取一次，会增加请求数；休市/缺失证券不等于交易状态。建议小关注池1–3秒起步，完整市场按实际分片数控预算。[cite:1](#cite-1)


签名依据：[quotes](../../crates/rustdx-core/src/tcp/stock/client.rs#L130) · [quotes_with_metadata](../../crates/rustdx-core/src/tcp/stock/client.rs#L139)。


<a id="cmd-0547"></a>
### 0547：游标刷新、主动推送与五档状态

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn subscribe_depth(
        &mut self,
        stocks: &[(u16, &str)],
        options: SubscriptionOptions,
    ) -> io::Result<QuoteSubscription>;
```

业务含义：以证券更新游标保持五档状态，接收更新响应和服务器未配对的主动推送。更新记录代表该证券完整状态，用来替换；累计量额不能再次累加。[cite:1](#cite-1)[cite:8](#cite-8)

`subscribe_depth` 返回独占连接的 `QuoteSubscription`。会话方法完整列于下方；`refresh()` 返回 `QuoteRefresh`，`poll_push(timeout)` 返回 `Option<QuotePush>`，`snapshot()` 返回 `DepthSnapshot`，含 status、connection_id、generation、states、queued_frames/bytes。每个 DepthState 包含 symbol、quote:Option<BasicQuote>、cursor:u32、received_at。

使用约束：1–100个不同证券，市场0/1，当日精度目录必须可用。订阅额外占用一个连接许可，保留原 Client 查询连接，关闭订阅连接后台心跳。队列默认且最大256帧/8MiB，refresh_timeout默认15秒（可被策略/外层期限缩短）；容量计入原始体、解码记录及暂存，不只计wire大小。

先建立全证券基线再使用 Synchronized 状态；AwaitingBaseline 可能只有部分证券，Invalid 中残留quote仅供诊断。poll超时返回None不是证明无更新；本地poll可保留半帧。断线、队列溢出、游标回退、目录跨日等令会话失效，不静默丢推送；rebuild 使用新目录重建连接/代次/基线，close/Drop释放连接。

不要把订阅 Tcp 交回查询池。`RefreshRequest::recv` 消耗传入 Tcp 做一次刷新，重复使用宜走会话。建议持续轮询poll，按业务需求低频主动refresh，而不是每证券新建订阅。同步传输/状态测试已具备；**真实盘中主动推送仍待有效交易窗口验收**。[cite:8](#cite-8)


签名依据：[subscribe_depth](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L142)。


会话实际签名（`impl QuoteSubscription`）：

```rust

pub fn with_config(
        config: &TcpConfig,
        stocks: &[(u16, &str)],
        metadata: Arc<PrecisionSnapshot>,
        options: SubscriptionOptions,
    ) -> io::Result<Self>;


pub fn from_tcp(
        mut tcp: Tcp,
        request: RefreshRequest,
        metadata: Arc<PrecisionSnapshot>,
        options: SubscriptionOptions,
    ) -> io::Result<Self>;


pub fn set_push_observer(&mut self, observer: Option<Arc<dyn PushObserver>>);


pub fn snapshot(&self) -> DepthSnapshot;


pub fn refresh(&mut self) -> io::Result<QuoteRefresh>;


pub fn poll_push(&mut self, timeout: Duration) -> io::Result<Option<QuotePush>>;


pub fn rebuild(&mut self, metadata: Arc<PrecisionSnapshot>) -> io::Result<QuoteRefresh>;


pub fn close(&mut self);


```


<a id="cmd-054b"></a>
### 054b：服务端分类行情排行

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn category_quotes(
        &mut self,
        category: QuoteCategory,
        sort: QuoteSortField,
        options: CategoryQuoteOptions,
    ) -> io::Result<CategoryQuotePage>;

pub fn category_quotes_all(
        &mut self,
        category: QuoteCategory,
        sort: QuoteSortField,
        page: CategoryQuoteOptions,
        scan: CommandScanOptions,
    ) -> io::Result<CategoryQuoteCollection>;
```

业务含义：由服务器按分类和排序字段返回行情排行，适合涨幅、成交额等榜单；返回期间排名可能变化，不是冻结全市场截面。[cite:1](#cite-1)

分类 `QuoteCategory::AShares=6 / Raw(u16)`；排序 `QuoteSortField::{Code=0,Price=6,Amount=10,ChangePercent=14,OrderAmount=28,Raw}`。`CategoryQuotePage` 含 request、header_raw、page，每条 `CategoryQuote` 含 BasicQuote 一档、status_or_sort_raw、rise_speed/raw、short_turnover/raw、two_minute_amount、opening_rush/raw、volume_rise_speed、depth_raw 和额外原始元数据。

使用约束：CategoryQuoteOptions默认start=0、count=80、ascending=false、sort_reverse_raw=None、filter_raw=0；count=1–700为客户端单帧预算。非代码排序默认降序reverse=1/升序=2，代码排序默认reverse=0，显式raw覆盖不保证业务语义。自动预热沪深目录；榜单可能包含不在支持精度目录中的证券，此时价格/scale为None，保留raw，不擅自/100。

扫描短页/空页结束，重复证券（动态排行重叠）报错，预算终止complete=false，atomic=false。建议榜首每3–5秒起步，完整扫描盘后使用并保存来源时间；不能拼成同一时刻的静态全市场排行。[cite:1](#cite-1)


签名依据：[category_quotes](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L110) · [category_quotes_all](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L124)。


<a id="cmd-054c"></a>
### 054c：一档基础快照与分片批量

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn snapshots(&mut self, stocks: &[(u16, &str)]) -> io::Result<SnapshotPage>;

pub fn snapshots_batch(
        &mut self,
        stocks: &[(u16, &str)],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<SnapshotPage>>>;
```

业务含义：取得证券基础行情和买一/卖一，数据比五档轻，适合关注池和分片市场扫描。[cite:1](#cite-1)

结果 `SnapshotPage` 包含 request、header_raw、missing_codes、page:CommandPage<BasicQuote>。BasicQuote保存market_raw/code、可选现价/昨收/OHLC、价格原始整数、时间或游标、原始状态/量/额/内外盘、一档bids/asks、scale和tail_raw。time_raw未自动转为带日期成交时间戳，未命名状态/尾槽的解释见[行情字段状态](RESPONSE_FIELD_SEMANTICS.md#market-fields)。

使用约束：单帧1–80个不重复证券、市场0/1；数量80是客户端策略，不是实测服务器硬上限。`snapshots_batch` 对最多10万输入按80分片，**每个结果对应一个分片，不是一只证券**；重复验证发生在每个请求分片，跨分片重复输入未做统一禁止。低层要求正确且新鲜的精度，高层先加载目录。返回缺失证券保留missing_codes，不给虚构零快照。建议小池1–3秒，全市场按实际分片数规划周期。[cite:1](#cite-1)


签名依据：[snapshots](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L95) · [snapshots_batch](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L163)。


<a id="cmd-056a"></a>
### 056a：当日/历史竞价过程

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn auction_series(
        &mut self,
        market: u16,
        code: &str,
        date: Option<u32>,
        options: AuctionOptions,
    ) -> io::Result<AuctionSeries>;

pub fn auction_series_all(
        &mut self,
        market: u16,
        code: &str,
        date: Option<u32>,
        page: AuctionOptions,
        scan: CommandScanOptions,
    ) -> io::Result<AuctionCollection>;

pub fn auction_series_batch(
        &self,
        stocks: &[(u16, &str)],
        date: Option<u32>,
        options: AuctionOptions,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<AuctionSeries>>>;
```

业务含义：取得竞价过程的虚拟匹配价、匹配量与未匹配量快照，包括09:25点；这些记录不是等同于正式开盘成交的逐笔记录。[cite:1](#cite-1)[cite:9](#cite-9)

请求date:Option<u32>，None编码0为当日模式，Some有效YYYYMMDD选择历史。AuctionOptions默认mode=3/start=0/count=500。`AuctionSeries` 包含 request、requested_date、server_trading_date=None、process_snapshots=true、page。每个 AuctionPoint 保存 index、minute_raw、second_raw、time_label、price/price_bits、matched_volume_raw、unmatched_signed_raw:i32、unmatched_volume_raw:u32、unmatched_sign:i8 和 reserved_raw。

使用约束：市场0/1、六位ASCII字母数字；start/count为u32，count=1–4095且start+count不溢出，是本地单帧预算。符号正/负/零可以可靠保留，当前DTO不将其直接升级为已由交易所确认的买/卖侧。匹配/未匹配数量单位保留raw；历史日期请求不等于响应日期回显。扫描按短/空页结束，重复完整页报错，预算返回complete=false。原始mode可表达不认识的模式，但未验收语义。

建议竞价窗口1–2秒起步、盘后低频归档；主站未返回不证明没有竞价。当前日期选择/符号有真实证据，精确侧别和成交对应仍按专项验收边界判断。[cite:9](#cite-9)


签名依据：[auction_series](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L69) · [auction_series_all](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L78) · [auction_series_batch](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L271)。


<a id="cmd-06b9"></a>
### 06b9：通用资源、统计ZIP与板块成分

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn block(&mut self, block_file: &str) -> crate::Result<Vec<BlockRecord>>;

pub fn block_file(&mut self, block_file: &str) -> io::Result<BlockFile>;

pub fn file_content(&mut self, path: &str, offset: u32, size: u32) -> io::Result<FileContentChunk>;

pub fn resource_file(&mut self, path: &str, options: ResourceReadOptions) -> io::Result<ResourceFile>;

pub fn resource_stats(&mut self, path: &str, options: ResourceReadOptions) -> io::Result<TdxStatsResource>;
```

业务含义：下载服务器保存的板块文件，解析概念/指数/风格等板块的成分代码。板块文件属于主站资料，可能更新延迟，不是交易所权威行业分类。[cite:2](#cite-2)[cite:5](#cite-5)

高层 `block(file)` 或自由函数 `get_block_info(tcp,file)` 返回 `Vec<BlockRecord>`，字段为 blockname:String、block_type:u16、code:String。先02c5取得size/MD5，再每30000字节下载、验算完整文件并严格解析。`block_file(file)`或`download_block_file(tcp,file)->io::Result<BlockFile>`保留filename、metadata、content、computed_md5和chunks；每块保留start、length、header_raw:[u8;4]和source。`content`与元数据raw_payload不进入serde JSON，归档需显式保存。低层 `BlockInfoChunk::new(file,start:u32,size:u32)` 仍是旧单块下载对象，`data/response`保留数据，不单独证明全文件完整性。

使用约束：板块高层file先通过02c5的40字节ASCII校验，文件最多64MiB；所有块必须精确等于期望长度，并保持同端点、同连接、同北京时间观察日，共享pagination_timeout总预算。完整长度与MD5不一致报InvalidData；短块报UnexpectedEof，size=0只有空文件MD5匹配才成功。常见block_gn.dat、block_zs.dat、block_fg.dat、block.dat。板块兼容请求仍为旧100字节名称字段，旧构造器会静默截断。

通用入口采用独立的300字节路径布局：trim后须1–300字节ASCII、不含NUL，反斜线规范成`/`；offset/size为u32，size须1–60000且范围不溢出。`FileContentChunk`保留request、chunk_len、header_raw、content、trailing_payload、source及raw_payload。前4字节在此协议配置中为声明块长；超过请求或截断返回Err，超出声明长度的尾部保留且不拼入文件。这不改变旧板块头部的未知状态。

`ResourceReadOptions`默认每块30000字节、文件64MiB、最多4096块、累计解码响应128MiB、pagination_timeout总预算、20ms页间隔；以上容量也是可配置上限。每次读取前为最坏单帧65535字节预留响应预算，无法预留则返回`ResponseBudget`。`ResourceFile`保留content、每块来源/原始响应和未知尾部、response_bytes、termination、complete。仅`ShortChunk/EmptyChunk`置complete=true；`ByteBudget/ChunkBudget/ResponseBudget`交付标记为不完整的前缀，`require_complete()`会拒绝。恰好填满文件字节预算也不证明末尾，需更大预算读取终止块。所有块要求同端点、同连接、同北京时间观察日；传输/解码/来源异常返回Err。通用完整性不带02c5声明MD5，也不证明资源在下载期间没有更新。

`resource_stats`要求完整下载后解析已知`zhb.zip`：ZIP最多32MiB、128成员、每成员32MiB、累计解压64MiB；拒绝加密、多卷/ZIP64、中央目录重复名称/数量不符、缺少`tdxstat.cfg/tdxstat2.cfg`、必需CFG的CRC/GBK错误及重复市场代码键。每CFG最多100000行、单行16384 UTF-8字节；未识别非空行保留在unparsed_lines。`StatsRow`保留market_id_raw、code、行号、fields_raw及candidate_values，日期为stats_date_raw文本；所有命名槽和单位都是eltdx3.3.0配置的候选解释，空/非法/非有限数值为None，整数槽不截断小数或饱和溢出。`TdxStatsResource`同时保留下载文件和archive，原始字节须另行保存，不进入serde JSON。`market_id_raw`不能自动套行情市场枚举。

高层最终使用`decode_block_content(&[u8])->io::Result<Vec<BlockRecord>>`，也可通过`BlockFile::decode_records()`调用：空文件或386字节零板块头返回空；其他文件须满足固定槽位整除、最少13字节槽位头、成分数不越槽、名称合法GBK和代码为6位ASCII数字，失败不返回部分成分。旧`parse_block_content`仍保留容错兼容行为。06b9正文前4字节、代码第7字节及未用槽位字节不推断业务含义；BlockRecord code无市场，不能直接按字符串作沪深唯一身份。两节点概念/指数文件共105,826条成分已逐字段独立核对，见[修复报告](../archive/development/BLOCK_FILE_INTEGRITY_FIX_20261024.md)；供应商分类权威性仍待独立依据。建议每日低频下载，在元数据变化后刷新缓存。[cite:5](#cite-5)


签名依据：[Client入口](../../crates/rustdx-core/src/tcp/stock/client.rs)、[板块完整下载/严格解析](../../crates/rustdx-core/src/tcp/stock/block_info.rs)、[通用资源](../../crates/rustdx-core/src/tcp/stock/resource_file.rs)、[统计配置](../../crates/rustdx-core/src/tcp/stock/resource_stats.rs)。新范围证据见[资源与推送验收修复](../archive/development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md)。


<a id="cmd-0fb4"></a>
### 0fb4：指定日期历史分时兼容查询

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn history_minute(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
    ) -> std::io::Result<Vec<MinuteTimeData>>;
```

业务含义：按请求日期查询主站保留的历史分钟价格/量，适合短期分时回补。当前高层仍是兼容模型，不能替代完整分时合同。[cite:5](#cite-5)

结果 `Vec<MinuteTimeData>`，只有 `price:f64 / vol:i32`；没有独立服务器日期、均价或时间戳字段。底层 HistoryMinuteTime 保存 request date 与原始response。

使用约束：市场0/1、六位ASCII字母数字、有效YYYYMMDD；没有公开start/count/batch。当前价格沿用累加差值/100，未接目录精度，ETF等非两位价格不得据此假定正确。响应中辅助字段被兼容解析跳过；高层走try_parse，截断/非法价格报错。源文件注释中的“约1–2周保留”是历史经验描述，本轮未实测为稳定服务承诺。建议每交易日盘后归档，空值不证明当日无交易，超保留期用其他历史来源补齐。[cite:5](#cite-5)


签名依据：[history_minute](../../crates/rustdx-core/src/tcp/stock/client.rs#L507)。


<a id="cmd-0fc5"></a>
### 0fc5：当日原始成交事件与完整枚举

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn trade_page(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
    ) -> std::io::Result<super::TradePage>;

pub fn transaction(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
    ) -> std::io::Result<Vec<TransactionData>>;

pub fn transaction_all(
        &mut self,
        market: u16,
        code: &str,
        options: TransactionPaginationOptions,
    ) -> io::Result<TransactionCollection>;
```

业务含义：读取当日主站原始成交事件，包含成交、竞价过程快照、开盘撮合、盘后固定价及未知状态。记录通常是主站聚合事件，不是交易所逐笔成交ID清单。[cite:3](#cite-3)

完整单页方法实际名为 **trade_page**，返回 TradePage；兼容 `transaction` 返回 `Vec<TransactionData>`（time/price/vol/num/buyorsell）。TradePage通过getter读取query/start/request_count/raw_count/records/historical_prefix_raw/payload/source/next_offset，`confirmed_trades()`按当前配置筛选。每条TradeRecord保留原始价格差/整数价、数量、笔数槽、状态、尾部、字节范围、protocol、event_kind、phase、direction、qualification。

使用约束：市场0/1、六位ASCII字母数字；start:u16、count=1–2000。当前成交事件价格/100，未统一接报价目录精度；vol为未转换协议量，num不是稳定ID。状态8优先判为竞价快照、状态5在15:05–15:30属于盘后固定价候选/配置确认；不能仅按0/1删掉其他事件。ConfirmedByProfile是协议配置规则确认，不是交易所事实核验。当前命令无日期，query.date=None。事件、阶段、方向、成交资格与金额来源分别读取，见[事件字段状态](RESPONSE_FIELD_SEMANTICS.md#trade-fields)。

`transaction_all` 使用 TransactionPaginationOptions，短非空页继续、按raw数量推进直到空页；同分钟相同记录保留，整页重复、排序不合法、跨连接/当日跨日、预算/偏移耗尽均返回Err，**不返回成功的部分集合**。recheck_empty在同一连接复查空页；最终termination=EmptyPageObserved不证明全天交易所完整或盘中冻结快照。建议小窗口盘中2–5秒，稳定盘后枚举归档；保存raw再做业务筛选。[cite:3](#cite-3)


签名依据：[trade_page](../../crates/rustdx-core/src/tcp/stock/client.rs#L526) · [transaction](../../crates/rustdx-core/src/tcp/stock/client.rs#L563) · [transaction_all](../../crates/rustdx-core/src/tcp/stock/client/transactions.rs#L39)。


<a id="cmd-0fc6"></a>
### 0fc6：增强历史成交事件

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn history_trade_page(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        start: u16,
        count: u16,
    ) -> std::io::Result<super::TradePage>;

pub fn history_transaction(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
        date: u32,
    ) -> std::io::Result<Vec<TransactionData>>;

pub fn enhanced_history_transaction(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
        date: u32,
    ) -> std::io::Result<super::EnhancedHistoryTransactionPage>;

pub fn history_transaction_all(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        options: TransactionPaginationOptions,
    ) -> io::Result<TransactionCollection>;

pub fn enhanced_history_transaction_all(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        options: TransactionPaginationOptions,
    ) -> io::Result<TransactionCollection>;
```

业务含义：取得指定日期的增强历史事件，保留普通成交协议之外的笔数、参考字节和状态槽，用于历史成交过程分析。默认历史高层走0fc6，不自动回退旧0fb5。[cite:3](#cite-3)

完整通用单页实际名为 **history_trade_page(market,code,date,start,count)**；兼容 `history_transaction` 和专用 `enhanced_history_transaction` 的参数顺序是 **market,code,start,count,date**，不要调换。前者 TradePage，后者 EnhancedHistoryTransactionPage 含完整原始字节及 actual_trades/auction等视图。全量 `history_transaction_all` 与 `enhanced_history_transaction_all` 返回 TransactionCollection，event_pages保留完整事件页。

使用约束：有效YYYYMMDD，count=1–1800、start:u16；价格/数量/日期真实性边界同0fc5。requested_date只反映请求，响应没有交易日回显。全量短页继续且预算不足报错，不因过滤视图数量改变续页偏移。旧0fb5没有笔数槽，兼容num=0是缺失，不是真实“0笔”。

建议日终以固定日期枚举并保存源字节。主站样本的完整分页与量价额一致性已有专项证据，但不证明每证券、每日期均保留全部交易所逐笔，成交额不能用每条末价×聚合量当作精确原始额。[cite:3](#cite-3)


签名依据：[history_trade_page](../../crates/rustdx-core/src/tcp/stock/client.rs#L544) · [history_transaction](../../crates/rustdx-core/src/tcp/stock/client.rs#L582) · [enhanced_history_transaction](../../crates/rustdx-core/src/tcp/stock/client.rs#L642) · [history_transaction_all](../../crates/rustdx-core/src/tcp/stock/client/transactions.rs#L73) · [enhanced_history_transaction_all](../../crates/rustdx-core/src/tcp/stock/client/transactions.rs#L112)。


<a id="cmd-0fd1"></a>
### 0fd1：轻量价格走势图

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn sparkline(
        &mut self,
        market: u16,
        code: &str,
        options: SparklineOptions,
    ) -> io::Result<SparklineSeries>;

pub fn sparkline_batch(
        &self,
        stocks: &[(u16, &str)],
        options: SparklineOptions,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<SparklineSeries>>>;
```

业务含义：取得一组轻量价格点和基准价，适合列表里的小走势图。它没有完整OHLC、成交量或真实时间戳。[cite:1](#cite-1)

SparklineOptions默认selector=1/window=20；结果SparklineSeries保存request、market_raw、code、selector_echo、parameter_raw、capacity_raw、base_price/base_bits、page。每个SparklinePrice只有price:f32和bits。

使用约束：市场0/1、六位ASCII字母数字，window:u16且>0，selector任意u8不代表语义已核实。window并不等于点数，当前真实window20/40均曾返回61点；不得按window给点标注真实分钟，也不能用其补标准K线。响应校验证券/selector回显及capacity。建议关注池3–5秒起步，历史小图缓存后低频更新。[cite:1](#cite-1)


签名依据：[sparkline](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L42) · [sparkline_batch](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L208)。


<a id="cmd-0feb"></a>
### 0feb：近期分时日期选择

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn recent_minutes(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
    ) -> io::Result<MinuteSeries>;

pub fn recent_minutes_batch(
        &mut self,
        stocks: &[(u16, &str)],
        date: u32,
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<MinuteSeries>>>;
```

业务含义：使用特定日期选择器读取近期分时，含昨收、今开、均价与分钟量。其日期选择方式区别于0fb4；不能将二者当作可无条件互换的历史接口。[cite:1](#cite-1)[cite:9](#cite-9)

结果MinuteSeries保存request、requested_date、server_trading_date=None、date_selector_raw、previous_close/open_price及浮点位模式、scale和page。MinutePoint包括index、合成time_label、price、average_price、volume_raw:i64及原始价格/均价字段和还原整数。`date_qualification()`读取私有日期资格，当前为`Unverified { reason: ServerDoesNotEchoTradingDate }`。

使用约束：市场0/1、六位ASCII字母数字、有效YYYYMMDD，最多240点，无start/count。日期schema2将YYYYMMDD整数取32位补码，按小端发送（`date.wrapping_neg()`）；旧ordinal公式会错选日期。未回显日期，时间标签同0537为合成会话标签。价格/均价用当日目录精度，均价再多除100；量仍raw。批量按证券返回，最多10万项并先加载精度。

**旧日期冲突已定位为请求编码错误**：旧20260817/18请求实际选中20260609/10。修正后的两节点、六证券、六交易日共17,280个分钟点与同日0fb4一致，开盘/末价与日期日线一致，见[修复报告](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。Nonempty/解码成功仍不能证明任意日期真实性。按交易日分区或发布前调用`require_trading_date()`，默认Unsupported；调用方可用`MinuteDateReference`与`with_date_reference`绑定完整独立价格点、日期、来源摘要及本次原始结果。改写公开字段或来源后资格失效；日期参照不证明量/均价/合成标签或通用日期。消费约定见[应用接入第2节](../development/APPLICATION_INTEGRATION.md#2-价格数量金额和未知值)，字段状态见[日期边界](RESPONSE_FIELD_SEMANTICS.md#market-fields)。建议近期归档核对日期、盘中关注池3–5秒起步；历史数据低频重取。[cite:9](#cite-9)


签名依据：[recent_minutes](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L50) · [recent_minutes_batch](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L226)。


<a id="cmd-0ffc"></a>
### 0ffc：日资金流向与分档净额

下列为 `impl Client` 的实际签名，省略函数体；`io::Result` 即 `std::io::Result`，`crate::Result` 是本库错误类型。


```rust
pub fn money_flow_daily(&mut self, market: u16, code: &str) -> io::Result<MoneyFlowBlock>;

pub fn money_flow_daily_batch(
        &self,
        stocks: &[(u16, &str)],
        max_parallel: usize,
    ) -> io::Result<Vec<io::Result<MoneyFlowBlock>>>;
```

业务含义：读取主站日级资金分档，按超大、大、中、小四档计算净额，以及另一套主买净额与比例。这是供应商分类统计，不是从本库逐笔自行汇总的精确资金事实。[cite:1](#cite-1)

MoneyFlowBlock保存request、market_raw、code、reported_count_raw、header_raw和page。MoneyFlowDaily全部字段：date_raw、raw:[u32;21]、total_amount/total_amount_bits、buckets_raw:[u16;16]、net_amounts:[f64;4]、active_buy_net_amounts:[f64;4]、main_net/main_ratio、active_buy_net/active_buy_ratio。

使用约束：市场0/1、六位ASCII字母数字；无用户日期/start/count，返回主站保存的日期集，不能宣称全历史。使用专用消息路由0x007e2dXX，并仍通过受管连接/当日有效池。某些主站count=0时仅在剩余长度为88字节整倍数下推导记录数。

原始分档从raw[10..18]拆16个u16，相邻差值乘total_amount/50000得到净额；主力取超大+大，主买取另一套四档和。比例是**百分点**（差值/500），不是0..1比例；负净額是合法值。量额单位保留total的原始协议单位，业务外部单位和大小单定义需另证，不能直接推广为统一人民币尺度或主力真实身份。两套分档的准确索引、派生公式与未命名槽边界见[资金流字段状态](RESPONSE_FIELD_SEMANTICS.md#flow-f10-fields)。建议日级归档，盘中需要展示时5–15秒起步。[cite:1](#cite-1)


签名依据：[money_flow_daily](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L92) · [money_flow_daily_batch](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L254)。


## 4. 完整选项参数与分页结束规则

### 4.1 CommandScanOptions

| 字段 | 默认值 | 合法范围/含义 |
| --- | ---: | --- |
| max_pages:usize | 256 | 1..65536，页预算 |
| max_records:usize | 10000 | 1..1000000，保留记录预算；052b按原始条数计算 |
| timeout:Duration | NetworkPolicy.pagination_timeout，默认120秒 | 必须非零；外层deadline可以缩短 |

用于 special_limits_all、category_quotes_all、auction_series_all、delisted_bars_scan/all 等。通用集合带 pages、record_count、termination、complete、atomic；退市集合另有标准化records和raw_record_count。`atomic=false`明确不是冻结快照。[cite:1](#cite-1)[cite:2](#cite-2)

| 网络枚举 | 正常停止 | 短页处理 | 预算不足 |
| --- | --- | --- | --- |
| 完整044d目录 | 达到044e预期总条数 | 按实际数量继续 | 超时/不完整/重复/跨来源报Err |
| 052d区间K线 | 新状态入口观察空页或越过begin | 新入口按raw数继续，旧Vec短页停止 | 新入口预算不足complete=false；来源变化/冲突/不推进报Err；Vec无覆盖证明 |
| 0452特殊表 | 空页 | 继续 | 已保留页+complete=false |
| 052b退市日线 | 空页 | 继续 | 已保留页+complete=false |
| 054b分类行情 | 短页/空页 | 终止 | 已保留页+complete=false；动态重叠报Err |
| 056a竞价过程 | 短页/空页 | 终止 | 已保留页+complete=false；重复页报Err |
| 0fc5/0fc6完整明细 | 观察到空页 | 按raw条数继续 | 返回Err，不返回成功部分集合 |
| 02d0完整文本 | 达到指定字节长度 | 按实际字节继续 | 返回Err，提前空块不算完整 |

没有一个统一的“少于count就结束”规则适合所有命令。手动分页时先读该命令约定，不能用已过滤的业务记录数推进start。[cite:1](#cite-1)[cite:2](#cite-2)[cite:3](#cite-3)

### 4.2 TransactionPaginationOptions

| 字段 | 默认值 | 说明 |
| --- | ---: | --- |
| page_size:u16 | 1800 | 0fc6须1–1800；0fc5/旧0fb5须1–2000 |
| max_pages:usize | 100 | 包含最终空页；必须>0，空页复查不另占逻辑页 |
| max_records:usize | 67535 | 原始事件记录预算，等于65535+2000；必须>0 |
| max_bytes:usize | 8MiB | 解压后的累计正文，包含空页复查 |
| timeout:Duration | 120秒 | 总时限会取本值、进程pagination_timeout和外层deadline的更小者 |
| page_delay:Duration | 100ms | 客户端分页间隔，不是主站QPS |

在记录预算恰好耗尽时允许count=1空页探测；若还有记录则报RecordLimit。u16偏移不会回绕，状态5/8等原始事件参与raw计数。预算错误可通过io::Error内部downcast读取TransactionPaginationError的reason/start/pages/records，其他网络/取消/解码错误保留原error kind。[cite:3](#cite-3)

### 4.3 CompanyInfoReadOptions

| 字段 | 默认值 | 上限/行为 |
| --- | ---: | --- |
| chunk_size:u32 | 30000字节 | 1–60000 |
| max_bytes:u32 | 64MiB | 1–64MiB，完整范围长度不得超过它 |
| max_chunks:usize | 4096 | 1–65536 |
| timeout:Duration | 分页策略默认120秒 | 非零，总体budget |

对 `company_info_text` 是该文件范围的预算；对 `f10_with_options` 是全部栏目共用预算。请求本身仍用offset/length:u32，length>0，不溢出；零栏目只能由f10保留空串而跳过网络请求。[cite:2](#cite-2)

### 4.4 SubscriptionOptions 与状态

| 字段 | 默认值 | 合法范围 |
| --- | ---: | --- |
| max_queue_frames:usize | 256 | 1–256 |
| max_queue_bytes:usize | 8MiB | 1–8MiB |
| refresh_timeout:Duration | 请求策略默认15秒 | 非零且Instant能表示 |

`poll_push(timeout)`另指定本次等待时限，未取得完整推送时返回None，并可保留已收到的半帧；此值不是refresh_timeout。所有会话操作仍继承取消/总体deadline。[cite:8](#cite-8)

| DepthSyncStatus | 可观察含义 | 正确使用 |
| --- | --- | --- |
| AwaitingBaseline | 未取得全部订阅证券的有效基线 | 可展示“等待基线”，不能当作完整五档集合 |
| Synchronized | 已有全证券基线且会话健康 | 按证券完整状态读取，不累计加量 |
| Invalid | 断线、目录失效、游标/队列等导致失效 | 残留quote仅诊断；刷新目录并rebuild |
| Closed | 已主动关闭 | 重新创建订阅 |

### 4.5 其他选项完整字段

| 类型 | 全部字段/默认值 | 使用约束 |
| --- | --- | --- |
| AuctionOptions | mode:u32=3，start:u32=0，count:u32=500 | count1–4095，start+count不溢出；mode未知语义待证 |
| SparklineOptions | selector:u8=1，window:u16=20 | window>0；window不等于点数 |
| CategoryQuoteOptions | start:u16=0，count:u16=80，ascending:bool=false，sort_reverse_raw:Option<u16>=None，filter_raw:u16=0 | count1–700；raw字段只表达原始值，不保证所有组合可用 |
| TradeEstimateOptions | include_candidates:bool；显式示例使用false | 是否单独纳入候选统计，不把候选当默认已确认成交 |
| TurnoverOptions | page_size:u16=800，max_pages:usize=100，max_records:usize=65536，max_bytes:usize=8MiB，timeout=120秒，page_delay=100ms，daily_category:u16=9，profile:Option<TurnoverProfile>=None | page_size受KlineRequest1–800校验；其他预算>0；profile缺失不输出已确认业务总额 |

### 4.6 052d周期表

| category | 当前周期解释 |
| ---: | --- |
| 0 | 5分钟 |
| 1 | 15分钟 |
| 2 | 30分钟 |
| 3 | 1小时 |
| 4 | 日线，另一种日线请求类别 |
| 5 | 周线 |
| 6 | 月线 |
| 7 | 1分钟的旧类别 |
| 8 | 1分钟 |
| 9 | 日线，高层k及默认源日成交额使用 |
| 10 | 季线 |
| 11 | 年线 |

原始KlineRequest接受0..11；标准 `Period` 的枚举归一化可能将某些别名类别映射成一个值。整数“周期编号”不能直接套 eltdx 的增强周期对象；格式接受不等于每个证券/主站/周期已实测。实测过的4与9日线量口径存在差异；选择明确VolumeUnit后再构造CanonicalBar。[cite:1](#cite-1)[cite:3](#cite-3)

## 5. 相关本地业务处理与旧入口

### 5.1 价格精度缓存

`Client::refresh_quote_metadata(markets)`加载/更新当日完整目录；`quote_metadata_status()`查看各市场加载时间、来源、证券数/未支持精度数；`quote_metadata_store()`取得共享Arc，`with_quote_metadata(config, store)`显式让多个Client共享同一个缓存。`QuoteMetadataStore::snapshot()`返回只读PrecisionSnapshot；`scale(market,code)`给出PriceScale，decimal=2/3/4对应除数100/1000/10000，跨日需要刷新。

冷目录加载可能包含044e及多页044d请求。快照/近期分时的15秒请求总体时限中也包含预热，完整加载可先走120秒refresh预热。离线 `PrecisionSnapshot::from_decimals`只声明给定精度，不等于实际完成网络目录核验。[cite:1](#cite-1)

### 5.2 源成交额、明细估算与对账

`Client::reported_turnover(market,code,date,scope,options)`读取052d源金额：

| TurnoverScope | 行为 |
| --- | --- |
| Minute1 | category=8，按响应分钟日期窗口读取 |
| Minute5 | category=0 |
| RegularWithAuctions | 指定daily_category，默认9 |
| AfterHours | 当前没有独立盘后源；返回不可用原因 |
| Combined | 保留可得常规部分；不伪造缺少的盘后部分 |

`TurnoverProfile::a_share(market,code,date,endpoint,evidence)`由调用者明确提供证券/日期/主站范围与证据；不能凭创建profile就认定外部事实已验证。`ReportedTurnover`包含availability、unavailable_reason、reported_amount_yuan、regular_component_*、sampling、date_basis、quantity_unit、raw bars/pages与来源。合计使用Neumaier补偿求和。[cite:3](#cite-3)

`TradePage/TradeCollection::amount_estimates(context)`和`amount_estimate_summary(context, options)`是**估算**：需显式TradeAmountContext的数量单位/evidence，未核实单位时用unknown上下文。分时聚合末价×量不是精确成交额。估算保留origin/qualification、常规含竞价/盘后/候选分量、未计算记录、不可用原因；`TradeAmountReconciliation`比对估算与源金额，仍须对齐证券、日期、时段、单位、来源/采样，不把差额自动归因于少记。[cite:3](#cite-3)

### 5.3 显式兼容方法

| 当前方法 | 实际协议/模型 | 约束 |
| --- | --- | --- |
| minute_legacy | 051d，必要时今日0fb4回退 | price/vol兼容视图、历史精度处理 |
| stocks_legacy | 0450 | 29字节旧目录、1000条页，仍做完整总量检查 |
| history_transaction_legacy | 0fb5 | 单页，无笔数槽，num=0表示缺失 |
| history_trade_page_legacy | 0fb5 TradePage | 原始字段保留，事件分类候选资格 |
| history_transaction_all_legacy | 0fb5全量 | 严格分页，不升级为增强0fc6 |
| finance_legacy | 0010旧FinanceInfoData | 旧字段名/数量缩放，不混入标准财务值 |
| QuotesRequest.recv_legacy / decode_quotes_legacy | 053e旧/100 | 只作明确历史兼容，不建立新证券精度 |
| ex_right_reference(preclose,ev) | 纯本地除权参考价 | 非网络命令；正式复权须核对有效权息事件 |

低层 `CompanyInfoCategory/Content`、`BlockInfoMeta/Chunk`等旧构造器仍有panic或静默截断边界。推荐新F10/板块元数据/退市调用选择checked类型，不用旧对象替代新参数校验。[cite:5](#cite-5)

### 5.4 已有证据的范围

前一实施阶段两台7709服务器固定了98组完整帧（其中96组是四新增注册项业务帧），独立核验24084条含重采样的退市记录及44个F10块；每台三个证券完整扫描合计12039条。它证明这些样本的解析和分页，而不是全市场、市场2或全部26业务参数的验收。[cite:7](#cite-7)

手册整理本身不计为新增验收；后续0feb日期修复实采单独记录在[修复报告](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。当前0feb通用日期资格/完整保存窗口、0547真实盘中主动推送窗口、部分财务/股本/增强周期/通用文件合同差距均保留；接口使用时应读取相应资格和来源，不用“有返回数据”代替业务条件。[cite:7](#cite-7)[cite:8](#cite-8)[cite:9](#cite-9)

## 6. 可编译使用示例

以下仅示范实际API，没有虚构返回行情。本次校验编译，不执行示例或联网；使用时按证据和业务期限调整预算。

### 6.1 初始化、快照、F10与退市建档

<!-- compile-example: basic -->
```rust
use rustdx_complete::tcp::{
    configure_network, startup, NetworkPolicy,
    stock::{Client, CommandScanOptions},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_network(NetworkPolicy::default())?;
    let _startup = startup::initialize()?;
    let mut client = Client::new()?;
    client.refresh_quote_metadata(&[0, 1])?;

    let quotes = client.quotes_with_metadata(&[(0, "000001"), (1, "600000")])?;
    for missing in &quotes.missing_codes {
        eprintln!("本次缺失：{missing:?}");
    }
    let minute = client.minute_series(0, "000001")?;
    println!("{} 个分时点，服务器日期：{:?}", minute.points.len(), minute.trading_date);

    let meta = client.block_metadata("block_gn.dat")?;
    println!("文件大小={}，服务器MD5={}", meta.size, meta.md5);
    let f10 = client.f10(0, "000001")?;
    println!("F10栏目数={}", f10.len());

    let archive = client.delisted_bars_all(
        0, "000038",
        CommandScanOptions { max_records: 20_000, ..Default::default() },
    )?;
    if !archive.complete {
        return Err(std::io::Error::other(format!("退市枚举未完成：{:?}", archive.termination)).into());
    }
    println!("该主站可枚举退市日线={}条", archive.records.len());
    Ok(())
}
```

### 6.2 完整F10范围与保序批量

<!-- compile-example: f10_batch -->
```rust
use rustdx_complete::tcp::stock::{
    Client, CompanyInfoReadOptions, CompanyInfoTextRequest, IntradayAuxKind,
};

fn main() -> std::io::Result<()> {
    let mut client = Client::new()?;
    let directory = client.company_info_directory(0, "000001")?;
    if let Some(item) = directory.entries.iter().find(|i| i.length > 0) {
        let text = client.company_info_text(
            CompanyInfoTextRequest::new(0, "000001", &item.filename, item.start, item.length)?,
            CompanyInfoReadOptions::default(),
        )?;
        println!("{}：{}字节、{}块", item.name, text.content_raw.len(), text.chunks.len());
    }

    let stocks = [(0, "000001"), (1, "600000")];
    let replies = client.intraday_aux_batch(&stocks, IntradayAuxKind::VolumeComparison, 2)?;
    for (stock, reply) in stocks.iter().zip(replies) {
        match reply {
            Ok(series) => println!("{stock:?}：{}个副图点", series.page.records.len()),
            Err(error) => eprintln!("{stock:?}：{error}"),
        }
    }
    Ok(())
}
```

### 6.3 历史事件完整枚举和本地估算

<!-- compile-example: trades -->
```rust
use rustdx_complete::tcp::stock::{
    Client, TradeCollection, TradeQuery, TradeTermination,
    TradeAmountContext, TradeEstimateOptions, TradeEstimateSummary,
    TransactionCollection, TransactionPaginationOptions,
};

fn summarize(
    query: TradeQuery,
    all: TransactionCollection,
    context: &TradeAmountContext,
) -> std::io::Result<TradeEstimateSummary> {
    let events = TradeCollection::new(query, all.event_pages, TradeTermination::EmptyPageObserved)?;
    events.amount_estimate_summary(context, TradeEstimateOptions { include_candidates: false })
}

fn main() -> std::io::Result<()> {
    let mut client = Client::new()?;
    let query = TradeQuery::history(0, "000001", 20261023)?;
    let all = client.history_transaction_all(
        0, "000001", 20261023, TransactionPaginationOptions::default(),
    )?;
    let context = TradeAmountContext::unknown(query.clone());
    let summary = summarize(query, all, &context)?;
    println!("数量单位未核实的记录数={}", summary.unavailable_records());
    Ok(())
}
```

### 6.4 0547独占会话、状态与推送

<!-- compile-example: subscription -->
```rust
use std::time::Duration;
use rustdx_complete::tcp::stock::{Client, DepthSyncStatus, SubscriptionOptions};

fn main() -> std::io::Result<()> {
    let mut client = Client::new()?;
    client.refresh_quote_metadata(&[0, 1])?;
    let mut session = client.subscribe_depth(
        &[(0, "000001"), (1, "600000")],
        SubscriptionOptions::default(),
    )?;
    session.refresh()?;
    for _ in 0..5 {
        if let Some(push) = session.poll_push(Duration::from_millis(500))? {
            println!("本次更新证券数={}", push.refresh.page.records.len());
        }
        let state = session.snapshot();
        if state.status == DepthSyncStatus::Synchronized {
            for security in state.states {
                println!("{:?} 游标={}", security.symbol, security.cursor);
            }
        } else {
            eprintln!("当前状态={:?}", state.status);
        }
    }
    session.close();
    Ok(())
}
```


## 7. 其他高层方法的完整签名

以下补齐缓存控制与显式旧协议入口；由这些方法组合出来的业务不新增26项opcode计数。[cite:1](#cite-1)


### history_trade_page_legacy

```rust
pub fn history_trade_page_legacy(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        start: u16,
        count: u16,
    ) -> std::io::Result<super::TradePage>;
```

[源码位置](../../crates/rustdx-core/src/tcp/stock/client.rs#L610)。


### history_transaction_all_legacy

```rust
pub fn history_transaction_all_legacy(
        &mut self,
        market: u16,
        code: &str,
        date: u32,
        options: TransactionPaginationOptions,
    ) -> io::Result<TransactionCollection>;
```

[源码位置](../../crates/rustdx-core/src/tcp/stock/client/transactions.rs#L92)。


### history_transaction_legacy

```rust
pub fn history_transaction_legacy(
        &mut self,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
        date: u32,
    ) -> std::io::Result<Vec<TransactionData>>;
```

[源码位置](../../crates/rustdx-core/src/tcp/stock/client.rs#L599)。


### quote_metadata_status

```rust
pub fn quote_metadata_status(&self) -> std::io::Result<Vec<MarketPrecision>>;
```

[源码位置](../../crates/rustdx-core/src/tcp/stock/client.rs#L86)。


### quote_metadata_store

```rust
pub fn quote_metadata_store(&self) -> Arc<QuoteMetadataStore>;
```

[源码位置](../../crates/rustdx-core/src/tcp/stock/client.rs#L82)。


### refresh_quote_metadata

```rust
pub fn refresh_quote_metadata(
        &mut self,
        markets: &[u16],
    ) -> std::io::Result<Vec<MarketPrecision>>;
```

[源码位置](../../crates/rustdx-core/src/tcp/stock/client.rs#L91)。


## 8. 底层请求、解码器和结果模型声明


以下从当前源码提取声明；该部分展示准确类型/字段/方法，不是可直接编译的重复类型定义。`pub` 字段可读取；私有字段、`pub(super)` 字段不能从使用方直接写入，应走同节getter。请求参数私有的类型必须由new校验后构造。包含必要的元数据、事件/金额处理和兼容模型；不把纯本地处理再算作网络命令。

对于旧Tdx对象，调用方式为 `use rustdx_complete::tcp::Tdx; request.recv_parsed(&mut tcp)?;`，可读 `result()`；优先使用try_parse而不是忽略错误的parse。新请求的new/encode/recv与独立decode函数分离。只有实际导出的类型和函数才属于用户入口；源码中的pub辅助实现不等于新增顶层方法。[cite:1](#cite-1)[cite:5](#cite-5)


### src/tcp/mod.rs


**TcpConfig** · [定义](../../crates/rustdx-core/src/tcp/mod.rs#L58)

```rust
pub struct TcpConfig {
    pub timeout: Duration,
    pub ip: Option<SocketAddr>,
    pub ip_pool: Option<ip::IpPool>,
    pub auto_reconnect: usize,
    pub retry_delay_ms: u64,
    pub recheck_empty: bool,
}
```


**Tdx** · [定义](../../crates/rustdx-core/src/tcp/mod.rs#L768)

```rust
pub trait Tdx {
    const SEND: &'static [u8];
    const TAG: &'static str;
    const LEN: usize = Self::SEND.len();
    type Item: ?Sized;
    fn send(&mut self) -> &[u8];
    fn recv(&mut self, tcp: &mut Tcp) -> Result<Vec<u8>> {
        send_recv_decompress(tcp, self.send(), Self::TAG)
    }
    fn parse(&mut self, response: Vec<u8>);
    fn try_parse(&mut self, response: Vec<u8>) -> Result<()> {
        self.parse(response);
        Ok(())
    }
    fn recv_parsed(&mut self, tcp: &mut Tcp) -> Result<&Self::Item> {
        let response = self.recv(tcp)?;
        self.try_parse(response)
            .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {e}", Self::TAG)))?;
        Ok(self.result())
    }
    fn result(&self) -> &Self::Item;
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl TcpConfig

pub fn with_index(index: usize, timeout: Duration) -> Self;


// impl Tcp

pub fn connection_id(&self) -> NonZeroU64;


pub fn new() -> Result<Self>;


pub fn with_config(config: &TcpConfig) -> Result<Self>;


pub fn with_config_and_observer(
        config: &TcpConfig,
        observer: Arc<dyn FrameObserver>,
    ) -> Result<Self>;


pub fn with_timeout<T>(
        &mut self,
        timeout: Duration,
        run: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T>;


pub fn config(&self) -> &TcpConfig;


pub fn is_reusable(&self) -> bool;


pub fn reconnect(&mut self) -> Result<()>;


pub fn heartbeat(&mut self) -> Result<HeartbeatAck>;


pub fn heartbeat_legacy(&mut self) -> Result<u16>;


pub fn set_heartbeat_interval(&mut self, interval: Option<Duration>) -> Result<()>;


pub fn retry<T>(
        &mut self,
        mut f: impl FnMut(&mut Tcp) -> Result<T>,
        attempts: usize,
    ) -> Result<T>;


pub fn send_recv(&mut self, send: &[u8]) -> Result<(usize, usize)>;


pub fn into_inner(mut self) -> (TcpStream, BufTcp, [u8; RECV_SIZE]);


pub fn get_ref(&self) -> (&TcpStream, &BufTcp, &[u8]);


pub fn get_ref_recv(&self) -> &[u8];


// free function

pub fn send_recv_decompress(tcp: &mut Tcp, send: &[u8], tag: &str) -> Result<Vec<u8>>;


pub fn send_recv(tcp: &mut Tcp, send: &[u8], tag: &str) -> Result<(Vec<u8>, u16, u16)>;


pub fn tcpstream() -> Result<(TcpStream, BufTcp, [u8; RECV_SIZE])>;


pub fn tcpstream_with_timeout(timeout: Duration) -> Result<(TcpStream, BufTcp, [u8; RECV_SIZE])>;


pub fn tcpstream_ip(ip: &SocketAddr) -> Result<(TcpStream, BufTcp, [u8; RECV_SIZE])>;


pub fn tcpstream_ip_with_timeout(
    ip: &SocketAddr,
    timeout: Duration,
) -> Result<(TcpStream, BufTcp, [u8; RECV_SIZE])>;


```


### src/tcp/basic.rs


**SecurityCount** · [定义](../../crates/rustdx-core/src/tcp/basic.rs#L6)

```rust
pub struct SecurityCount {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**SecurityList** · [定义](../../crates/rustdx-core/src/tcp/basic.rs#L72)

```rust
pub struct SecurityList {
    pub send: Box<[u8]>,
    pub market: u16,
    pub start: u16,
    pub count: usize,
    pub response: Vec<u8>,
    pub data: Box<[SecurityListData]>,
}
```


**SecurityListData** · [定义](../../crates/rustdx-core/src/tcp/basic.rs#L185)

```rust
pub struct SecurityListData {
    pub code: String,
    pub name: String,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl SecurityCount

pub fn new(market: u16) -> Self;


pub fn market(&mut self, market: u16);


// impl SecurityList

pub fn new(market: u16, start: u16) -> Self;


// impl SecurityListData

pub fn parse(bytes: &[u8]) -> Self;


// free function

pub fn send_packs(tcp: &mut super::Tcp, decompress: bool) -> Result<()>;


```


### src/tcp/heartbeat.rs


**HeartbeatAck** · [定义](../../crates/rustdx-core/src/tcp/heartbeat.rs#L20)

```rust
pub struct HeartbeatAck {
    pub reserved_raw: [u8; 6],
    pub server_date_raw: u32,
    pub server_date: Option<chrono::NaiveDate>,
    pub raw_payload: Vec<u8>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// free function

pub fn decode_heartbeat(bytes: &[u8]) -> io::Result<HeartbeatAck>;


// impl IdleHeartbeat

pub fn start(
        socket: &TcpStream,
        gate: Arc<FrameGate>,
        interval: Duration,
        timeout: Duration,
        observer: Option<Arc<dyn FrameObserver>>,
        pool: super::ip::IpPool,
    ) -> io::Result<Self>;


pub fn stop(&mut self, shutdown: bool);


```


### src/tcp/governance.rs


**NetworkPolicy** · [定义](../../crates/rustdx-core/src/tcp/governance.rs#L14)

```rust
pub struct NetworkPolicy {
    pub max_connections: usize,
    pub shared: Option<SharedNetworkPolicy>,
    pub acquire_timeout: Duration,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub pagination_timeout: Duration,
    pub batch_timeout: Duration,
    pub endpoint_cooldown: Duration,
}
```


**SharedNetworkPolicy** · [定义](../../crates/rustdx-core/src/tcp/governance.rs#L42)

```rust
pub struct SharedNetworkPolicy {
    pub directory: PathBuf,
    pub max_connections: usize,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// free function

pub fn configure_network(policy: NetworkPolicy) -> io::Result<()>;


pub fn network_policy() -> &'static NetworkPolicy;


pub fn network_stats() -> NetworkStats;


// impl Governor

pub fn acquire(self: &Arc<Self>, deadline: Instant) -> io::Result<Permit>;


pub fn candidates(&self, addresses: &[SocketAddr]) -> Vec<SocketAddr>;


pub fn endpoint_result(&self, address: SocketAddr, success: bool);


pub fn request_result(&self, success: bool);


pub fn observation(&self, elapsed: Duration, empty: bool);


// free function

pub fn acquire_network(timeout: Duration) -> io::Result<Permit>;


pub fn observe_request(elapsed: Duration, success: bool, empty: bool);


pub fn observe_empty_response();


```


### src/tcp/evidence.rs


**FrameEvent** · [定义](../../crates/rustdx-core/src/tcp/evidence.rs#L9)

```rust
pub enum FrameEvent<'a> {
    Request {
        endpoint: SocketAddr,
        request: &'a [u8],
    },
    Response {
        endpoint: SocketAddr,
        request: &'a [u8],
        header: &'a [u8; 16],
        body: &'a [u8],
    },
    Failed {
        endpoint: SocketAddr,
        request: &'a [u8],
        kind: io::ErrorKind,
    },
}
```


**FrameObserver** · [定义](../../crates/rustdx-core/src/tcp/evidence.rs#L29)

```rust
pub trait FrameObserver: Debug + Send + Sync {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()>;
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// free function

pub fn decode_frame(header: &[u8; 16], body: &[u8], decompress: bool) -> io::Result<Vec<u8>>;


```


### src/tcp/startup.rs


**StartupReport** · [定义](../../crates/rustdx-core/src/tcp/startup.rs#L24)

```rust
pub struct StartupReport {
    pub daily: DailyIpPool,
    pub endpoint: SocketAddr,
    pub connection_id: NonZeroU64,
    pub heartbeat: HeartbeatAck,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// free function

pub fn initialize() -> io::Result<StartupReport>;


pub fn initialize_with_config(config: &TcpConfig) -> io::Result<StartupReport>;


pub fn initialize_until(config: &TcpConfig, deadline: Instant) -> io::Result<StartupReport>;


pub fn initialize_from_env() -> io::Result<StartupReport>;


pub fn snapshot() -> Option<StartupReport>;


pub fn release_client();


```


### src/tcp/stock/auctions.rs


**AuctionOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/auctions.rs#L9)

```rust
pub struct AuctionOptions {
    pub mode: u32,
    pub start: u32,
    pub count: u32,
}
```


**AuctionSeriesRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/auctions.rs#L24)

```rust
pub struct AuctionSeriesRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**AuctionPoint** · [定义](../../crates/rustdx-core/src/tcp/stock/auctions.rs#L72)

```rust
pub struct AuctionPoint {
    pub index: u32,
    pub minute_raw: u16,
    pub second_raw: u8,
    pub time_label: String,
    pub price: f32,
    pub price_bits: u32,
    pub matched_volume_raw: u32,
    pub unmatched_signed_raw: i32,
    pub unmatched_volume_raw: u32,
    pub unmatched_sign: i8,
    pub reserved_raw: u8,
}
```


**AuctionSeries** · [定义](../../crates/rustdx-core/src/tcp/stock/auctions.rs#L87)

```rust
pub struct AuctionSeries {
    pub request: AuctionSeriesRequest,
    pub requested_date: Option<u32>,
    pub server_trading_date: Option<u32>,
    pub process_snapshots: bool,
    pub page: CommandPage<AuctionPoint>,
}
```


**AuctionCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/auctions.rs#L94)

```rust
pub type AuctionCollection = CommandCollection<AuctionPoint>;
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl AuctionSeriesRequest

pub fn new(
        market: u16,
        code: &str,
        trading_date: Option<u32>,
        options: AuctionOptions,
    ) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<AuctionSeries>;


// free function

pub fn decode_auction_series(
    request: &AuctionSeriesRequest,
    bytes: &[u8],
) -> io::Result<AuctionSeries>;


```


### src/tcp/stock/block_info.rs


**BlockRecord** · [定义](../../crates/rustdx-core/src/tcp/stock/block_info.rs#L6)

```rust
pub struct BlockRecord {
    pub blockname: String,
    pub block_type: u16,
    pub code: String,
}
```


**BlockInfoMeta** · [定义](../../crates/rustdx-core/src/tcp/stock/block_info.rs#L23)

```rust
pub struct BlockInfoMeta {
    pub send: Box<[u8]>,
    pub response: Vec<u8>,
    pub size: u32,
}
```


**BlockInfoChunk** · [定义](../../crates/rustdx-core/src/tcp/stock/block_info.rs#L84)

```rust
pub struct BlockInfoChunk {
    pub send: Box<[u8]>,
    pub response: Vec<u8>,
    pub data: Vec<u8>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl BlockInfoMeta

pub fn new(block_file: &str) -> Self;


// impl BlockInfoChunk

pub fn new(block_file: &str, start: u32, size: u32) -> Self;


// free function

pub fn get_block_info(
    tcp: &mut crate::tcp::Tcp,
    block_file: &str,
) -> crate::Result<Vec<BlockRecord>>;


pub fn parse_block_content(data: &[u8]) -> Vec<BlockRecord>;


```


### src/tcp/stock/category_quotes.rs


**QuoteCategory** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L8)

```rust
pub enum QuoteCategory {
    AShares,
    Raw(u16),
}
```


**QuoteSortField** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L21)

```rust
pub enum QuoteSortField {
    Code,
    Price,
    Amount,
    ChangePercent,
    OrderAmount,
    Raw(u16),
}
```


**CategoryQuoteOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L42)

```rust
pub struct CategoryQuoteOptions {
    pub start: u16,
    pub count: u16,
    pub ascending: bool,
    pub sort_reverse_raw: Option<u16>,
    pub filter_raw: u16,
}
```


**CategoryQuotesRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L61)

```rust
pub struct CategoryQuotesRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**CategoryQuote** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L133)

```rust
pub struct CategoryQuote {
    pub quote: BasicQuote,
    pub status_or_sort_raw: u16,
    pub rise_speed_raw: i16,
    pub rise_speed: f64,
    pub short_turnover_raw: i16,
    pub short_turnover: f64,
    pub two_minute_amount: f32,
    pub opening_rush_raw: i16,
    pub opening_rush: f64,
    pub extra_pair_raw: Vec<u8>,
    pub volume_rise_speed: f32,
    pub depth_raw: f32,
    pub extra_metadata_raw: Vec<u8>,
    pub active2_raw: u16,
}
```


**CategoryQuotePage** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L150)

```rust
pub struct CategoryQuotePage {
    pub request: CategoryQuotesRequest,
    pub header_raw: u16,
    pub page: CommandPage<CategoryQuote>,
}
```


**CategoryQuoteCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/category_quotes.rs#L155)

```rust
pub type CategoryQuoteCollection = CommandCollection<CategoryQuote>;
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl QuoteCategory

pub fn raw(self) -> u16;


// impl QuoteSortField

pub fn raw(self) -> u16;


// impl CategoryQuotesRequest

pub fn new(
        category: QuoteCategory,
        sort: QuoteSortField,
        options: CategoryQuoteOptions,
    ) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(
        &self,
        tcp: &mut Tcp,
        metadata: &PrecisionSnapshot,
    ) -> io::Result<CategoryQuotePage>;


// free function

pub fn decode_category_quotes(
    request: &CategoryQuotesRequest,
    bytes: &[u8],
    metadata: &PrecisionSnapshot,
) -> io::Result<CategoryQuotePage>;


```


### src/tcp/stock/client.rs


**BatchKline** · [定义](../../crates/rustdx-core/src/tcp/stock/client.rs#L1030)

```rust
pub struct BatchKline<'a> {
    pub market: u16,
    pub code: &'a str,
    pub result: std::io::Result<Vec<KlineData<'a>>>,
}
```


**BatchBars** · [定义](../../crates/rustdx-core/src/tcp/stock/client.rs#L1041)

```rust
pub struct BatchBars<'a> {
    pub market: u16,
    pub code: &'a str,
    pub result: std::io::Result<Vec<KlineData<'a>>>,
}
```


**Adj** · [定义](../../crates/rustdx-core/src/tcp/stock/client.rs#L1052)

```rust
pub enum Adj {
    Qfq,
    Hfq,
}
```


### src/tcp/stock/command_request.rs


**CommandSymbol** · [定义](../../crates/rustdx-core/src/tcp/stock/command_request.rs#L7)

```rust
pub struct CommandSymbol {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**CommandSource** · [定义](../../crates/rustdx-core/src/tcp/stock/command_request.rs#L95)

```rust
pub struct CommandSource {
    pub opcode: u16,
    pub endpoint: Option<SocketAddr>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub connection_id: Option<u64>,
    pub metadata_generation: Option<u64>,
}
```


**CommandPage** · [定义](../../crates/rustdx-core/src/tcp/stock/command_request.rs#L121)

```rust
pub struct CommandPage<T> {
    pub records: Vec<T>,
    pub source: CommandSource,
    pub raw_payload: Arc<[u8]>,
}
```


**CommandTermination** · [定义](../../crates/rustdx-core/src/tcp/stock/command_request.rs#L137)

```rust
pub enum CommandTermination {
    EmptyPage,
    ShortPage,
    PageBudget,
    RecordBudget,
    OffsetLimit,
}
```


**CommandScanOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/command_request.rs#L145)

```rust
pub struct CommandScanOptions {
    pub max_pages: usize,
    pub max_records: usize,
    pub timeout: Duration,
}
```


**CommandCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/command_request.rs#L173)

```rust
pub struct CommandCollection<T> {
    pub pages: Vec<CommandPage<T>>,
    pub record_count: usize,
    pub termination: CommandTermination,
    pub complete: bool,
    pub atomic: bool,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl CommandSymbol

pub fn new(market: u16, code: &str) -> io::Result<Self>;


pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


```


### src/tcp/stock/company_info.rs


**CompanyInfoCategoryItem** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info.rs#L6)

```rust
pub struct CompanyInfoCategoryItem {
    pub name: String,
    pub filename: String,
    pub start: u32,
    pub length: u32,
}
```


**CompanyInfoCategory** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info.rs#L32)

```rust
pub struct CompanyInfoCategory<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub response: Vec<u8>,
    pub data: Vec<CompanyInfoCategoryItem>,
}
```


**CompanyInfoContent** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info.rs#L121)

```rust
pub struct CompanyInfoContent<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub filename: &'d str,
    pub start: u32,
    pub length: u32,
    pub response: Vec<u8>,
    pub data: String,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> CompanyInfoCategory<'d>

pub fn new(market: u16, code: &'d str) -> Self;


// impl<'d> CompanyInfoContent<'d>

pub fn new(market: u16, code: &'d str, filename: &'d str, start: u32, length: u32) -> Self;


```


### src/tcp/stock/company_info_request.rs


**CompanyInfoDirectoryRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L39)

```rust
pub struct CompanyInfoDirectoryRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**CompanyInfoEntry** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L71)

```rust
pub struct CompanyInfoEntry {
    pub name: String,
    pub filename: String,
    pub start: u32,
    pub length: u32,
    pub name_raw: Vec<u8>,
    pub filename_raw: Vec<u8>,
}
```


**CompanyInfoDirectory** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L90)

```rust
pub struct CompanyInfoDirectory {
    pub request: CompanyInfoDirectoryRequest,
    pub entries: Vec<CompanyInfoEntry>,
    pub trailing_payload: Vec<u8>,
    pub source: CommandSource,
    pub raw_payload: Arc<[u8]>,
}
```


**CompanyInfoTextRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L130)

```rust
pub struct CompanyInfoTextRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**CompanyInfoTextChunk** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L192)

```rust
pub struct CompanyInfoTextChunk {
    pub request: CompanyInfoTextRequest,
    pub header_raw: [u8; 10],
    pub declared_length: u16,
    pub content_raw: Vec<u8>,
    pub trailing_payload: Vec<u8>,
    pub source: CommandSource,
    pub raw_payload: Arc<[u8]>,
}
```


**CompanyInfoReadOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L232)

```rust
pub struct CompanyInfoReadOptions {
    pub chunk_size: u32,
    pub max_bytes: u32,
    pub max_chunks: usize,
    pub timeout: Duration,
}
```


**CompanyInfoText** · [定义](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs#L265)

```rust
pub struct CompanyInfoText {
    pub request: CompanyInfoTextRequest,
    pub text: String,
    pub content_raw: Vec<u8>,
    pub chunks: Vec<CompanyInfoTextChunk>,
    pub complete: bool,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl CompanyInfoDirectoryRequest

pub fn new(market: u16, code: &str) -> io::Result<Self>;


pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<CompanyInfoDirectory>;


// impl CompanyInfoEntry

pub fn to_legacy(&self) -> super::CompanyInfoCategoryItem;


// free function

pub fn decode_company_info_directory(
    request: &CompanyInfoDirectoryRequest,
    bytes: &[u8],
) -> io::Result<CompanyInfoDirectory>;


// impl CompanyInfoTextRequest

pub fn new(
        market: u16,
        code: &str,
        filename: &str,
        offset: u32,
        length: u32,
    ) -> io::Result<Self>;


pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn filename_raw(&self) -> &[u8];


pub fn offset(&self) -> u32;


pub fn length(&self) -> u32;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<CompanyInfoTextChunk>;


// impl CompanyInfoTextChunk

pub fn decode_text(&self) -> io::Result<String>;


// free function

pub fn decode_company_info_text(
    request: &CompanyInfoTextRequest,
    bytes: &[u8],
) -> io::Result<CompanyInfoTextChunk>;


pub fn company_info_text(
    tcp: &mut Tcp,
    request: CompanyInfoTextRequest,
    options: CompanyInfoReadOptions,
) -> io::Result<CompanyInfoText>;


```


### src/tcp/stock/delisted_bars.rs


**DelistedBarsRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs#L12)

```rust
pub struct DelistedBarsRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**DelistedBar** · [定义](../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs#L60)

```rust
pub struct DelistedBar {
    pub date_raw: u32,
    pub date: NaiveDate,
    pub time: DateTime<FixedOffset>,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub open_price_milli: i64,
    pub close_price_milli: i64,
    pub high_price_milli: i64,
    pub low_price_milli: i64,
    pub last_close_price_milli: Option<i64>,
    pub open_delta_raw: i64,
    pub close_delta_raw: i64,
    pub high_delta_raw: i64,
    pub low_delta_raw: i64,
    pub volume_raw: u32,
    pub volume_shares: u32,
    pub volume_lots: f64,
    pub amount_raw: u32,
    pub amount: f64,
    pub unknown_suffix_raw: [u8; 4],
    pub record_raw: Vec<u8>,
}
```


**DelistedBarsPage** · [定义](../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs#L87)

```rust
pub struct DelistedBarsPage {
    pub request: DelistedBarsRequest,
    pub records: Vec<DelistedBar>,
    pub source: CommandSource,
    pub raw_payload: Arc<[u8]>,
}
```


**DelistedBarsCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs#L195)

```rust
pub struct DelistedBarsCollection {
    pub pages: Vec<DelistedBarsPage>,
    pub records: Vec<DelistedBar>,
    pub raw_record_count: usize,
    pub termination: CommandTermination,
    pub complete: bool,
    pub atomic: bool,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl DelistedBarsRequest

pub fn new(market: u16, code: &str, start: u16, count: u16) -> io::Result<Self>;


pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn start(&self) -> u16;


pub fn count(&self) -> u16;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<DelistedBarsPage>;


// free function

pub fn decode_delisted_amount(raw: u32) -> f64;


pub fn decode_delisted_bars(
    request: &DelistedBarsRequest,
    bytes: &[u8],
) -> io::Result<DelistedBarsPage>;


pub fn delisted_bars_all(
    tcp: &mut Tcp,
    mut request: DelistedBarsRequest,
    options: CommandScanOptions,
) -> io::Result<DelistedBarsCollection>;


```


### src/tcp/stock/enhanced_history_transaction.rs


**EnhancedHistoryTransactionRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs#L31)

```rust
pub struct EnhancedHistoryTransactionRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**EnhancedTradeSide** · [定义](../../crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs#L103)

```rust
pub enum EnhancedTradeSide {
    Buy,
    Sell,
    Neutral,
    Status(i32),
}
```


**EnhancedTradeEventKind** · [定义](../../crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs#L112)

```rust
pub enum EnhancedTradeEventKind {
    Trade,
    OpeningMatch,
    AuctionSnapshot,
}
```


**EnhancedHistoryTransactionData** · [定义](../../crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs#L120)

```rust
pub struct EnhancedHistoryTransactionData {
    pub index: u16,
    pub absolute_index: u32,
    pub time_minutes: u16,
    pub time: String,
    pub trade_datetime: NaiveDateTime,
    pub price: f64,
    pub price_delta_raw: i32,
    pub price_acc_raw: i64,
    pub volume: i32,
    pub order_count: i32,
    pub status_raw: i32,
    pub reserved_raw: i32,
}
```


**EnhancedHistoryTransactionPage** · [定义](../../crates/rustdx-core/src/tcp/stock/enhanced_history_transaction.rs#L180)

```rust
pub struct EnhancedHistoryTransactionPage {
    pub market: u16,
    pub code: String,
    pub date: u32,
    pub start: u16,
    pub request_count: u16,
    pub records: Vec<EnhancedHistoryTransactionData>,
    pub price_base_raw_f32: f32,
    pub price_base_raw_bytes: [u8; 4],
    pub raw_payload: Vec<u8>,
    pub source_endpoint: Option<SocketAddr>,
    pub fetched_at: Option<DateTime<Utc>>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'a> EnhancedHistoryTransactionRequest<'a>

pub fn new(market: u16, code: &'a str, start: u16, count: u16, date: u32) -> io::Result<Self>;


pub fn market(self) -> u16;


pub fn code(self) -> &'a str;


pub fn start(self) -> u16;


pub fn count(self) -> u16;


pub fn date(self) -> u32;


pub fn encode(self) -> [u8; 28];


pub fn recv(self, tcp: &mut Tcp) -> io::Result<EnhancedHistoryTransactionPage>;


// impl EnhancedHistoryTransactionData

pub fn side(&self) -> EnhancedTradeSide;


pub fn event_kind(&self) -> EnhancedTradeEventKind;


pub fn is_actual_trade(&self) -> bool;


pub fn is_after_hours_fixed_price(&self) -> bool;


// impl EnhancedHistoryTransactionPage

pub fn raw_count(&self) -> usize;


pub fn actual_trades(&self) -> impl Iterator<Item = &EnhancedHistoryTransactionData>;


pub fn auction_snapshots(&self) -> impl Iterator<Item = &EnhancedHistoryTransactionData>;


pub fn opening_matches(&self) -> impl Iterator<Item = &EnhancedHistoryTransactionData>;


pub fn after_hours_trades(&self) -> impl Iterator<Item = &EnhancedHistoryTransactionData>;


pub fn next_start(&self) -> io::Result<Option<u16>>;


// free function

pub fn decode_enhanced_history_transactions(
    request: &EnhancedHistoryTransactionRequest<'_>,
    bytes: &[u8],
) -> io::Result<EnhancedHistoryTransactionPage>;


```


### src/tcp/stock/extended_quote.rs


**QuoteLevel** · [定义](../../crates/rustdx-core/src/tcp/stock/extended_quote.rs#L8)

```rust
pub struct QuoteLevel {
    pub price: Option<f64>,
    pub volume_raw: i64,
    pub price_delta_raw: i64,
}
```


**BasicQuote** · [定义](../../crates/rustdx-core/src/tcp/stock/extended_quote.rs#L14)

```rust
pub struct BasicQuote {
    pub market_raw: u8,
    pub code: String,
    pub active_raw: u16,
    pub price: Option<f64>,
    pub previous_close: Option<f64>,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub price_fields_raw: [i64; 5],
    pub time_raw: Option<i64>,
    pub update_cursor: Option<u32>,
    pub status_or_unknown_raw: i64,
    pub total_volume_raw: i64,
    pub current_volume_raw: i64,
    pub amount: f64,
    pub amount_raw: u32,
    pub inside_volume_raw: i64,
    pub outside_volume_raw: i64,
    pub after_outside_raw: i64,
    pub open_amount_raw: i64,
    pub bids: Vec<QuoteLevel>,
    pub asks: Vec<QuoteLevel>,
    pub scale: Option<PriceScale>,
    pub tail_raw: Vec<u8>,
}
```


### src/tcp/stock/finance_info.rs


**FinanceInfo** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_info.rs#L23)

```rust
pub struct FinanceInfo<'d> {
    pub send: Box<[u8]>,
    pub market: u8,
    pub code: &'d str,
    pub response: Vec<u8>,
    pub data: Vec<super::FinanceSnapshot>,
}
```


**FinanceInfoData** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_info.rs#L119)

```rust
pub struct FinanceInfoData {
    pub market: u8,
    pub code: String,
    pub liutongguben: f64,
    pub province: u16,
    pub industry: u16,
    pub updated_date: u32,
    pub ipo_date: u32,
    pub zongguben: f64,
    pub guojiagu: f64,
    pub faqirenfarengu: f64,
    pub farengu: f64,
    pub bgu: f64,
    pub hgu: f64,
    pub zhigonggu: f64,
    pub zongzichan: f64,
    pub liudongzichan: f64,
    pub gudingzichan: f64,
    pub wuxingzichan: f64,
    pub gudongrenshu: f64,
    pub liudongfuzhai: f64,
    pub changqifuzhai: f64,
    pub zibengongjijin: f64,
    pub jingzichan: f64,
    pub zhuyingshouru: f64,
    pub zhuyinglirun: f64,
    pub yingshouzhangkuan: f64,
    pub yingyelirun: f64,
    pub touzishouyu: f64,
    pub jingyingxianjinliu: f64,
    pub zongxianjinliu: f64,
    pub cunhuo: f64,
    pub lirunzonghe: f64,
    pub shuihoulirun: f64,
    pub jinglirun: f64,
    pub weifenpeilirun: f64,
    pub meigujingzichan: f64,
    pub baoliu2: f64,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> FinanceInfo<'d>

pub fn new(market: u8, code: &'d str) -> Self;


```


### src/tcp/stock/finance_raw.rs


**FinanceRaw** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_raw.rs#L11)

```rust
pub struct FinanceRaw {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl FinanceRaw

pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn body(&self) -> &[u8; FINANCE_BODY_SIZE];


pub fn raw_bits(&self, offset: usize) -> io::Result<u32>;


pub fn raw_f32(&self, offset: usize) -> io::Result<f32>;


pub fn province(&self) -> u16;


pub fn industry(&self) -> u16;


pub fn updated_date_raw(&self) -> u32;


pub fn ipo_date_raw(&self) -> u32;


pub fn updated_date(&self) -> Option<NaiveDate>;


pub fn ipo_date(&self) -> Option<NaiveDate>;


// free function

pub fn decode_finance_raw(
    request: &FinanceRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<FinanceRaw>>;


```


### src/tcp/stock/finance_semantics.rs


**FinanceField** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L11)

```rust
pub enum FinanceField {
    CirculatingShares,
    TotalShares,
    Slot01,
    Slot02,
    Slot03,
    Slot04,
    Slot05,
    ReportedEps,
    TotalAssets,
    CurrentAssets,
    FixedAssets,
    IntangibleAssets,
    ShareholderCount,
    CurrentLiabilities,
    LongTermLiabilities,
    CapitalReserve,
    NetAssets,
    OperatingRevenue,
    Slot17,
    Receivables,
    OperatingProfit,
    InvestmentIncome,
    OperatingCashFlow,
    CashFlowNetChange,
    Inventory,
    ProfitBeforeTax,
    AfterTaxProfit,
    NetProfit,
    RetainedProfit,
    NetAssetsPerShare,
    Slot29,
}
```


**FinanceUnit** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L105)

```rust
pub enum FinanceUnit {
    Shares,
    Yuan,
    YuanPerShare,
    Count,
    Unknown,
}
```


**FinanceSemanticStatus** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L126)

```rust
pub enum FinanceSemanticStatus {
    Verified,
    BaselineCandidate,
    Unknown,
}
```


**FinanceFieldValue** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L133)

```rust
pub struct FinanceFieldValue {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**FinanceProfile** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L176)

```rust
pub struct FinanceProfile {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**FinanceSnapshot** · [定义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L228)

```rust
pub struct FinanceSnapshot {
    pub semantic_schema_version: u32,
    pub profile_version: String,
    pub report_period: Option<NaiveDate>,
    pub eps_basis: &'static str,
    pub net_profit_basis: &'static str,
    pub net_assets_basis: &'static str,
    pub fetched_at: Option<DateTime<Utc>>,
    pub source_endpoint: Option<SocketAddr>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl FinanceField

pub fn body_offset(self) -> usize;


// impl FinanceFieldValue

pub fn field(&self) -> FinanceField;


pub fn raw_bits(&self) -> u32;


pub fn raw_value(&self) -> f32;


pub fn unit(&self) -> FinanceUnit;


pub fn semantic_status(&self) -> FinanceSemanticStatus;


pub fn value(&self) -> Option<f64>;


pub fn candidate_value(&self) -> Option<f64>;


pub fn evidence(&self) -> Option<&str>;


// impl FinanceProfile

pub fn new(
        market: u16,
        code: &str,
        updated_date: u32,
        report_period: NaiveDate,
        evidence: &str,
    ) -> io::Result<Self>;


pub fn verify(mut self, field: FinanceField, expected_raw_bits: u32) -> io::Result<Self>;


// impl FinanceSnapshot

pub fn raw(&self) -> &FinanceRaw;


pub fn code(&self) -> &str;


pub fn market(&self) -> u16;


pub fn field(&self, field: FinanceField) -> &FinanceFieldValue;


pub fn fields(&self) -> &[FinanceFieldValue];


pub fn total_shares(&self) -> Option<f64>;


pub fn circulating_shares(&self) -> Option<f64>;


pub fn total_assets_yuan(&self) -> Option<f64>;


pub fn net_profit_yuan(&self) -> Option<f64>;


pub fn reported_eps(&self) -> Option<f64>;


// free function

pub fn normalize_finance(raw: FinanceRaw, profile: Option<&FinanceProfile>) -> FinanceSnapshot;


pub fn decode_finance(
    request: &FinanceRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<FinanceSnapshot>>;


```


### src/tcp/stock/history_minute_time.rs


**HistoryMinuteTime** · [定义](../../crates/rustdx-core/src/tcp/stock/history_minute_time.rs#L26)

```rust
pub struct HistoryMinuteTime<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub date: u32,
    pub response: Vec<u8>,
    pub data: Vec<super::minute_time::MinuteTimeData>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> HistoryMinuteTime<'d>

pub fn new(market: u16, code: &'d str, date: u32) -> Self;


```


### src/tcp/stock/history_transaction.rs


**HistoryTransaction** · [定义](../../crates/rustdx-core/src/tcp/stock/history_transaction.rs#L25)

```rust
pub struct HistoryTransaction<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub start: u16,
    pub count: u16,
    pub date: u32,
    pub response: Vec<u8>,
    pub data: Vec<super::transaction::TransactionData>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> HistoryTransaction<'d>

pub fn new(market: u16, code: &'d str, start: u16, count: u16, date: u32) -> Self;


```


### src/tcp/stock/index_kline.rs


**IndexKlineData** · [定义](../../crates/rustdx-core/src/tcp/stock/index_kline.rs#L122)

```rust
pub struct IndexKlineData<'d> {
    pub dt: crate::tcp::helper::DateTime,
    pub code: &'d str,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub vol: f64,
    pub amount: f64,
    pub up_count: u16,
    pub down_count: u16,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> IndexKline<'d>

pub fn from_request(request: super::KlineRequest<'d>) -> Self;


pub fn new(market: u16, code: &'d str, category: u16, start: u16, count: u16) -> Self;


```


### src/tcp/stock/intraday_aux.rs


**IntradayAuxKind** · [定义](../../crates/rustdx-core/src/tcp/stock/intraday_aux.rs#L9)

```rust
pub enum IntradayAuxKind {
    BuySellStrength,
    VolumeComparison,
    Raw(u8),
}
```


**IntradayAuxRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/intraday_aux.rs#L24)

```rust
pub struct IntradayAuxRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**IntradayAuxPoint** · [定义](../../crates/rustdx-core/src/tcp/stock/intraday_aux.rs#L50)

```rust
pub enum IntradayAuxPoint {
    IntegerPair {
        index: u16,
        series_a: i64,
        series_b: i64,
    },
    VolumePair {
        index: u16,
        previous_day: f32,
        current_day: f32,
        previous_bits: u32,
        current_bits: u32,
    },
}
```


**IntradayAuxSeries** · [定义](../../crates/rustdx-core/src/tcp/stock/intraday_aux.rs#L65)

```rust
pub struct IntradayAuxSeries {
    pub request: IntradayAuxRequest,
    pub page: CommandPage<IntradayAuxPoint>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl IntradayAuxKind

pub fn selector(self) -> u8;


// impl IntradayAuxRequest

pub fn new(market: u16, code: &str, kind: IntradayAuxKind) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<IntradayAuxSeries>;


// free function

pub fn decode_intraday_aux(
    request: &IntradayAuxRequest,
    bytes: &[u8],
) -> io::Result<IntradayAuxSeries>;


```


### src/tcp/stock/kline.rs


**KlineData** · [定义](../../crates/rustdx-core/src/tcp/stock/kline.rs#L161)

```rust
pub struct KlineData<'d> {
    pub dt: DateTime,
    pub code: &'d str,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub vol: f64,
    pub amount: f64,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> Kline<'d>

pub fn from_request(request: super::KlineRequest<'d>) -> Self;


pub fn new(market: u16, code: &'d str, category: u16, start: u16, count: u16) -> Self;


pub fn market(&mut self, market: u16) -> &mut Self;


pub fn code(&mut self, code: &'d str) -> &mut Self;


pub fn category(&mut self, category: u16) -> &mut Self;


pub fn start(&mut self, start: u16) -> &mut Self;


pub fn count(&mut self, count: u16) -> &mut Self;


```


### src/tcp/stock/kline_request.rs


**KlineRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/kline_request.rs#L29)

```rust
pub struct KlineRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'a> KlineRequest<'a>

pub fn new(
        market: u16,
        code: &'a str,
        category: u16,
        start: u16,
        count: u16,
    ) -> io::Result<Self>;


pub fn market(self) -> u16;


pub fn code(self) -> &'a str;


pub fn category(self) -> u16;


pub fn start(self) -> u16;


pub fn count(self) -> u16;


pub fn for_symbol(
        symbol: &'a crate::data::Symbol,
        period: crate::data::Period,
        start: u16,
        count: u16,
    ) -> io::Result<Self>;


pub fn decode_canonical_stock(
        self,
        bytes: &[u8],
        unit: crate::data::VolumeUnit,
    ) -> io::Result<Vec<crate::data::CanonicalBar>>;


pub fn recv_canonical_stock(
        self,
        tcp: &mut Tcp,
        unit: crate::data::VolumeUnit,
    ) -> io::Result<Vec<crate::data::CanonicalBar>>;


pub fn decode_canonical_index(
        self,
        bytes: &[u8],
        unit: crate::data::VolumeUnit,
    ) -> io::Result<Vec<crate::data::CanonicalBar>>;


pub fn recv_canonical_index(
        self,
        tcp: &mut Tcp,
        unit: crate::data::VolumeUnit,
    ) -> io::Result<Vec<crate::data::CanonicalBar>>;


pub fn encode(self) -> [u8; 38];


pub fn recv_stock(self, tcp: &mut Tcp) -> io::Result<Vec<KlineData<'a>>>;


pub fn recv_index(self, tcp: &mut Tcp) -> io::Result<Vec<IndexKlineData<'a>>>;


// free function

pub fn decode_stock_bars<'a>(
    request: &KlineRequest<'a>,
    bytes: &[u8],
) -> io::Result<Vec<KlineData<'a>>>;


pub fn decode_index_bars<'a>(
    request: &KlineRequest<'a>,
    bytes: &[u8],
) -> io::Result<Vec<IndexKlineData<'a>>>;


```


### src/tcp/stock/minute_time.rs


**MinuteTime** · [定义](../../crates/rustdx-core/src/tcp/stock/minute_time.rs#L24)

```rust
pub struct MinuteTime<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub response: Vec<u8>,
    pub data: Vec<MinuteTimeData>,
}
```


**MinuteTimeData** · [定义](../../crates/rustdx-core/src/tcp/stock/minute_time.rs#L153)

```rust
pub struct MinuteTimeData {
    pub price: f64,
    pub vol: i32,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> MinuteTime<'d>

pub fn new(market: u16, code: &'d str) -> Self;


```


### src/tcp/stock/money_flow.rs


**MoneyFlowRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/money_flow.rs#L9)

```rust
pub struct MoneyFlowRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**MoneyFlowDaily** · [定义](../../crates/rustdx-core/src/tcp/stock/money_flow.rs#L32)

```rust
pub struct MoneyFlowDaily {
    pub date_raw: u32,
    pub raw: [u32; 21],
    pub total_amount: f32,
    pub total_amount_bits: u32,
    pub buckets_raw: [u16; 16],
    pub net_amounts: [f64; 4],
    pub active_buy_net_amounts: [f64; 4],
    pub main_net: f64,
    pub main_ratio: f64,
    pub active_buy_net: f64,
    pub active_buy_ratio: f64,
}
```


**MoneyFlowBlock** · [定义](../../crates/rustdx-core/src/tcp/stock/money_flow.rs#L48)

```rust
pub struct MoneyFlowBlock {
    pub request: MoneyFlowRequest,
    pub market_raw: u8,
    pub code: String,
    pub reported_count_raw: u16,
    pub header_raw: Vec<u8>,
    pub page: CommandPage<MoneyFlowDaily>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl MoneyFlowRequest

pub fn new(market: u16, code: &str) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<MoneyFlowBlock>;


// free function

pub fn decode_money_flow(request: &MoneyFlowRequest, bytes: &[u8]) -> io::Result<MoneyFlowBlock>;


```


### src/tcp/stock/price_scale.rs


**PriceScale** · [定义](../../crates/rustdx-core/src/tcp/stock/price_scale.rs#L6)

```rust
pub struct PriceScale {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl PriceScale

pub fn from_metadata(decimal: u8) -> io::Result<Self>;


pub fn decimal(self) -> u8;


pub fn divisor(self) -> u32;


```


### src/tcp/stock/quote_metadata.rs


**MarketPrecision** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_metadata.rs#L13)

```rust
pub struct MarketPrecision {
    pub market: u16,
    pub fetched_at: DateTime<Utc>,
    pub as_of_date: NaiveDate,
    pub source_endpoint: Option<SocketAddr>,
    pub securities: usize,
    pub unsupported: usize,
}
```


**PrecisionSnapshot** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_metadata.rs#L23)

```rust
pub struct PrecisionSnapshot {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**QuoteMetadataStore** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_metadata.rs#L98)

```rust
pub struct QuoteMetadataStore {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**QuotePrecision** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_metadata.rs#L182)

```rust
pub struct QuotePrecision {
    pub market: u16,
    pub code: String,
    pub decimal: u8,
    pub source_endpoint: Option<SocketAddr>,
}
```


**QuotePage** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_metadata.rs#L190)

```rust
pub struct QuotePage {
    pub records: Vec<QuoteData>,
    pub requested_codes: Vec<(u16, String)>,
    pub returned_codes: Vec<(u16, String)>,
    pub missing_codes: Vec<(u16, String)>,
    pub precision: Vec<QuotePrecision>,
    pub metadata_generation: u64,
    pub fetched_at: DateTime<Utc>,
    pub source_endpoint: Option<SocketAddr>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl PrecisionSnapshot

pub fn from_decimals(
        entries: &[(u16, &str, u8)],
        fetched_at: DateTime<Utc>,
        source_endpoint: Option<SocketAddr>,
    ) -> io::Result<Self>;


pub fn generation(&self) -> u64;


pub fn status(&self) -> Vec<MarketPrecision>;


pub fn is_fresh(&self, market: u16, at: DateTime<Utc>) -> bool;


pub fn scale(&self, market: u16, code: &str) -> io::Result<PriceScale>;


// impl QuoteMetadataStore

pub fn snapshot(&self) -> io::Result<Arc<PrecisionSnapshot>>;


pub fn replace(&self, mut value: PrecisionSnapshot) -> io::Result<Arc<PrecisionSnapshot>>;


```


### src/tcp/stock/quote_refresh.rs


**RefreshCursor** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs#L9)

```rust
pub struct RefreshCursor {
    pub symbol: CommandSymbol,
    pub cursor: u32,
}
```


**RefreshRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs#L14)

```rust
pub struct RefreshRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**QuoteRefresh** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs#L58)

```rust
pub struct QuoteRefresh {
    pub requested: Vec<CommandSymbol>,
    pub page: CommandPage<BasicQuote>,
    pub decoded_payload: Arc<[u8]>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl RefreshRequest

pub fn new(items: Vec<RefreshCursor>) -> io::Result<Self>;


pub fn from_stocks(stocks: &[(u16, &str)]) -> io::Result<Self>;


pub fn items(&self) -> &[RefreshCursor];


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: Tcp, metadata: Arc<PrecisionSnapshot>) -> io::Result<QuoteRefresh>;


// free function

pub fn decode_quote_refresh(
    request: &RefreshRequest,
    bytes: &[u8],
    metadata: &PrecisionSnapshot,
) -> io::Result<QuoteRefresh>;


```


### src/tcp/stock/quote_subscription.rs


**SubscriptionOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L21)

```rust
pub struct SubscriptionOptions {
    pub max_queue_frames: usize,
    pub max_queue_bytes: usize,
    pub refresh_timeout: Duration,
}
```


**DepthSyncStatus** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L50)

```rust
pub enum DepthSyncStatus {
    AwaitingBaseline,
    Synchronized,
    Invalid,
    Closed,
}
```


**DepthState** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L57)

```rust
pub struct DepthState {
    pub symbol: CommandSymbol,
    pub quote: Option<BasicQuote>,
    pub cursor: u32,
    pub received_at: Option<DateTime<Utc>>,
}
```


**DepthSnapshot** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L64)

```rust
pub struct DepthSnapshot {
    pub status: DepthSyncStatus,
    pub connection_id: Option<u64>,
    pub generation: u64,
    pub states: Vec<DepthState>,
    pub queued_frames: usize,
    pub queued_bytes: usize,
}
```


**QuotePush** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L73)

```rust
pub struct QuotePush {
    pub message_id: u32,
    pub connection_id: u64,
    pub generation: u64,
    pub received_at: DateTime<Utc>,
    pub header: [u8; 16],
    pub wire_body: Arc<[u8]>,
    pub refresh: QuoteRefresh,
}
```


**PushObserver** · [定义](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L83)

```rust
pub trait PushObserver: std::fmt::Debug + Send + Sync {
    fn observe(&self, endpoint: SocketAddr, push: &QuotePush) -> io::Result<()>;
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl QuoteSubscription

pub fn with_config(
        config: &TcpConfig,
        stocks: &[(u16, &str)],
        metadata: Arc<PrecisionSnapshot>,
        options: SubscriptionOptions,
    ) -> io::Result<Self>;


pub fn from_tcp(
        mut tcp: Tcp,
        request: RefreshRequest,
        metadata: Arc<PrecisionSnapshot>,
        options: SubscriptionOptions,
    ) -> io::Result<Self>;


pub fn set_push_observer(&mut self, observer: Option<Arc<dyn PushObserver>>);


pub fn snapshot(&self) -> DepthSnapshot;


pub fn refresh(&mut self) -> io::Result<QuoteRefresh>;


pub fn poll_push(&mut self, timeout: Duration) -> io::Result<Option<QuotePush>>;


pub fn rebuild(&mut self, metadata: Arc<PrecisionSnapshot>) -> io::Result<QuoteRefresh>;


pub fn close(&mut self);


```


### src/tcp/stock/quotes.rs


**SecurityQuotes** · [定义](../../crates/rustdx-core/src/tcp/stock/quotes.rs#L25)

```rust
pub struct SecurityQuotes<'d> {
    pub send: Box<[u8]>,
    pub stocks: Vec<(u16, &'d str)>,
    pub response: Vec<u8>,
    pub data: Vec<QuoteData>,
}
```


**QuoteData** · [定义](../../crates/rustdx-core/src/tcp/stock/quotes.rs#L394)

```rust
pub struct QuoteData {
    pub code: String,
    pub price: f64,
    pub last_close: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub vol: f64,
    pub cur_vol: f64,
    pub amount: f64,
    pub s_vol: f64,
    pub b_vol: f64,
    pub change: f64,
    pub change_percent: f64,
    pub speed: f64,
    pub servertime: String,
    pub bid1: f64,
    pub ask1: f64,
    pub bid1_vol: f64,
    pub ask1_vol: f64,
    pub bid2: f64,
    pub ask2: f64,
    pub bid2_vol: f64,
    pub ask2_vol: f64,
    pub bid3: f64,
    pub ask3: f64,
    pub bid3_vol: f64,
    pub ask3_vol: f64,
    pub bid4: f64,
    pub ask4: f64,
    pub bid4_vol: f64,
    pub ask4_vol: f64,
    pub bid5: f64,
    pub ask5: f64,
    pub bid5_vol: f64,
    pub ask5_vol: f64,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> SecurityQuotes<'d>

pub fn new(stocks: Vec<(u16, &'d str)>) -> Self;


pub fn with_precision(mut self, precision: Arc<PrecisionSnapshot>) -> Self;


pub fn try_parse_legacy(&mut self, bytes: Vec<u8>) -> std::io::Result<()>;


```


### src/tcp/stock/recent_minutes.rs


**MinuteDateUnavailableReason / MinuteDateQualification** · [定义](../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)

```rust
#[non_exhaustive]
pub enum MinuteDateUnavailableReason {
    ServerDoesNotEchoTradingDate,
}

#[non_exhaustive]
pub enum MinuteDateQualification {
    Unverified { reason: MinuteDateUnavailableReason },
}
```


**RecentMinutesRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)

```rust
pub struct RecentMinutesRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**MinutePoint** · [定义](../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)

```rust
pub struct MinutePoint {
    pub index: u16,
    pub time_label: String,
    pub price: f64,
    pub average_price: f64,
    pub volume_raw: i64,
    pub price_field_raw: i64,
    pub average_field_raw: i64,
    pub price_raw: i64,
    pub average_raw: i64,
}
```


**MinuteSeries** · [定义](../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)

```rust
pub struct MinuteSeries {
    pub request: RecentMinutesRequest,
    pub requested_date: u32,
    pub server_trading_date: Option<u32>,
    // date_qualification私有；通过date_qualification()/require_trading_date()读取。
    pub date_selector_raw: u32,
    pub previous_close: f32,
    pub open_price: f32,
    pub previous_close_bits: u32,
    pub open_bits: u32,
    pub scale: PriceScale,
    pub page: CommandPage<MinutePoint>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl RecentMinutesRequest

pub fn new(market: u16, code: &str, requested_date: u32) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp, metadata: &PrecisionSnapshot) -> io::Result<MinuteSeries>;


// impl MinuteSeries

pub fn date_qualification(&self) -> MinuteDateQualification;


pub fn require_trading_date(&self) -> io::Result<u32>;

pub fn date_reference(&self) -> Option<&MinuteDateReference>;

pub fn with_date_reference(self, reference: MinuteDateReference) -> io::Result<Self>;


// free function

pub fn decode_recent_minutes(
    request: &RecentMinutesRequest,
    bytes: &[u8],
    metadata: &PrecisionSnapshot,
) -> io::Result<MinuteSeries>;


```


### src/tcp/stock/reported_turnover.rs


**TurnoverScope** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L30)

```rust
pub enum TurnoverScope {
    Minute1,
    Minute5,
    RegularWithAuctions,
    AfterHours,
    Combined,
}
```


**TurnoverAvailability** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L38)

```rust
pub enum TurnoverAvailability {
    Available,
    Partial,
    Unavailable,
}
```


**TurnoverUnavailableReason** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L44)

```rust
pub enum TurnoverUnavailableReason {
    UnconfirmedProfile,
    NoBarsForDate,
    NoIndependentAfterHoursSource,
}
```


**TurnoverSource** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L64)

```rust
pub struct TurnoverSource {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TurnoverQuery** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L116)

```rust
pub struct TurnoverQuery {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TurnoverProfile** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L171)

```rust
pub struct TurnoverProfile {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**ReportedTurnoverBar** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L225)

```rust
pub struct ReportedTurnoverBar {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**ReportedTurnoverSeries** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L273)

```rust
pub struct ReportedTurnoverSeries {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**ReportedTurnover** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L374)

```rust
pub struct ReportedTurnover {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TurnoverOutcome** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L391)

```rust
pub type TurnoverOutcome = ReportedTurnover;
```


**TurnoverOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs#L569)

```rust
pub struct TurnoverOptions {
    pub page_size: u16,
    pub max_pages: usize,
    pub max_records: usize,
    pub max_bytes: usize,
    pub timeout: Duration,
    pub page_delay: Duration,
    pub daily_category: u16,
    pub profile: Option<TurnoverProfile>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// free function

pub fn decode_nonnegative_binary32(raw: u32) -> io::Result<f64>;


// impl TurnoverSource

pub fn archived(
        endpoint: SocketAddr,
        connection_id: NonZeroU64,
        fetched_at: DateTime<Utc>,
        archive_id: &str,
    ) -> io::Result<Self>;


pub fn endpoint(&self) -> SocketAddr;


pub fn connection_id(&self) -> NonZeroU64;


pub fn fetched_at(&self) -> DateTime<Utc>;


pub fn archive_id(&self) -> Option<&str>;


// impl TurnoverQuery

pub fn new(
        market: u16,
        code: &str,
        date: u32,
        scope: TurnoverScope,
        category: u16,
    ) -> io::Result<Self>;


pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn date(&self) -> u32;


pub fn scope(&self) -> TurnoverScope;


pub fn category(&self) -> u16;


// impl TurnoverProfile

pub fn a_share(
        market: u16,
        code: &str,
        date: u32,
        endpoint: SocketAddr,
        evidence: &str,
    ) -> io::Result<Self>;


pub fn evidence(&self) -> &str;


pub fn endpoint(&self) -> SocketAddr;


pub fn quantity_unit(&self, category: u16) -> Option<TradeQuantityUnit>;


// impl ReportedTurnoverBar

pub fn date(&self) -> u32;


pub fn time_minutes(&self) -> u16;


pub fn wire_index(&self) -> usize;


pub fn byte_range(&self) -> Range<usize>;


pub fn raw_volume_u32(&self) -> u32;


pub fn raw_amount_u32(&self) -> u32;


pub fn reported_amount_yuan(&self) -> f64;


pub fn reported_volume_raw(&self) -> f64;


pub fn amount_encoding_step_yuan(&self) -> f64;


// impl ReportedTurnoverSeries

pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn category(&self) -> u16;


pub fn start(&self) -> u16;


pub fn request_count(&self) -> u16;


pub fn bars(&self) -> &[ReportedTurnoverBar];


pub fn payload(&self) -> &[u8];


pub fn source(&self) -> Option<&TurnoverSource>;


// free function

pub fn decode_reported_turnover(
    request: &KlineRequest<'_>,
    payload: &[u8],
    source: Option<TurnoverSource>,
) -> io::Result<ReportedTurnoverSeries>;


// impl ReportedTurnover

pub fn from_pages(
        query: TurnoverQuery,
        pages: Vec<ReportedTurnoverSeries>,
        profile: Option<&TurnoverProfile>,
    ) -> io::Result<Self>;


pub fn query(&self) -> &TurnoverQuery;


pub fn availability(&self) -> TurnoverAvailability;


pub fn unavailable_reason(&self) -> Option<TurnoverUnavailableReason>;


pub fn reported_amount_yuan(&self) -> Option<f64>;


pub fn regular_component_reported_amount_yuan(&self) -> Option<f64>;


pub fn regular_component_availability(&self) -> TurnoverAvailability;


pub fn quantity_unit(&self) -> Option<TradeQuantityUnit>;


pub fn sampling(&self) -> TurnoverSampling;


pub fn profile(&self) -> Option<&TurnoverProfile>;


pub fn date_basis(&self) -> TurnoverDateBasis;


pub fn summation(&self) -> TurnoverSummation;


pub fn bars(&self) -> &[ReportedTurnoverBar];


pub fn pages(&self) -> &[ReportedTurnoverSeries];


```


### src/tcp/stock/requests.rs


**QuotesRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/requests.rs#L55)

```rust
pub struct QuotesRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**FinanceRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/requests.rs#L205)

```rust
pub struct FinanceRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TransactionRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/requests.rs#L308)

```rust
pub struct TransactionRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**HistoryTransactionRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/requests.rs#L360)

```rust
pub struct HistoryTransactionRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'a> QuotesRequest<'a>

pub fn new(stocks: Vec<(u16, &'a str)>) -> io::Result<Self>;


pub fn stocks(&self) -> &[(u16, &'a str)];


pub fn encode(&self) -> Vec<u8>;


pub fn recv(&self, tcp: &mut Tcp, precision: &PrecisionSnapshot) -> io::Result<Vec<QuoteData>>;


pub fn recv_legacy(&self, tcp: &mut Tcp) -> io::Result<Vec<QuoteData>>;


pub fn recv_with_metadata(
        &self,
        tcp: &mut Tcp,
        precision: &PrecisionSnapshot,
    ) -> io::Result<QuotePage>;


// free function

pub fn decode_quotes(
    request: &QuotesRequest<'_>,
    bytes: &[u8],
    precision: &PrecisionSnapshot,
) -> io::Result<Vec<QuoteData>>;


pub fn decode_quotes_with_metadata(
    request: &QuotesRequest<'_>,
    bytes: &[u8],
    precision: &PrecisionSnapshot,
) -> io::Result<QuotePage>;


pub fn decode_quotes_legacy(
    request: &QuotesRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<QuoteData>>;


// impl<'a> FinanceRequest<'a>

pub fn new(market: u16, code: &'a str) -> io::Result<Self>;


pub fn market(self) -> u16;


pub fn code(self) -> &'a str;


pub fn encode(self) -> [u8; 21];


pub fn recv(self, tcp: &mut Tcp) -> io::Result<Vec<FinanceSnapshot>>;


pub fn recv_legacy(self, tcp: &mut Tcp) -> io::Result<Vec<FinanceInfoData>>;


// free function

pub fn decode_finance_legacy(
    request: &FinanceRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<FinanceInfoData>>;


// impl<'a> TransactionRequest<'a>

pub fn new(market: u16, code: &'a str, start: u16, count: u16) -> io::Result<Self>;


pub fn market(self) -> u16;


pub fn code(self) -> &'a str;


pub fn start(self) -> u16;


pub fn count(self) -> u16;


pub fn encode(self) -> [u8; 24];


pub fn recv(self, tcp: &mut Tcp) -> io::Result<Vec<TransactionData>>;


// impl<'a> HistoryTransactionRequest<'a>

pub fn new(market: u16, code: &'a str, start: u16, count: u16, date: u32) -> io::Result<Self>;


pub fn market(self) -> u16;


pub fn code(self) -> &'a str;


pub fn start(self) -> u16;


pub fn count(self) -> u16;


pub fn date(self) -> u32;


pub fn encode(self) -> [u8; 28];


pub fn recv(self, tcp: &mut Tcp) -> io::Result<Vec<TransactionData>>;


// free function

pub fn decode_transactions(
    request: &TransactionRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<TransactionData>>;


pub fn decode_history_transactions(
    request: &HistoryTransactionRequest<'_>,
    bytes: &[u8],
) -> io::Result<Vec<TransactionData>>;


```


### src/tcp/stock/resource_metadata.rs


**BlockMetaRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/resource_metadata.rs#L10)

```rust
pub struct BlockMetaRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**BlockMetadata** · [定义](../../crates/rustdx-core/src/tcp/stock/resource_metadata.rs#L44)

```rust
pub struct BlockMetadata {
    pub size: u32,
    pub md5: String,
    pub reserved_raw: u8,
    pub md5_raw: [u8; 32],
    pub terminator_raw: u8,
    pub trailing_payload: Vec<u8>,
    pub source: CommandSource,
    pub raw_payload: Arc<[u8]>,
}
```

**BlockFile / BlockFileChunk** · [定义](../../crates/rustdx-core/src/tcp/stock/block_info.rs)

```rust
pub struct BlockFile {
    pub filename: String,
    pub metadata: BlockMetadata,
    pub content: Vec<u8>,
    pub computed_md5: String,
    pub chunks: Vec<BlockFileChunk>,
}

pub struct BlockFileChunk {
    pub start: u32,
    pub length: u32,
    pub header_raw: [u8; 4],
    pub source: CommandSource,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl BlockMetaRequest

pub fn new(filename: &str) -> io::Result<Self>;


pub fn filename(&self) -> &str;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<BlockMetadata>;


// free function

pub fn decode_block_metadata(bytes: &[u8]) -> io::Result<BlockMetadata>;


```


### src/tcp/stock/security_directory.rs


**SecurityDirectoryRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/security_directory.rs#L12)

```rust
pub struct SecurityDirectoryRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**SecurityCategory** · [定义](../../crates/rustdx-core/src/tcp/stock/security_directory.rs#L52)

```rust
pub enum SecurityCategory {
    Index,
    Etf,
    AShare,
    BShare,
    Unknown,
}
```


**SecurityBoard** · [定义](../../crates/rustdx-core/src/tcp/stock/security_directory.rs#L61)

```rust
pub enum SecurityBoard {
    ShanghaiMain,
    Star,
    ShenzhenMain,
    Chinext,
    Unknown,
}
```


**SecurityDirectoryData** · [定义](../../crates/rustdx-core/src/tcp/stock/security_directory.rs#L70)

```rust
pub struct SecurityDirectoryData {
    pub market: u16,
    pub code: String,
    pub name: String,
    pub volume_unit: u16,
    pub decimal: u8,
    pub previous_close: f32,
    pub volume_ratio_base: f32,
    pub unknown0_raw: [u8; 4],
    pub previous_close_raw: [u8; 4],
    pub unknown3_raw: [u8; 4],
    pub category: SecurityCategory,
    pub board: SecurityBoard,
}
```


**SecurityDirectoryPage** · [定义](../../crates/rustdx-core/src/tcp/stock/security_directory.rs#L100)

```rust
pub struct SecurityDirectoryPage {
    pub market: u16,
    pub start: u32,
    pub request_count: u16,
    pub records: Vec<SecurityDirectoryData>,
    pub raw_payload: Vec<u8>,
    pub source_endpoint: Option<SocketAddr>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl SecurityDirectoryRequest

pub fn new(market: u16, start: u32, count: u16) -> io::Result<Self>;


pub fn encode(self) -> [u8; 26];


pub fn recv(self, tcp: &mut Tcp) -> io::Result<SecurityDirectoryPage>;


// impl SecurityDirectoryData

pub fn to_legacy(&self) -> super::SecurityListData;


// free function

pub fn decode_security_directory(
    request: &SecurityDirectoryRequest,
    bytes: &[u8],
) -> io::Result<SecurityDirectoryPage>;


pub fn security_directory(tcp: &mut Tcp, market: u16) -> io::Result<Vec<SecurityDirectoryData>>;


```


### src/tcp/stock/security_list.rs


**SecurityList** · [定义](../../crates/rustdx-core/src/tcp/stock/security_list.rs#L24)

```rust
pub struct SecurityList {
    pub send: Box<[u8]>,
    pub market: u16,
    pub start: u16,
    pub response: Vec<u8>,
    pub data: Vec<SecurityListData>,
}
```


**SecurityListData** · [定义](../../crates/rustdx-core/src/tcp/stock/security_list.rs#L162)

```rust
pub struct SecurityListData {
    pub code: String,
    pub name: String,
    pub volunit: u32,
    pub decimal_point: u8,
    pub pre_close: f64,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl SecurityList

pub fn new(market: u16, start: u16) -> Self;


```


### src/tcp/stock/snapshots.rs


**SnapshotsRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/snapshots.rs#L10)

```rust
pub struct SnapshotsRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**SnapshotPage** · [定义](../../crates/rustdx-core/src/tcp/stock/snapshots.rs#L46)

```rust
pub struct SnapshotPage {
    pub request: SnapshotsRequest,
    pub header_raw: u16,
    pub missing_codes: Vec<CommandSymbol>,
    pub page: CommandPage<BasicQuote>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl SnapshotsRequest

pub fn new(stocks: &[(u16, &str)]) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp, metadata: &PrecisionSnapshot) -> io::Result<SnapshotPage>;


// free function

pub fn decode_snapshots(
    request: &SnapshotsRequest,
    bytes: &[u8],
    metadata: &PrecisionSnapshot,
) -> io::Result<SnapshotPage>;


```


### src/tcp/stock/sparkline.rs


**SparklineOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/sparkline.rs#L8)

```rust
pub struct SparklineOptions {
    pub selector: u8,
    pub window: u16,
}
```


**SparklineRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/sparkline.rs#L21)

```rust
pub struct SparklineRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**SparklinePrice** · [定义](../../crates/rustdx-core/src/tcp/stock/sparkline.rs#L55)

```rust
pub struct SparklinePrice {
    pub price: f32,
    pub bits: u32,
}
```


**SparklineSeries** · [定义](../../crates/rustdx-core/src/tcp/stock/sparkline.rs#L60)

```rust
pub struct SparklineSeries {
    pub request: SparklineRequest,
    pub market_raw: u8,
    pub code: String,
    pub selector_echo: u8,
    pub parameter_raw: u32,
    pub capacity_raw: u16,
    pub base_price: f32,
    pub base_bits: u32,
    pub page: CommandPage<SparklinePrice>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl SparklineRequest

pub fn new(market: u16, code: &str, options: SparklineOptions) -> io::Result<Self>;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<SparklineSeries>;


// free function

pub fn decode_sparkline(request: &SparklineRequest, bytes: &[u8]) -> io::Result<SparklineSeries>;


```


### src/tcp/stock/special_limits.rs


**SpecialLimitPage** · [定义](../../crates/rustdx-core/src/tcp/stock/special_limits.rs#L8)

```rust
pub type SpecialLimitPage = CommandPage<SpecialLimitRecord>;
```


**SpecialLimitCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/special_limits.rs#L9)

```rust
pub type SpecialLimitCollection = CommandCollection<SpecialLimitRecord>;
```


**SpecialLimitsRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/special_limits.rs#L12)

```rust
pub struct SpecialLimitsRequest {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**SpecialLimitRecord** · [定义](../../crates/rustdx-core/src/tcp/stock/special_limits.rs#L35)

```rust
pub struct SpecialLimitRecord {
    pub market_raw: u8,
    pub code_number_raw: u32,
    pub code: String,
    pub upper_price: f32,
    pub lower_price: f32,
    pub upper_bits: u32,
    pub lower_bits: u32,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl SpecialLimitsRequest

pub fn new(start: u16) -> Self;


pub fn start(&self) -> u16;


pub fn encode(&self, id: u32) -> io::Result<Vec<u8>>;


pub fn recv(&self, tcp: &mut Tcp) -> io::Result<SpecialLimitPage>;


// free function

pub fn decode_special_limits(bytes: &[u8]) -> io::Result<SpecialLimitPage>;


```


### src/tcp/stock/today_minute.rs


**TodayMinuteRequest** · [定义](../../crates/rustdx-core/src/tcp/stock/today_minute.rs#L10)

```rust
pub struct TodayMinuteRequest<'a> {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TodayMinutePoint** · [定义](../../crates/rustdx-core/src/tcp/stock/today_minute.rs#L57)

```rust
pub struct TodayMinutePoint {
    pub index: u16,
    pub time_label: String,
    pub price: f64,
    pub average_price: f64,
    pub volume_raw: i32,
    pub price_field_raw: i32,
    pub average_field_raw: i32,
    pub price_raw: i64,
    pub average_raw: i64,
    pub byte_range: Range<usize>,
}
```


**TodayMinuteSeries** · [定义](../../crates/rustdx-core/src/tcp/stock/today_minute.rs#L74)

```rust
pub struct TodayMinuteSeries {
    pub market: u16,
    pub code: String,
    pub trading_date: Option<u32>,
    pub points: Vec<TodayMinutePoint>,
    pub reserved_raw: u16,
    pub scale: PriceScale,
    pub metadata_generation: u64,
    pub raw_payload: Vec<u8>,
    pub source_endpoint: Option<SocketAddr>,
    pub fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'a> TodayMinuteRequest<'a>

pub fn new(market: u16, code: &'a str) -> io::Result<Self>;


pub fn encode(self) -> [u8; 24];


pub fn recv(
        self,
        tcp: &mut Tcp,
        metadata: &PrecisionSnapshot,
    ) -> io::Result<TodayMinuteSeries>;


// free function

pub fn decode_today_minutes(
    request: &TodayMinuteRequest<'_>,
    bytes: &[u8],
    metadata: &PrecisionSnapshot,
) -> io::Result<TodayMinuteSeries>;


```


### src/tcp/stock/trade_amount.rs


**TradeQuantityUnit** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L15)

```rust
pub enum TradeQuantityUnit {
    Unknown,
    Shares,
    LotsOf100Shares,
}
```


**TradeAmountContext** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L47)

```rust
pub struct TradeAmountContext {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**AmountOrigin** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L111)

```rust
pub enum AmountOrigin {
    RecordPriceTimesQuantity,
    VendorReported,
}
```


**AmountAvailability** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L134)

```rust
pub enum AmountAvailability {
    Available,
    Partial,
    Unavailable,
}
```


**TradeAmountEstimate** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L148)

```rust
pub struct TradeAmountEstimate {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeAmountOutcome** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L186)

```rust
pub struct TradeAmountOutcome {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeEstimateOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L218)

```rust
pub struct TradeEstimateOptions {
    pub include_candidates: bool,
}
```


**TradeEstimateComponent** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L223)

```rust
pub struct TradeEstimateComponent {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeEstimateSummary** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs#L277)

```rust
pub struct TradeEstimateSummary {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl TradeAmountContext

pub fn unknown(query: TradeQuery) -> Self;


pub fn a_share(
        query: TradeQuery,
        quantity_unit: TradeQuantityUnit,
        evidence: &str,
    ) -> io::Result<Self>;


pub fn query(&self) -> &TradeQuery;


pub fn quantity_unit(&self) -> TradeQuantityUnit;


pub fn unit_evidence(&self) -> Option<&str>;


// impl TradeAmountEstimate

pub fn amount_origin(&self) -> AmountOrigin;


pub fn estimated_amount_fen(&self) -> i128;


pub fn estimated_amount_yuan(&self) -> f64;


pub fn quantity_shares(&self) -> i128;


// impl TradeAmountOutcome

pub fn record_index(&self) -> usize;


pub fn page_start(&self) -> u16;


pub fn phase(&self) -> TradePhase;


pub fn qualification(&self) -> AmountQualification;


pub fn estimate(&self) -> Option<&TradeAmountEstimate>;


pub fn unavailable_reason(&self) -> Option<AmountUnavailableReason>;


// impl TradeEstimateComponent

pub fn availability(&self) -> AmountAvailability;


pub fn unavailable_reason(&self) -> Option<AmountUnavailableReason>;


pub fn records(&self) -> usize;


pub fn quantity_shares(&self) -> i128;


pub fn estimated_amount_fen(&self) -> Option<i128>;


pub fn estimated_amount_yuan(&self) -> Option<f64>;


// impl TradeEstimateSummary

pub fn context(&self) -> &TradeAmountContext;


pub fn trade_coverage(&self) -> TradeCoverage;


pub fn date_basis(&self) -> TradeAmountDateBasis;


pub fn page_sources(&self) -> &[Option<super::TradeSource>];


pub fn regular_with_auctions(&self) -> &TradeEstimateComponent;


pub fn after_hours(&self) -> &TradeEstimateComponent;


pub fn candidates(&self) -> &TradeEstimateComponent;


pub fn raw_records(&self) -> usize;


pub fn excluded_records(&self) -> usize;


pub fn candidate_records(&self) -> usize;


pub fn unknown_records(&self) -> usize;


pub fn unavailable_records(&self) -> usize;


// impl TradePage

pub fn amount_estimates(
        &self,
        context: &TradeAmountContext,
    ) -> io::Result<Vec<TradeAmountOutcome>>;


// impl EnhancedHistoryTransactionPage

pub fn amount_estimates(
        &self,
        context: &TradeAmountContext,
    ) -> io::Result<Vec<TradeAmountOutcome>>;


// impl TradeCollection

pub fn amount_estimate_summary(
        &self,
        context: &TradeAmountContext,
        options: TradeEstimateOptions,
    ) -> io::Result<TradeEstimateSummary>;


// impl TransactionCollection

pub fn amount_estimate_summary(
        &self,
        context: &TradeAmountContext,
        options: TradeEstimateOptions,
    ) -> io::Result<TradeEstimateSummary>;


```


### src/tcp/stock/trade_amount_reconcile.rs


**TradeAmountInterval** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount_reconcile.rs#L25)

```rust
pub struct TradeAmountInterval {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeAmountReconciliation** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_amount_reconcile.rs#L51)

```rust
pub struct TradeAmountReconciliation {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl TradeAmountInterval

pub fn time_minutes(&self) -> u16;


pub fn event_range(&self) -> Range<usize>;


pub fn quantity_shares(&self) -> i128;


pub fn estimate_minus_reported_yuan(&self) -> f64;


// impl TradeAmountReconciliation

pub fn estimates(&self) -> &TradeEstimateSummary;


pub fn alignment(&self) -> TradeAmountAlignment;


pub fn source_status(&self) -> ReconciliationSourceStatus;


pub fn full_scope_available(&self) -> bool;


pub fn intervals(&self) -> &[TradeAmountInterval];


pub fn failed_bar_index(&self) -> Option<usize>;


pub fn estimate_minus_minute_yuan(&self) -> Option<f64>;


pub fn minute_minus_day_yuan(&self) -> Option<f64>;


pub fn estimate_minus_day_yuan(&self) -> Option<f64>;


pub fn minute_encoding_step_sum_yuan(&self) -> f64;


// free function

pub fn reconcile_trade_amounts(
    trades: &TradeCollection,
    context: &TradeAmountContext,
    minute: &ReportedTurnover,
    daily: Option<&ReportedTurnover>,
) -> io::Result<TradeAmountReconciliation>;


```


### src/tcp/stock/trade_decode.rs


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// free function

pub fn decode_trade_page(
    query: &TradeQuery,
    start: u16,
    count: u16,
    bytes: &[u8],
) -> io::Result<TradePage>;


```


### src/tcp/stock/trade_models.rs


**TradeProtocol** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L18)

```rust
pub enum TradeProtocol {
    Today0fc5,
    EnhancedHistory0fc6,
    LegacyHistory0fb5,
}
```


**TradeQuery** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L28)

```rust
pub struct TradeQuery {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeEventKind** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L125)

```rust
pub enum TradeEventKind {
    AuctionSnapshot,
    OpeningMatch,
    Trade,
    Unknown,
}
```


**TradePhase** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L134)

```rust
pub enum TradePhase {
    AuctionProcess,
    Opening,
    ClosingMatch,
    AfterHoursFixedPrice,
    Regular,
    Unknown,
}
```


**TradeDirection** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L144)

```rust
pub enum TradeDirection {
    Buy,
    Sell,
    Neutral,
    Unknown,
}
```


**TradeQualification** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L154)

```rust
pub enum TradeQualification {
    ConfirmedByProfile,
    Candidate,
    ExcludedByProfile,
    Unknown,
}
```


**TradeRecord** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L165)

```rust
pub struct TradeRecord {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeSource** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L343)

```rust
pub struct TradeSource {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradePage** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L375)

```rust
pub struct TradePage {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


**TradeTermination** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L430)

```rust
pub enum TradeTermination {
    EmptyPageObserved,
    OffsetLimit,
    PageLimit,
    RecordLimit,
    ByteLimit,
    Deadline,
    Cancelled,
    ConnectionChanged,
    DateChanged,
    DecodeError { message: String },
    TransportError { message: String },
    RepeatedPage,
    CallerStopped,
}
```


**TradeCoverage** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L448)

```rust
pub enum TradeCoverage {
    SourceEnumerated,
    Partial,
    Unknown,
}
```


**TradeWarning** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L458)

```rust
pub enum TradeWarning {
    MissingSource,
    NonzeroStart,
    CurrentDayMayShift,
    TradingDateUnverified,
    LegacyClassificationCandidate,
    UnqualifiedEvents,
}
```


**TradeCounts** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L469)

```rust
pub struct TradeCounts {
    pub raw: usize,
    pub auction_snapshots: usize,
    pub opening_matches: usize,
    pub trades: usize,
    pub unknown_events: usize,
    pub confirmed: usize,
    pub candidates: usize,
    pub excluded: usize,
    pub unknown_qualification: usize,
    pub confirmed_unknown_direction: usize,
}
```


**TradeCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/trade_models.rs#L497)

```rust
pub struct TradeCollection {
    // 私有字段；通过下列构造器和只读方法访问。
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl TradeQuery

pub fn today(market: u16, code: &str) -> io::Result<Self>;


pub fn history(market: u16, code: &str, date: u32) -> io::Result<Self>;


pub fn history_legacy(market: u16, code: &str, date: u32) -> io::Result<Self>;


pub fn market(&self) -> u16;


pub fn code(&self) -> &str;


pub fn date(&self) -> Option<u32>;


pub fn protocol(&self) -> TradeProtocol;


pub fn encode(&self, start: u16, count: u16) -> io::Result<Vec<u8>>;


pub fn recv_page(
        &self,
        tcp: &mut crate::tcp::Tcp,
        start: u16,
        count: u16,
    ) -> io::Result<TradePage>;


// impl TradeRecord

pub fn time_minutes(&self) -> u16;


pub fn price_delta_raw(&self) -> i32;


pub fn price_raw(&self) -> i64;


pub fn price(&self) -> f64;


pub fn volume_raw(&self) -> i32;


pub fn trade_count_raw(&self) -> Option<i32>;


pub fn status_raw(&self) -> i32;


pub fn tail_raw(&self) -> i32;


pub fn byte_range(&self) -> Range<usize>;


pub fn protocol(&self) -> TradeProtocol;


pub fn event_kind(&self) -> TradeEventKind;


pub fn phase(&self) -> TradePhase;


pub fn direction(&self) -> TradeDirection;


pub fn qualification(&self) -> TradeQualification;


pub fn is_confirmed_trade(&self) -> bool;


pub fn to_legacy(&self) -> super::TransactionData;


// impl TradeSource

pub fn endpoint(&self) -> SocketAddr;


pub fn connection_id(&self) -> NonZeroU64;


pub fn fetched_at(&self) -> DateTime<Utc>;


pub fn observed_date(&self) -> chrono::NaiveDate;


// impl TradePage

pub fn query(&self) -> &TradeQuery;


pub fn start(&self) -> u16;


pub fn request_count(&self) -> u16;


pub fn raw_count(&self) -> usize;


pub fn records(&self) -> &[TradeRecord];


pub fn historical_prefix_raw(&self) -> Option<[u8; 4]>;


pub fn payload(&self) -> &[u8];


pub fn source(&self) -> Option<&TradeSource>;


pub fn record_bytes(&self, index: usize) -> Option<&[u8]>;


pub fn next_offset(&self) -> u32;


pub fn confirmed_trades(&self) -> impl Iterator<Item = &TradeRecord>;


// impl TradeCollection

pub fn new(
        query: TradeQuery,
        pages: Vec<TradePage>,
        termination: TradeTermination,
    ) -> io::Result<Self>;


pub fn query(&self) -> &TradeQuery;


pub fn pages(&self) -> &[TradePage];


pub fn termination(&self) -> &TradeTermination;


pub fn coverage(&self) -> TradeCoverage;


pub fn counts(&self) -> TradeCounts;


pub fn warnings(&self) -> &[TradeWarning];


pub fn time_range(&self) -> Option<(u16, u16)>;


pub fn payload_bytes(&self) -> usize;


pub fn records(&self) -> impl Iterator<Item = &TradeRecord>;


pub fn confirmed_trades(&self) -> impl Iterator<Item = &TradeRecord>;


pub fn next_offset(&self) -> u32;


```


### src/tcp/stock/transaction.rs


**Transaction** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction.rs#L25)

```rust
pub struct Transaction<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub start: u16,
    pub count: u16,
    pub response: Vec<u8>,
    pub data: Vec<TransactionData>,
}
```


**TransactionData** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction.rs#L119)

```rust
pub struct TransactionData {
    pub time: String,
    pub price: f64,
    pub vol: i32,
    pub num: i32,
    pub buyorsell: i32,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> Transaction<'d>

pub fn new(market: u16, code: &'d str, start: u16, count: u16) -> Self;


```


### src/tcp/stock/transaction_pagination.rs


**TransactionPaginationOptions** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs#L20)

```rust
pub struct TransactionPaginationOptions {
    pub page_size: u16,
    pub max_pages: usize,
    pub max_records: usize,
    pub max_bytes: usize,
    pub timeout: Duration,
    pub page_delay: Duration,
}
```


**TransactionPageInfo** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs#L55)

```rust
pub struct TransactionPageInfo {
    pub start: u16,
    pub request_count: u16,
    pub raw_count: usize,
    pub record_range: Range<usize>,
    pub payload_bytes: usize,
    pub requests: usize,
}
```


**TransactionCollection** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs#L74)

```rust
pub struct TransactionCollection {
    pub market: u16,
    pub code: String,
    pub command: u16,
    pub requested_date: Option<u32>,
    pub observed_date: u32,
    pub source_endpoint: SocketAddr,
    pub connection_generation: u64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub records: Vec<TransactionData>,
    pub pages: Vec<TransactionPageInfo>,
    pub event_pages: Vec<super::TradePage>,
    pub requests: usize,
    pub payload_bytes: usize,
    pub termination: TransactionPaginationTermination,
}
```


**TransactionPaginationTermination** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs#L105)

```rust
pub enum TransactionPaginationTermination {
    EmptyPageObserved,
}
```


**TransactionPaginationFailure** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs#L111)

```rust
pub enum TransactionPaginationFailure {
    PageLimit,
    RecordLimit,
    ByteLimit,
    OffsetLimit,
    ConnectionChanged,
    DateChanged,
    RepeatedPage,
    InvalidOrder,
}
```


**TransactionPaginationError** · [定义](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs#L129)

```rust
pub struct TransactionPaginationError {
    pub reason: TransactionPaginationFailure,
    pub start: u32,
    pub pages: usize,
    pub records: usize,
}
```


### src/tcp/stock/xdxr.rs


**Xdxr** · [定义](../../crates/rustdx-core/src/tcp/stock/xdxr.rs#L5)

```rust
pub struct Xdxr<'d> {
    pub send: Box<[u8]>,
    pub market: u16,
    pub code: &'d str,
    pub response: Vec<u8>,
    pub count: Option<usize>,
    pub data: Vec<XdxrData>,
}
```


**XdxrData** · [定义](../../crates/rustdx-core/src/tcp/stock/xdxr.rs#L97)

```rust
pub struct XdxrData {
    pub market: u8,
    pub code: String,
    pub date: u32,
    pub category: u8,
    pub fh_qltp: f32,
    pub pgj_qzgb: f32,
    pub sg_hltp: f32,
    pub pg_hzgb: f32,
}
```


本模块公开声明（方法归属见源码，new/recv可属于多个请求类型）：

```rust

// impl<'d> Xdxr<'d>

pub fn new(market: u16, code: &'d str) -> Self;


// impl XdxrData

pub fn parse(bytes: &[u8]) -> XdxrData;


```


## 9. 来源、核对口径与固定摘要

本手册的接口名、类型、默认值和校验边界来自实际源码；业务解释按协议配置及独立证据区分已确认、候选和未知。源码注释与当前实现有差异时以执行路径为准，例如旧历史分时注释曾写“异常返回空”，高层try_parse实际传播非法响应错误。没有取得完整官方7709字节规范或各命令官方QPS，本手册不提供虚构上限。

<a id="cite-1"></a>**[cite:1] 当前高层与公开导出。** [Client](../../crates/rustdx-core/src/tcp/stock/client.rs)、[扩展查询](../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs)、[stock导出](../../crates/rustdx-core/src/tcp/stock/mod.rs)、[目录](../../crates/rustdx-core/src/tcp/stock/security_directory.rs)、[周期类型](../../crates/rustdx-core/src/data/model.rs)。下表和第8节保留实际签名，当前快照见同目录 `.snapshot.json`。

<a id="cite-2"></a>**[cite:2] 四个新增注册项实现。** [Client接入](../../crates/rustdx-core/src/tcp/stock/client/version_330.rs)、[板块元数据](../../crates/rustdx-core/src/tcp/stock/resource_metadata.rs)、[F10](../../crates/rustdx-core/src/tcp/stock/company_info_request.rs)、[退市日线](../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs)。

<a id="cite-3"></a>**[cite:3] 财务、事件、量额与分页。** [财务语义](../../crates/rustdx-core/src/tcp/stock/finance_semantics.rs)、[成交事件](../../crates/rustdx-core/src/tcp/stock/trade_models.rs)、[严格枚举](../../crates/rustdx-core/src/tcp/stock/client/transactions.rs)、[分页选项](../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs)、[K线解码](../../crates/rustdx-core/src/tcp/stock/kline_request.rs)、[源成交额](../../crates/rustdx-core/src/tcp/stock/reported_turnover.rs)、[估算额](../../crates/rustdx-core/src/tcp/stock/trade_amount.rs)。

<a id="cite-4"></a>**[cite:4] 网络、启动与保活。** [Tcp/TcpConfig](../../crates/rustdx-core/src/tcp/mod.rs)、[网络预算](../../crates/rustdx-core/src/tcp/governance.rs)、[心跳](../../crates/rustdx-core/src/tcp/heartbeat.rs)、[启动](../../crates/rustdx-core/src/tcp/startup.rs)、[当日IP池](../../crates/rustdx-core/src/tcp/ip/daily.rs)、[OperationContext](../../crates/rustdx-core/src/operation.rs)。

<a id="cite-5"></a>**[cite:5] 兼容模型。** [股本变迁](../../crates/rustdx-core/src/tcp/stock/xdxr.rs)、[历史分时](../../crates/rustdx-core/src/tcp/stock/history_minute_time.rs)、[旧F10](../../crates/rustdx-core/src/tcp/stock/company_info.rs)、[板块下载/解析](../../crates/rustdx-core/src/tcp/stock/block_info.rs)、[checked兼容请求](../../crates/rustdx-core/src/tcp/stock/requests.rs)。

<a id="cite-6"></a>**[cite:6] 上游注册与解释资料。** [eltdx3.3.0注册表](../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/request.rs)、[PyPI3.3.0](https://pypi.org/project/eltdx/3.3.0/)、[3.2.2的22命令业务手册](../archive/operations/ELTDX_3_2_2_COMMAND_GUIDE_20261024.md)。后者用于旧22项背景与建议频率，不用其Python方法名替代本库实际Rust方法。

<a id="cite-7"></a>**[cite:7] 四项真实样本与固定实施证据。** [26项实施报告](../archive/validation/ELTDX_3_3_0_26_COMMANDS_IMPLEMENTATION_20261024.md)、[固定机器证据](../testing/evidence/eltdx-3.3.0-26-commands-implementation-20261024.json)、[live-a独立核验](../testing/evidence/eltdx-3.3.0-commands-20261024/live-a/independent-verification.json)、[live-b独立核验](../testing/evidence/eltdx-3.3.0-commands-20261024/live-b/independent-verification.json)。这是此前实施阶段样本，不是本次新增请求。

<a id="cite-8"></a>**[cite:8] 推送状态与验收边界。** [会话](../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs)、[游标请求/解码](../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs)、[推送验证](../archive/validation/QUOTE_PUSH_VALIDATION_20261024.md)。

<a id="cite-9"></a>**[cite:9] 近期分时/竞价真实合同。** [真实合同报告](../archive/validation/LIVE_COMMAND_CONTRACT_VALIDATION_20261024.md)、[0feb源码](../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)、[056a源码](../../crates/rustdx-core/src/tcp/stock/auctions.rs)。

需要区分三种证据：源码验证接口和工程约束；真实报文验证某主站某参数的传输/解码；交易所或发行人依据验证日期、单位、报表/成交含义。后两者不能由方法存在、数值非空或单元测试通过自动推导。

本手册26个命令章节及Client声明按源码维护；第6节四个实际API示例此前通过Rust1.89.0离线编译，未执行或连接服务器。本次迁移后另核对源码导航及示例编译。原[接口快照](COMMAND_API.snapshot.json)保留生成时的旧路径、源码与手册摘要；当前文件摘要和路径映射由[本次验证](../testing/evidence/project-restructure-20261024/verification.json)记录，不改写历史快照。
