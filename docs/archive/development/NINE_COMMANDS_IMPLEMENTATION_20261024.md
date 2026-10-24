# 九个7709命令实施记录

版本：2026-10-24 / 1.0。状态：**九项源码调用链全部实现，未发布新crate版本**。范围为 `0452 / 051b / 0547 / 054b / 054c / 056a / 0fd1 / 0feb / 0ffc`；沿用 Rust 2024、MSRV 1.89、同步 I/O、有界线程和当日有效 IP 池。

后续边界复查修复了0feb负数昨收/今开、0547首帧游标下界、合法大批次报价的分区搜索，以及不可表示的订阅超时参数。详见[0feb/0547边界检查报告](../validation/COMMAND_0FEB_0547_BOUNDARY_REVIEW_20261024.md)。本文第6节的437项测试及源码摘要保留为原实施轮次证据，后续复查结果单独记录。

后续[真实业务合同验证](../validation/LIVE_COMMAND_CONTRACT_VALIDATION_20261024.md)取得0feb非空响应，但与同请求日的0fb4及日K线存在日期业务冲突；056a历史日期选择生效，符号解码通过，正买负卖得到非同期盘口支持；0547盘中主动推送及竞价同步侧别仍待有效交易时段验证。新增564组真实帧和本轮44项相关回归独立留证，不能将非空或工程通过解释为全部业务验收完成。

结合此前[四命令迁移](FOUR_COMMANDS_MODERNIZATION_20261024.md)，eltdx 3.2.2 的22项基线现在为 **22同码、0替代、0未接入**。这是请求、解析和公开入口的覆盖统计，不代表所有命令的全部参数、市场、字段业务语义或生产运行均已验收。已有财务/股本/K线/文件等扩展合同仍按各自报告判断。

## 1. 交付与公开API

每项都有独立请求构包、解码、owned模型和高层入口；请求参数私有且在构造时校验。`SpecialLimitsRequest::new(u16)`使用类型边界，无额外失败条件。源码入口位于[stock模块](../../../crates/rustdx-core/src/tcp/stock/mod.rs)和[Client扩展查询](../../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs)。

下表所有查询均返回 `io::Result<T>`。批量返回 `io::Result<Vec<io::Result<T>>>`，每只或每分片保留失败，按输入顺序交付，不把错误伪装为空数据。

| opcode | Client方法 | 返回T | 请求/解码模块 |
| --- | --- | --- | --- |
| `0452` | `special_limits(start: u16)` | `SpecialLimitPage` | [special_limits.rs](../../../crates/rustdx-core/src/tcp/stock/special_limits.rs) |
| `051b` | `intraday_aux(market, code, kind)` | `IntradayAuxSeries` | [intraday_aux.rs](../../../crates/rustdx-core/src/tcp/stock/intraday_aux.rs) |
| `0fd1` | `sparkline(market, code, options)` | `SparklineSeries` | [sparkline.rs](../../../crates/rustdx-core/src/tcp/stock/sparkline.rs) |
| `0feb` | `recent_minutes(market, code, date)` | `MinuteSeries` | [recent_minutes.rs](../../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs) |
| `056a` | `auction_series(market, code, date: Option<u32>, options)` | `AuctionSeries` | [auctions.rs](../../../crates/rustdx-core/src/tcp/stock/auctions.rs) |
| `0ffc` | `money_flow_daily(market, code)` | `MoneyFlowBlock` | [money_flow.rs](../../../crates/rustdx-core/src/tcp/stock/money_flow.rs) |
| `054c` | `snapshots(stocks)` | `SnapshotPage` | [snapshots.rs](../../../crates/rustdx-core/src/tcp/stock/snapshots.rs) |
| `054b` | `category_quotes(category, sort, options)` | `CategoryQuotePage` | [category_quotes.rs](../../../crates/rustdx-core/src/tcp/stock/category_quotes.rs) |
| `0547` | `subscribe_depth(stocks, options)` | `QuoteSubscription` | [quote_refresh.rs](../../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs)、[quote_subscription.rs](../../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs) |

六个批量入口：`intraday_aux_batch(stocks, kind, max_parallel)`、`sparkline_batch(stocks, options, max_parallel)`、`recent_minutes_batch(stocks, date, max_parallel)`、`auction_series_batch(stocks, date, options, max_parallel)`、`money_flow_daily_batch(stocks, max_parallel)`、`snapshots_batch(stocks, max_parallel)`。最后一个按80只分片，结果单位为请求页，其他五项按证券返回结果。

三个扫描入口：`special_limits_all(CommandScanOptions)`、`auction_series_all(market, code, date, AuctionOptions, CommandScanOptions)`、`category_quotes_all(category, sort, CategoryQuoteOptions, CommandScanOptions)`，返回相应 `CommandCollection<T>`。扫描保持单连接、总时限；已有页之后发生同地址重连、换址或北京时间观察日改变，明确报错，不拼接不同会话。

低层请求均提供 `encode(id)` 和 `recv`；所有九项导出 `decode_*`，可使用已保存正文离线重解。`RefreshRequest::recv(tcp, Arc<PrecisionSnapshot>)`消耗独占连接，读取一次后关闭，避免把有潜在推送的连接交回普通池。

```rust,no_run
use rustdx_complete::tcp::stock::{
    Client, AuctionOptions, CommandScanOptions, IntradayAuxKind,
    QuoteCategory, QuoteSortField, CategoryQuoteOptions, SubscriptionOptions,
};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut client = Client::new()?;
    client.refresh_quote_metadata(&[0, 1])?;
    let stocks = [(0, "000001"), (0, "159915"), (1, "510300")];
    let snapshot = client.snapshots(&stocks)?;
    let ranking = client.category_quotes(
        QuoteCategory::AShares, QuoteSortField::Amount,
        CategoryQuoteOptions { count: 20, ..Default::default() },
    )?;
    let auction = client.auction_series_all(
        0, "000001", None, AuctionOptions::default(), CommandScanOptions::default(),
    )?;
    let auxiliary = client.intraday_aux_batch(
        &stocks, IntradayAuxKind::BuySellStrength, 2,
    )?;
    let mut session = client.subscribe_depth(&stocks, SubscriptionOptions::default())?;
    session.refresh()?;
    let push = session.poll_push(Duration::from_millis(200))?;
    let depth = session.snapshot();
    session.close();
    Ok(())
}
```

## 2. 字段、参数与约束

查询证券显式接受市场0/1和六位ASCII字母数字代码；通过格式校验不代表主站存在或支持该证券。分类列表和特殊表保留服务端市场原值；实测分类列表可能返回北交所市场2，不据此开放所有单证券TCP接口。所有数量上限均是客户端内存/单帧政策，除有独立说明外，不宣称测得服务端硬上限。

| 接口 | 默认值/上界 | 结果合同 |
| --- | --- | --- |
| 特殊涨跌停 | start为u16；无page_size参数 | u16数量、13字节记录；市场/数字代码、上下限f32及位模式。表内未返回不代表无涨跌停 |
| 分时副图 | `BuySellStrength=0`、`VolumeComparison=11`；其他`Raw(u8)` | 0及非11布局为有符号i64双序列；11为f32昨日/当日序列及位模式。只有索引，无报文时间，其他selector不赋予已确认指标语义 |
| 小走势图 | selector=1、window=20；window须为1..65535 | 身份、selector回显、parameter/capacity、base_price和f32价格点。无时间戳；window20/40均返回61点，不解释成点数 |
| 近期分时 | 显式有效YYYYMMDD；最多240点 | selector=`0xfed62304 - chrono日期ordinal`；昨收/今开须为非负有限f32，0有效；价格/均价相对首点，均价再除100；分钟量保留raw，09:31起时间为合成标签 |
| 竞价过程 | date=None编码0；mode=3、start=0、count=500；count≤4095，偏移加数量不溢出u32 | 16字节记录；分钟/秒、f32价格、匹配量、未匹配有符号i32与绝对值、方向符号、保留槽 |
| 日资金流向 | 单只请求，38字节正文；消息路由保留`0x007e2dXX` | 40字节证券头与88字节日记录；日期、21个u32、总额f32位模式、16个u16分档、两套四档净额和比例 |
| 一档快照 | 1..80只、不得重复；批量自动80分片 | 价格、昨收/OHLC、量/额/内外盘、买一/卖一、时间raw、尾部raw和missing_codes |
| 分类行情 | A股category=6；sort代码0/现价6/成交额10/涨幅14/封单额28；start为u16，count=80且1..700 | reverse默认非代码排序降序1/升序2，代码排序0；可显式覆盖reverse/filter；一档、涨速/短换手/开盘指标和56字节尾部 |
| 刷新/推送 | 1..100只；初始游标0；不可重复 | 正文解压后XOR `0x93`；每条保存u32更新游标、完整五档和原始字段；空刷新表示本次无返回更新 |

`BasicQuote`与`QuoteLevel`的标准价格是 `Option<f64>`，精度来自完整目录。054c和0547要求所有请求证券有有效精度；054b返回不在支持目录中的品种时，价格与scale为None，原始价格整数、盘口差值及数量继续保留。不可把None当0、不能对市场2擅自固定除100。

报价的五个价格字段保存 `[base, 昨收差值, 开盘差值, 最高差值, 最低差值]`；加法先checked i64，再按目录除数换算。`amount_raw`保持协议专用编码，`amount`沿用已验证的协议解码；成交量槽均标为raw，未验证品种不自动乘100。

资金流向按顺序从8个u32分出16个u16，相邻成对取差，得到8个差值d。普通四档净額使用`d[0/2/4/6] × total / 50000`，主买四档使用`d[1/3/5/7] × total / 50000`；主力净额使用超大/大两档，主买净额使用主买四档和。比例为对应差值和`/500`，单位是百分点。负净额合法；总额原始单位不自行改名为已核实交易所金额。

近期分时和竞价均未回显交易日期，`server_trading_date=None`，`requested_date`只表明请求。056a始终 `process_snapshots=true`；09:25快照不当作正式开盘成交。未匹配量仅给符号，买卖侧映射保留待验证状态；i32::MIN可安全取绝对值。

## 3. 统一结果与资源预算

`CommandPage<T>`包含records、opcode、主站、获取时间、连接ID、可选目录代次，以及页级共享`Arc<[u8]>`原始正文。JSON序列化跳过正文，完整原始帧由观察器单独保存，避免将大正文逐行复制。

`CommandScanOptions`默认256页、10000条、网络政策pagination_timeout；显式最多65536页/100万条。终止原因区分EmptyPage、ShortPage、PageBudget、RecordBudget、OffsetLimit。达到预算时交付已保留页且`complete=false`；请求/解析/身份/重复错误返回Err。0452用空页确认结束；056a/054b按请求count的短/空页结束。`complete=true`只描述本次服务器分页结束，`atomic=false`始终保留，不能当作冻结榜单或交易所完整性证据。

批量在创建请求前拒绝超过10万证券项；默认并行值0取可用线程数，再限制为连接预算减去Client原连接数、请求数和至少1个worker。与原K线批量共用批次锁和受管连接池，所有连接继承当日有效池配置、总deadline和OperationContext取消，不引入异步运行时。

[公共帧层](../../../crates/rustdx-core/src/tcp/frame.rs)按实际请求/响应检查长度、前缀、opcode和消息号；外层解压继续使用现有有界逻辑，单帧正文≤65535字节。错配使查询连接不可复用。Reader新增有限f32、u64与有符号i64变长整数，包含最小负数、溢出、截断和超长编码防护。

054c/0547的变长记录独立使用有界唯一结构分区：候选≤512、搜索≤2048步，校验证券身份、完整核心和重复证券；不按裸代码字节第一次出现直接切开记录。未知尾部保留raw，歧义布局拒绝。实测当前尾部长度054c为56、0547为46；不把所有服务端未知尾部解释为已核实字段。

## 4. 0547会话生命周期

订阅独占一条受管Tcp，关闭其自动空闲心跳，避免后台心跳抢读未配对帧。Client查询连接保留；额外连接仍需全局许可。

`refresh()`发送当前各证券游标，按opcode/消息号配对响应；先到的未配对0547帧按推送解码和入队。`poll_push(timeout)`先取已排队通知，再同步读socket。局部poll超时返回None并保留半头/半体；整体deadline、取消、断线、撤销、精度跨日或不合法帧使会话失效并关闭。推送用独立 `PushObserver`，不伪造对应请求；观察器保存失败也使会话失效，不发布该帧。

队列默认最多256帧/8MiB，两者均可缩小。原始体、解压/异或体、owned记录/盘口/尾部/证券以及暂存帧都计入预算；直接poll的单帧也检查预算。超限不静默丢帧，清队列、释放连接并将状态标为Invalid。

每帧全页校验后再发布：每条必须有完整五档、请求证券和游标。返回记录是该证券完整状态，整条替换，不累加已有总量/内外盘；缺席证券保持旧状态。所有证券返回游标均不得低于请求值，包括首次基线；会话合并另外检查当前状态游标，不允许前置推送后的响应回退。同游标完整重复不会重复累加；有效0和零量盘口保留。只有所有订阅证券均收到基线才标Synchronized，否则AwaitingBaseline。取出已排队的旧通知不再次合并，避免覆盖更晚响应。

`snapshot()`保留状态、连接ID、代次、各证券quote/cursor/接收时间和队列统计；Invalid的残留quote供诊断，调用者必须按status判断有效性。`rebuild(fresh_metadata)`先关闭旧连接、代次递增、清空基线和游标、消息号回到1，再通过原池建连并刷新；新目录失败不留下有效会话。消息号不会在同一会话内回绕复用。`close()`和Drop释放连接许可。

## 5. 主站和独立真实核验

使用115.238.56.198:7709、180.153.18.170:7709两个节点，均先通过当日池三轮标准探测，使用受管连接。两者支持所测056a/0ffc，不需要另设不兼容Standard的ProbeProfile；显式选择专用节点仍通过 `TcpConfig::ip_pool`，不绕过许可或有效池。

[完整证据目录](../../testing/evidence/nine-commands-20261024)保存 **132组完整请求/响应帧、104342条目录记录**、decoded DTO、当日池结果、会话状态及SHA256清单。11组精选真实夹具位于[protocol_7709](../../../crates/rustdx-core/tests/fixtures/protocol_7709/manifest.json)。第三方源码只用于固定报文事实，未作为运行时依赖或执行。

| 命令 | 两节点各自实测结果 | 核验范围 |
| --- | --- | --- |
| 0452 | start0一条000010，start1空 | 上下限位模式、数值代码和分页 |
| 051b | selector0/11各240点 | 双序列、i64整数和f32位模式 |
| 0fd1 | window20/40各61点 | 返回身份、参数/容量、全部价格位模式 |
| 0feb | 20261022/23/24 × 股票/ETF全部空 | 12次真实传输、selector和10字节空头；非空业务未验收 |
| 056a | 日期0与请求20261023各80点；40条翻页40/40/0 | 全字段、正负未匹配量、索引、分页拼接一致；实际历史日期未证明 |
| 0ffc | 000001/600519各5日 | 21原始槽、16分档、两套净額/比例独立算术 |
| 054c | 000001/159915/510300各一条 | 2/3位目录精度、一档、原始字段和尾部 |
| 054b | 成交额/涨幅双向各5条，成交额下一页5条 | 排序请求值、全部记录及56字节尾部；市场2精度空值保留 |
| 0547 | 初始3条、随后0/0；Synchronized | XOR、全五档、游标、空更新；周六poll250ms未观察到主动推送 |

三个可比现价分别为11.2、2.702、4.178，054c/0547与相邻053e一致；OHLC亦核对。采样为周六非交易时段，不把该结果推广为盘中连续更新、竞价侧别或全市场业务验收。

[verify_nine_commands.py](../../../scripts/validation/verify_nine_commands.py)只用Python标准库，以固定记录长度独立解析报价，与Rust的结构分区算法分开；逐字段比较原始整数、浮点位、所有价格、资金计算和竞价跨页。复跑先核对源文件SHA，不联网、不执行eltdx：

```sh
python3 scripts/validation/verify_nine_commands.py \
  docs/validation/evidence/nine-commands-20261024/115.238.56.198
python3 scripts/validation/verify_nine_commands.py \
  docs/validation/evidence/nine-commands-20261024/180.153.18.170
```

## 6. 工程验证与发布范围

专项测试包含9项请求、11组真实夹具全部owned记录、i64极值、价格相对首点/均价比例、身份/精度、截断/异常数值及随机畸形输入；本地协议测试覆盖批量顺序/错误、分页预算/重复/同址重连、前置推送、半帧超时、队列与单帧预算、观察器失败、基线、游标回退、目录过期、重建/撤销/取消和许可释放。

最终工程命令及结果记录在[机器证据](../../testing/evidence/nine-commands-implementation-20261024.json)，[日志和21份源码摘要](../../testing/evidence/nine-commands-20261024/checks/sha256-manifest.json)已固定。Rust1.89下fmt、workspace全targets/allfeatures Clippy `-D warnings`、严格rustdoc构建全部通过；workspace全features测试为**437通过、0失败、24忽略**，其中九命令专项**26通过**。外部数据库及旧live测试按现有离线开关/ignore规则排除，两个真实节点采样另行执行，不混计离线回归。文档构建时一并修正五处既有无效/私有文档链接。

本轮提供库级查询与采样/核验工具，未新增CLI命令或runner持续采集Job。Cargo版本保持1.12.2/1.8.2；没有提交、推送或发布。当前可复核范围内，九项代码实现均完成；0feb非空响应、0547交易时段主动推送、竞价历史日期和方向、更多市场/参数的业务覆盖继续明确保留未验收状态。

## 7. 调用频率建议

协议没有可验证的公开统一频率配额。以下为客户端运维建议，实际需根据主站错误/延迟和许可预算调整；不是服务端承诺：

| 业务 | 建议 |
| --- | --- |
| 0452特殊表 | 会话初次或每日加载，规则可能变化时显式重取，避免逐证券重复扫描 |
| 051b副图/0feb近期分时 | 优先复用同证券同日结果，活跃序列按秒级按需更新，历史日减少重复读取 |
| 0fd1小图 | 展示需要时秒级更新，不用高频小图替代完整分时 |
| 056a竞价 | 竞价阶段按需秒级读取，历史日缓存但保留实际日期未知标记 |
| 0ffc资金流向 | 日结果按交易日缓存；盘中需更新时按需降频，不按行情快照频率轮询 |
| 054c/054b | 秒级按需刷新；分类全表扫描限制总页数和并行量，动态页不是原子快照 |
| 0547 | 同步调用方驱动refresh/poll，使用有界poll超时和队列，避免无限紧循环；失效先rebuild |

自动批量/分页执行当前网络总时限；本轮扫描没有独立自动节流睡眠，调用方需要在连续扫描或批次之间限制频率。采样工具明确在查询间等待150ms并限制帧/字节/总时限。

## 8. 后续 0feb / 0547 边界验证

2026-10-24 补充非空固定字节/240 分钟/多精度/压缩传输，以及多证券被动推送、完整状态替换、零值、游标推进与整页拒绝用例，见[专项验证报告](../validation/RECENT_MINUTES_PUSH_BOUNDARIES_20261024.md)。当前工作区工程回归为480通过、0失败、24忽略；该数量包含其他未提交改动，不替换第6节已固定的历史检查记录。

新增0feb轻量采样覆盖最近五个交易日和三只股票/ETF，15次真实响应仍全部为空，独立核验明确返回非空未验收。0547严格盘中采样入口在周六返回market_closed；正常验收要求完整交易窗口、同连接完整基线、被动期零请求，以及初始5秒后至少两帧游标推进和已知行情变化。非空本地回放和连续推送本地测试已通过，两项真实业务缺口继续保留。

## 9. 后续真实非空夹具与推送窗口测试

更早日期的归档采样已获得两个主站、六证券、两请求日期的24组非空0feb完整帧，独立解码5760条记录并固定为回归夹具，见[最新验证报告](../validation/NONEMPTY_FIXTURES_PUSH_WINDOW_20261024.md)。真实非空正文/字段回归已覆盖，响应未回显日期且与0fb4对照不一致，日期真实性、完整保存窗口和量单位仍不升级为已验收。

0547采样循环的通知评估逻辑与新增测试共用，补齐5秒临界点、重复通知、旧排队状态与严格开盘/午休/收盘窗口；本地推送与判定路径通过，实际交易时段主动推送仍须另行留证。第8节的空响应结论仅对应当时最近五个交易日采样。
