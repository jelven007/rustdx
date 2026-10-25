# 数据语义与迁移

<!-- lifecycle:registry -->
读者：升级集成者；责任：API及发布维护者。源码核验：2026-10-25；每90日及破坏性行为或升级路径变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

当前源码版本：<!-- version:project -->2.0.1<!-- /version -->。数字由主线 CI 同步；注册表历史发布事实另列。

更新日期：2026-10-25。核心/CLI 2.0.0已发布到crates.io，源码身份和实际包摘要见[发布记录](../archive/release/PUBLICATION_20261025.md)。下方较早章节保留当轮版本事实，其变更已纳入2.0.0；runner仍仅交付源码与二进制。外部应用共同消费约定见[应用接入](../development/APPLICATION_INTEGRATION.md)。

## 2.0.0主版本迁移与等待契约（已发布）

采用破坏性主版本升级，CLI依赖核心2.0.0。包名、导入名、二进制名保持原值，
1.x调用方不得假定新返回类型兼容；正式发布身份另含提交、源码与Cargo.lock摘要、
实际`.crate`摘要和独立解包检查结果。

| 调用 | 2.0.0行为与迁移 |
| --- | --- |
| `TradingCalendar::is_trading_day/is_trading_time` | 返回`Result<bool>`；未知日期为Err，已知休市为Ok(false) |
| 日历前后导航、最近N日、计数和当前/市场交易日 | 返回Result；使用`?`或显式处理覆盖错误，不再回退或工作日估计；try入口同语义保留 |
| `Fq::new/concat` | 同证券事件与日线才接受，身份错误为None；新`new_checked/concat_checked`返回明确错误，day字节封装使用checked |
| `Cache::get_or_fetch` | 兼容入口仍无等待期限，同条带重入直接fetch；有预算迁移到`get_or_fetch_checked(key, &context, fetch)` |
| checked缓存返回 | `CacheFetchError::Operation`保存超时/取消/重入/锁错误，`Fetch`保存原错误；非协作闭包超期完成后拒绝交付且不写缓存 |
| 池满额等待 | 每20毫秒以内重查父任务期限/取消，建连与探活预留容量通过RAII释放 |
| CLI/runner启动 | 本地文件、状态、指标、恢复默认离线；`--tcp-warmup`显式通用预热，与`--no-tcp-startup`互斥；TCP来源自行核验节点 |
| Vec批量 | 默认总100万行/256MiB、单项65536行/32MiB保留结果预算；`set_batch_result_budget`显式设置，超限/取消/期限为外层Err，普通单项错误保持顺序和种类 |

财务`FinanceSnapshot`、心跳`HeartbeatAck`和报价精度签名的破坏性迁移见下方。
`scripts/release/check_consumers.py`验证新的独立调用方，并要求历史返回类型调用方
以声明的类型错误失败；这不是声称所有1.x API都兼容。`preflight.py`从两个实际
解包构建核心下游与CLI，正式注册表验证需等待实际发布。

## 有预算的日线采集计划（2026-10-25，未发布）

`tcp_day::Request`增加可选`Schedule`，runner `Config`增加可选
`tcp_day_schedule`；Rust字面量分别补`schedule: None`及`tcp_day_schedule: None`。
旧冻结JSON省略字段时维持原串行32证券合同，序列化不额外写入None字段。
新runner冻结有总行、正文、时间及并行预算的计划，至多10000只沪深A股、8线程；
旧052d仅离线重放，原收据、原码与版本不重新标记。

默认计划取`workers.min(8)`、整任务扣除发布预留的期限、单证券最大原始行×证券数
及`max_source_bytes`正文总量。每窗口先按单证券最大值预留，包含最后一页/确认
131070字节余量；余额不足会拒绝下一证券。CSV字节先核预算，任一失败不生成
complete报告。详细配置、实际计数和消费边界见[来源合同](../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。

## 板块完整文件校验（2026-10-24，未发布）

`Client::block/get_block_info`现在自动验算02c5完整长度/MD5并严格解析成分；原来可能得到空表或部分记录的损坏文件会返回错误。size=0也要求正确空文件MD5。下载中途换节点、连接或北京时间观察日必须从元数据重新开始；`Client::block`保留底层IO错误kind。

新增`Client::block_file/download_block_file`返回`BlockFile`，保存content、元数据、实际MD5及块范围/未知头部/来源。`BlockFile::decode_records/decode_block_content`提供Result解析；旧`parse_block_content`保留历史容错行为。`BlockMetadata::verify_content`可独立验算字节，原MD5字段及未知尾部不改写。content及raw_payload不进入serde JSON，需要完整重放时显式归档原字节。范围及回归见[修复报告](../archive/development/BLOCK_FILE_INTEGRITY_FIX_20261024.md)。

## TCP日线候选与runner来源（2026-10-24，未发布）

runner `Config`新增可选`tcp_day`，旧JSON省略它仍为本地day；
Rust结构体字面量须补`tcp_day: None`。TCP来源与非空day_directories互斥，
日期、单位依据及规模必须符合[来源合同](../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。
旧Job哈希保持本地day身份；TCP哈希另含来源合同/端点/依据SHA。

CLI新增`tcp-day`及库`tcp_day`，`Client::with_config_and_observer`支持从握手留证，
`tcp_evidence::visit_records`按顺序核验完整库存。TCP schema8 CSV的vol为股，
旧day schema6/7仍为手；不可按相同表头统一乘100。新候选报告不代替runner业务覆盖，
清单定义见[导入状态](../operations/IMPORTS.md)。已封存恢复不重新采集。
本节当轮未升级；当前版本已在2.0.0迁移节同步，注册表核验仍未执行。

## 探测协调、错误分类和增量指标（2026-10-24，未发布）

新增`ip::ProbePolicy/configure_probes/probe_stats`；独立池和诊断现在共用探测配额，
共享业务配置自动衍生单独`probes/`锁目录。`ProbeStage`增加`Quota`，
完整match须处理该项；配额失败终止刷新而不将其当作节点故障发布。
新增命令观察与`capable_nodes`，能力消费边界见应用接入。

runner增加`Error::TimedOut`与`category()`；旧deadline不再算作`is_cancelled()`，
完整match与相关测试须同步。取消/超时账本detail保留分类，现有状态集合不扩展。
worker目录软计量改为1秒采样/退出对账，进程取消仍100ms，组长回收后最多
等待200ms核对后代消失；目标硬配额仍须独立落实。

新增checked批量及`EmaState/MacdState`，旧EMA/MACD返回形式和默认计算口径
保持一致。缺口/跨块/重复bar/一倍两倍柱状图合同统一在应用接入维护。

## K线来源、区间状态与流式批量（2026-10-24，未发布）

`CanonicalBar`新增必填的 `provenance`，手写结构体字面量须提供它；JSON新增来源对象，旧 `Bar/KlineData/IndexKlineData`字段不变。checked标准化保存原始category与分页参数、解码类型、转换前量/单位/依据；网络recv另附来源。旧 `from_kline`因没有请求参数而保留request=None，迁移到带请求适配器时不得从合并后的Period猜原始category。

新增 `bars_range_with_options/index_bars_range_with_options`，默认短页按raw数继续到空页或越过begin，返回 `BarRangeCollection`。先检查complete/termination/coverage；完成仅限来源枚举事实。旧Vec入口保持短页停止；K线所有区间入口改为同连接空页复查，自动重连后拒绝续页，重复时间戳内容冲突报错。过去会被静默去重或跨连接拼接的结果现在返回Err。

新增K线及扩展命令 `*_batch_stream`，按输入序号回调、逐项保留错误，支持单项/总行数与JSON字节、总deadline和取消。K线使用 `BarBatchStreamOptions`，其余使用 `BatchStreamOptions`；JSON字节不等于RSS或协议留证大小。Vec兼容批量保留，但K线批次与扩展命令共享线程/锁错误传播和10万输入上限。

2.0聚合Vec额外以`BatchResultBudget`核算结果槽位、嵌套容量、原始正文与资格绑定；单项/总量耗尽不再收集完整结果。错误诊断截至2048字节，种类与输入序保留。该保留预算不包含在途解码、目录与进程开销，不是RSS上限。精度表改为按市场的借用字符串查找，公开接口、缺失/过期及世代语义保持一致。

内存缓存使用增量计数、到期索引及Arc读取；逻辑容量仍包含键和值。文件缓存默认增加4096项/256MiB目录预算，所有同目录实例须采用相同配置并通过OS锁维护；过期读取会删除文件，写入可能驱逐最早到期项。缓存格式仍为RDXC0002；外部不符合缓存命名格式的文件保持不动。详见[调用合同](../development/APPLICATION_INTEGRATION.md)。

## 受管响应身份校验（2026-10-22会话修复，未发布）

旧、新命令的受管查询、握手、心跳和IP探测现在共同核对响应前缀、opcode及消息号。过去可能接受的错配响应现在返回`InvalidData`，连接不再可复用，该次自动重试停止；正常传输错误仍按既有预算重试。Header-only `Tcp::send_recv`也执行校验。

`tcp::decode_response`新增请求匹配的离线解码入口，CLI归档`replay`使用它。`decode_frame`保留原始长度/解压用途，完整错配帧仍可归档检查。模拟服务器须返回正确前缀和回显身份；冻结报文及历史摘要不改写。

## 生产工程目录（2026-10-24）

根Cargo.toml改为虚拟workspace，核心源码path依赖改为crates/rustdx-core，CLI源码安装改为apps/rustdx-cli。runner在apps/rustdx-runner，外部测试在tests/integration，fuzz在tools/fuzz。公开包名、导入名和二进制名保持不变。

当前文档迁至稳定生命周期入口，命令手册为docs/reference/COMMAND_API.md；固定证据迁至docs/testing/evidence。原始记录中的旧路径/摘要按映射追溯，不重新冻结历史数据。完整映射、删除原因和验证见[目录迁移](../archive/release/PROJECT_RESTRUCTURE_20261024.md)。

## F10与退市日线现代入口（2026-10-24，未发布）

26个同码命令均已有入口。02c5元数据保留文件摘要，02cf/02d0提供目录、指定字节范围、完整读取与批量；052b提供退市日线单页、分页集合及批量。F10/退市请求接受市场0/1/2与trim后六位数字代码，市场2在线范围仍未验收。

F10按实际字节推进，达到目录声明范围才完成；052b短页继续到空页，返回集合先检查complete/termination/atomic。052b量为股，价格/金额使用专属解码口径，不能套用普通报价或052d金额算法。签名与字段见[命令参考](../reference/COMMAND_API.md)。

## 成交估算与源成交额（2026-10-24，未发布）

新增 `TradePage/EnhancedHistoryTransactionPage::amount_estimates`、
`TradeCollection/TransactionCollection::amount_estimate_summary`、
`Client::reported_turnover` 和 `reconcile_trade_amounts`。旧成交 DTO 不增加必填字段。
数量单位须通过 `TradeAmountContext` 显式确认；价格×数量仅为估算，
整数分的 JSON 字段为十进制字符串。候选、快照、盘后和未知事件分别表达。

源金额需匹配证券、日期、端点的 `TurnoverProfile`，保存原始编码精度，
不可用不回退为估算或零。`Combined` 在缺少盘后源额时没有组合源总额。
股票/指数 K 线兼容 `amount` 改用受检 binary32 金额解码，纠正零码与低值；
旧数量 helper 的其他用途保持原合同。已有外部乘积字段迁移为
`estimated_amount_*`，不能批量改成实际成交额。
接口、示例、冻结样本与验证见[实施记录](../archive/development/TRADE_AMOUNT_SEMANTICS_IMPLEMENTATION_20261024.md)。

## 按命令启动TCP与当日IP池（2026-10-24，未发布）

`rustdx`和`rustdx-runner`本地文件与管理命令默认不初始化行情TCP；
`--tcp-warmup`显式启用初始池全量测速、当日池发布及握手/0004心跳验证。
显式预热失败非零退出；TCP来源默认只核验自己的冻结端点。
旧`--no-tcp-startup`保留并与预热开关互斥；runner的
`"no_tcp_startup": true`继续禁止TCP来源并传递给采集子进程。
默认false不写入序列化配置，保持既有配置摘要。`--help`维持不访问网络。

默认保留40个种子，二进制可用`RUSTDX_TCP_INITIAL_IPS`覆盖；
集成项目在`main`调用`tcp::startup::initialize_with_config(&config)`，
并在它之前完成`configure_network`。新增`IpPool::refresh_until`和启动
绝对时限API继承整任务deadline/取消。`TcpConfig::ip_pool`可传独立
可CRUD池；完整结构体字面量须补该字段，推荐`..Default::default()`。
所有库内TCP选址/重连/池复用仅放行当日三轮全通过节点。启动预算和预热
连接交接规则见[启动与当日IP池](../architecture/DAILY_IP_POOL.md)。

## 四条替代命令迁移（2026-10-24，未发布）

默认调用改为`0004/044d/0537/0fc6`。`heartbeat()`返回类型变更属于2.0.0主版本迁移。

| 旧默认调用 | 当前调用与约束 | 显式旧路径 |
| --- | --- | --- |
| `Tcp/Client::heartbeat() -> u16`（044e证券数） | 返回`HeartbeatAck`，含`server_date_raw/server_date/raw_payload`；管理连接默认空闲30秒自动保活 | `Tcp::heartbeat_legacy()`、`Client::stock_count(0)` |
| `Client::stocks()`（0450） | 使用044d，最多1600条/页，保留16字节名称、正确f32昨收；兼容返回结构不变并发布完整精度缓存 | `Client::stocks_legacy()`、`stock::stocks_legacy()`、旧`SecurityList` |
| `Client::minute()`（051d及今日0fb4回退） | 使用0537；完整均价、原字段、精度及合成会话时间用`minute_series()`；日期为None | `minute_legacy()`、旧`MinuteTime` |
| `history_transaction/history_transaction_all/history_trade_page/TradeQuery::history`（0fb5） | 使用0fc6，每页1–1800条；原`num`现保存真实笔数槽，仍不是唯一成交编号 | 同名`_legacy`、`TradeQuery::history_legacy()`、旧`HistoryTransactionRequest/HistoryTransaction` |

`security_directory_page(market, start: u32, count)`提供单页完整目录，`security_directory(market)`提供严格完整目录。完整目录枚举校验证券数、重复代码、连接ID及跨日；单页不发布全市场缓存。

0537的后续点相对**第一个点**恢复价格和均价：价格除以`10^decimal`，均价除以`100 × 10^decimal`。支持目录decimal 2/3/4，缺失或不支持精度报错；默认路径不会静默回退旧布局。首个冷分时查询及目录加载共用分页时限，热缓存分时查询沿用单请求时限。历史0fb4仍为显式日期入口。

全量历史保留原返回`TransactionCollection.records`兼容视图，同时新增`event_pages`保存每页完整原始正文、历史参考字节、笔数/状态/尾槽与来源。快照、盘后及未知状态参与原始偏移推进；短页继续，实际空页才结束，预算耗尽或中途重连报错。

空闲心跳和业务请求共用完整帧锁；后台失败只标记旧连接失效，后续调用负责重连，分页不会在后台换连接。`set_heartbeat_interval(None)`可关闭；`into_inner()`先停止并回收worker，正在进行的心跳最多等待已配置I/O时限。用`get_ref().try_clone()`执行裸I/O前须关闭自动心跳，避免绕过帧所有权。

字段、实帧与工程检查见[四命令实施记录](../archive/development/FOUR_COMMANDS_MODERNIZATION_20261024.md)。

## 财务语义与报价精度（2026-10-24，未发布）

默认`Client::finance/finance_snapshot`、`FinanceRequest::recv`和`FinanceInfo`返回`FinanceSnapshot`。`raw()`保存原始136字节及日期/地区/行业编码；`field(FinanceField::ReportedEps)`替代旧`zhigonggu`误称。财务总量金额候选乘1000，股本乘10000，每股指标/股东人数不缩放，未确定槽仅保留raw。原float32先扩宽为f64再换算。

`field.value()`仅返回逐字段已验证值，`candidate_value()`显式使用基线候选，`semantic_status/evidence`可检查依据。初始证据严格限定`sz000001`、更新日20260815及各字段原始位模式；其它证券或报告仍可原值回放、显式候选，或调用`normalize_finance(raw, Some(&caller_profile))`应用自备独立证据。`report_period`不从更新日推断，EPS不自动年化/TTM。负利润、现金流、EPS、净资产合法，`validate_finance_snapshot`只给语义风险提示。

| 历史调用 | 当前源码 |
| --- | --- |
| `client.finance(...).zongzichan` | `client.finance(...).total_assets_yuan()`返回`Option<f64>` |
| `FinanceInfoData`原类型 / 数值 | `finance_legacy`、`FinanceRequest::recv_legacy`、`decode_finance_legacy` |
| `decode_quotes(request, bytes)` | 增加显式`&PrecisionSnapshot`；历史固定/100使用`decode_quotes_legacy` |
| `QuotesRequest::recv(tcp)` | `recv(tcp, precision)`，或者默认高层`Client::quotes` |
| `SecurityQuotes::try_parse(bytes)` | 先`with_precision(Arc<PrecisionSnapshot>)`；历史回放用`try_parse_legacy` |
| 财务`industry/province/updated_date`公开字段 | `snapshot.raw().industry()/province()/updated_date()` |

Client的`quotes`仍返回`Vec<QuoteData>`，现价、昨收、OHLC及十档按目录同一精度还原；amount、量、涨速和时间不套价格除数。初始支持2/3/4，decimal0/1及其它值返回Unsupported，不按代码前缀猜测或静默/100。`quotes_with_metadata`提供请求/返回/缺失的市场代码、decimal/source与generation。

默认Client缓存相互独立；显式`Client::with_quote_metadata(config, Arc<QuoteMetadataStore>)`可共享。完整目录构造成功后一次发布；失败保留旧版本。北京时间自然日跨日或未来采集时间的快照不能用于live；没有默认stale开关。离线解码允许显式历史快照，使用者须给出当时精度。完整热目录找不到证券时返回MissingPrecision，同generation不会反复联网拉目录；新证券由主动refresh处理。

建议启动时调用`refresh_quote_metadata(&[所需市场])`（默认分页总体120秒）；首次`quotes`冷加载、报价及可选空帧重查共享默认请求15秒总时限，重试不扩展。单页目录不发布成全市场缓存，`Client::stocks`完整目录可预热缓存。

原`finance.expected.csv`保留作为历史pytdx期望；新`finance.semantic.expected.json`使用官方财报及字段容差，原始响应不变。既有导出只在原始数据及schema/精度来源充分时重解，不能对全部数据库字段统一除10。CLI/runner日线主链没有调用这些接口，本次不迁移日线库。

财务语义实施当轮保持Cargo 1.12.2/1.8.2；当前2.0.0主版本接纳上述返回类型及低层签名变化，仍仅源码交付。实际检查、实帧范围及未验收项见[实施记录](../archive/development/FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)。

## 通达信采集契约（2026-10-24，未发布）

行情来源枚举仅保留TdxTcp/TdxDayFile/TdxMinuteFile；CLI采集入口为`day`，行情导出为CSV，质量清单为JSON。清单schema7移除专用HTTP页字段，runner兼容schema6/7。通达信文件、TCP接口、复权、数据库和runner现有行为保留。新版检查及兼容边界见[验证记录](../archive/validation/TDX_ONLY_VALIDATION_20261024.md)。

## 新增生产控制层（2026-10-23，未发布）

`rustdx-runner 0.1.0`为独立不发布crate，使用本地A股day的`a-share/day/raw/cny/shares/v1`契约。既有CSV/平面表保留原列与手单位；runner规范分区和新CH/Mongo命名空间使用人民币元/股、完整dataset/release/契约/证券键，不直接修改旧表。DBA预建schema后影子发布并精确核验，消费者明确迁至固定catalog_version接口再切换；旧平面查询不能获得目录版本语义。

runner账本schema2从阶段一schema1自动增加目标/请求证据表；新目录保留固定数据库Receipt，恢复核验不改原凭据。备份与恢复包含完整文件和WAL状态，数据库服务端迁移/灾难恢复另验收。TCP新增可选FrameObserver/FrameArchive，不改变未启用留证的调用方式。命令/部署与16项回归见[runner](../../apps/rustdx-runner/README.md)、[实施记录](../archive/development/PRODUCTION_IMPLEMENTATION.md)。

## 仓库源码优化（2026-10-23，尚未发布新crate版本）

10月22日源码提交7cd02e7、5fb5b70、401265c、1f9f739已进入main；10月23日六步本地优化见[NEXT_OPTIMIZATION.md](../archive/history/NEXT_OPTIMIZATION.md)。该轮核心库/CLI的Cargo版本为1.12.2/1.8.2，crates.io上的这两个版本是10月11日历史发布内容；上述优化现已纳入已发布2.0.0，安装和发布身份见[发布记录](../archive/release/PUBLICATION_20261025.md)。完整文档见[索引](../README.md)，此前分支交付与两个历史快照见[RELEASES.md](RELEASES.md)。

`day`默认写运行清单和run_id归档，`--report`可更改路径；该轮schema_version为6，保留database_verification及verified状态，增加覆盖告警、回读批量/耗时、摘要行序及[每任务阶段观测](../operations/OBSERVABILITY.md)。默认4096键一批，`RUSTDX_VERIFY_BATCH_ROWS=1..16384`可调，仍精确核对全部导出数值；verified才允许清理CSV。规范化字段摘要改为CSV行序，schema3/4旧摘要可能不同，CSV字节摘要不变。同机同目标写入锁覆盖前因子读取至验收结束。Mongo需mongosh/mongoimport，长驻mongosh独立回读替代逐批mongoexport。数据库单次操作最多300秒并受整任务deadline约束，失败保留CSV；详见[IMPORTS.md](../operations/IMPORTS.md)。不可信文件使用`try_from_bytes/try_from_chunk/try_parse/try_to_vec`，旧单条入口保留。

布林带使用双栈聚合的中心化总体方差，KDJ使用单调队列；正常序列总耗时O(n)，预热及缺口恢复口径不变。方差舍入误差可能带来末位变化，避免通过逐位浮点相等评判布林带回归。

历史涨跌幅现在由`RuleContext`的日期、板块、当日ST状态及上市阶段决定。`rule_at/prices_at/status_at`能区分无幅度限制和非法输入；详细范围、IPO及例外见[LIMIT_RULES.md](../requirements/LIMIT_RULES.md)。静态接口仍保留，主板ST幅度按2026-07-06生效规则改为10%；历史5%须使用日期接口。

`CalendarProvider` 返回版本、来源和覆盖范围。严格前后交易日、最近N日和市场交易日接口在搜索越界时返回错误。`day --calendar calendar.json` 接受完整休市日列表（周末自动休市），报告记录使用的日历。输入格式为：

```json
{
  "version": "sse-2025-new-year",
  "source": "https://www.sse.com.cn/",
  "start": "2025-01-01",
  "end": "2025-01-06",
  "holidays": ["2025-01-01"]
}
```

示例只覆盖明确的局部区间。版本及来源不能为空；休市日期不能重复或越界。填入2027等新年份前，应从交易所公告取得**该覆盖区间所有休市日期**，不能通过扩大start/end或仅判断星期一至五来声称完整覆盖。库校验格式，数据真实性与完整性由提供者负责。

`CanonicalBar` 附带 `Period`、`BarSource` 和 `PriceBasis`；lc源记录仍保留旧类型，标准适配器要求提供带市场的Symbol。`PriceMultiplier` 与 `ReturnIndex` 是两个独立类型。`Bar::from_kline` 只兼容A股日线手单位，分钟周期与非A股用显式单位入口。北交所不会隐式映射到深圳TCP市场。`KlineBuilder::symbol` 是带市场的构建入口，旧code自动推断行为保留。

KlineRequest::new返回Result并提供私有参数的不可变请求，支持encode、recv_stock/recv_index及独立decode_stock_bars/decode_index_bars。KlineBuilder::build_request返回新请求，旧build/Kline/IndexKline及其公开字段保留；Client全部K线路径已迁移。checked解码现在拒绝尾随字节、非法日期/时分及超数量，数量要求1–800；原先可能默默接受的错误响应或零数量参数现在报错。旧parse签名仍无Result，失败会清空data，应使用try_parse或独立decode。

快照、财务及当日/历史逐笔新增`QuotesRequest/FinanceRequest/TransactionRequest/HistoryTransactionRequest`，参数私有、checked构造，独立decode与同步recv分离；Client对应方法全部使用新请求。快照拒绝请求/返回证券重复和错配；财务拒绝非有限字段及非法非零日期；逐笔1–2000条，检查数量、时间、正价格与非负量/编号，未公开方向值原样保留。上述旧对象的try_parse复用新解码，尾随和超量不再静默接受。当时财务先f32→f64再乘10000的行为现归`_legacy`，默认新语义见本文首节。`KlineRequest::for_symbol`显式周期，`decode_canonical_stock/index`及对应recv从请求推导周期，源量单位必须指定。详见[协议迁移](../architecture/PROTOCOL_REQUESTS.md)。

交易所股票名单请求使用HTTPS、总时限和16MiB响应限制；上交所不再发送历史硬编码会话Cookie。接口鉴权或反爬变化会显式返回错误。`docsrs` feature可在稳定Rust下开启；自动文档标记由docs.rs的 `cfg(docsrs)` 单独启用。

受管TCP/HTTP共享默认8条进程预算；CLI默认启用同机OS锁许可，首次使用或读取policy/stats前通过configure_network配置。CLI全命令新增--timeout（默认10800秒）、--cancel-file及Ctrl-C/SIGTERM协作取消；重试不能延长绝对deadline。批次完成释放连接，服务器轮换和冷却保留。不同许可目录、跨主机及裸socket不在该上限内，Mongo工具内部socket也不逐个计数；详见[NETWORK.md](../architecture/NETWORK.md)。

同一证券多个day来源的日期区间重叠现在拒绝发布，即使区间内实际缺口互补；需先整理源文件。day记录以4096条分块，质量与收益状态跨块承接；输出固定目录参数、文件名及日期顺序，改变线程数不改变CSV字节。非A股仍对同日保留源最后一条，质量详情有上限并记录省略数量。CSV发布前回读检查行数、来源区间和日期边界。旧清单消费者须兼容新增字段和中间状态；project_version仍是Cargo版本，应另存Git提交区分源码。run_version不替代ClickHouse服务端行version。

## jelven-rustdx-complete 1.12.2 / jelven-rustdx-cli 1.8.2（2026-10-11，已发布 crates.io）

以下为相对1.12.1 / 1.8.1的行为变化；核心库导入名仍为 `rustdx_complete`，CLI二进制名仍为 `rustdx`。

`k_adjusted` 的后复权锚点是服务器可用历史的第一根日K，不保证是上市首日；首日及更早事件已体现在首日价格中，不再重复应用。停牌期间多次事件以前次参考价继续计算。前复权仍锚定可用历史末日，区间过滤不改变锚点。非法事件、价格及乘数返回InvalidData；旧 `ex_right_reference` 单独调用仍保留非法输入返回原昨收的兼容行为。

涨跌停报价以分为单位，用整数百分比四舍五入；例如主板昨收1.15元，涨停价为1.27元。非法、非分单位或超出精确计算范围的昨收返回None。主板代码仅匹配600/601/603/605和000/001/002/003前缀；按代码推断仍不能区分同码的指数与股票。历史制度和新股特殊阶段仍需调用方提供规则。

证券列表遇到未达到总数的空页、重复证券或超出总数时返回错误，不交付部分列表。`market_of` 要求六位数字。重连握手失败消耗一次尝试，剩余尝试仍会执行；同时使用显式 `retry` 和自动重试会形成嵌套预算，不应将其当作任务总deadline。

旧 `get_industry_name/get_province_name/get_industry_info` 已弃用，分别返回“未知行业”“未知省份”及原编码/未知行业/未分类；原静态表无可靠来源，不能继续用于选股或归因。使用财务接口的原始 `industry/province` 编码并配合同源、带版本字典；通达信概念板块文件不替代财务分类字典。

`day` 在执行采集前检查所有输出与输入路径。相对/绝对路径、`..`、符号链接别名也会判定冲突；报告不能覆盖day、gbbq、前因子或名单文件，也不能与输出、保留CSV、factor.csv重名。输出锁覆盖整个命令，冲突任务立即失败；下载来源持有缓存锁直到解析结束。系统临时目录中的锁文件保留以避免锁竞争，文件存在不代表仍被占用。

从ClickHouse读取的前因子现在与CSV一样验证日期、代码、正有限因子/收盘价、重复代码及空集；不再接受未计算的factor=0。`--keep-factor` 的factor.csv只在数据校验通过后原子替换。HEAD总时限30秒；每次ZIP下载建连15秒、响应头30秒、总时限2小时，超时可通过原有续传恢复。

EMA周期0返回NaN，非有限输入返回NaN并重置，下一有效价格重新初始化。MACD非法周期返回全None，缺口处三条线均为None并保留原序列位置。布林带周期小于2或倍数非法时全None。SMA使用补偿求和，窗口总和溢出时返回None并在后续窗口恢复。KDJ要求窗口内每根收盘位于自身高低区间；保留 `alpha=2/(d_period+1)`、首个有效RSV初始化K/D的口径，不能直接等同于alpha=1/3、初值50的其他实现。

## jelven-rustdx-complete 1.12.1 / jelven-rustdx-cli 1.8.1

均已发布到 crates.io。核心库安装 `cargo add jelven-rustdx-complete --rename rustdx-complete`；CLI 安装 `cargo install jelven-rustdx-cli --locked`（二进制 `rustdx`）。CLI 更名因 crates.io 上 `rustdx-cli` 归属他人，Rust 导入名仍为 `rustdx_cli`。

该已发布版本加入同一Client批量调用有界排队、批次内并行。进程总配额、端点冷却及连接/帧/分页/批次总体时限是2026-10-22源码新增行为，见本文首节；不将其视为已发布1.12.1/1.8.1的能力。

旧静态概念查询已弃用：`get_concept_stocks/get_concept_info` 返回None，`get_concept_names` 返回空列表，表示本库不再提供未核实样例。迁移到 `Client::block("block_gn.dat")`，按源板块名筛选并保存采集时间。该接口返回代码而非证券名称，名称可从证券列表匹配；不同来源板块不能直接等同。

`Symbol` 继续支持六位字母数字代码，但字母代码不再判为A股。`limit_status` 对非有限、非正或high低于close的行情返回None。`k_adjusted` 的非法日期区间提前返回InvalidInput。

标准接口 `data::Symbol` 显式携带 sh/sz/bj 市场，保留前导零与字母代码。`data::Bar` 的价格和成交额单位为元、成交量为股、时间为 UTC+8 本地时间。TCP成交量按品种/周期和请求参数核实，显式指定源单位；旧适配器只兼容A股日线手单位。现有 day CSV 的 vol 仍为手（100股），保持文件兼容。

`factor` 是从基期开始的累计收益指数，不是前后复权价格乘数。原始 OHLC 未被改写。增量必须提供上一条有效 factor/收盘价，并只应用其日期之后的除权事件；停牌期间的多个事件按日期依次调整理论前收。首次导入以首条记录为基期。

内置交易日历仅有2024–2026休市数据。旧 `is_trading_day` 在范围外保留工作日估算；新 `try_is_trading_day` 和 `trading_days_between` 返回明确错误。质量报告对范围外记录标注未核实。时间判断排除11:30之后至13:00之前的午休，包含11:30和15:00端点。

## CLI 质量记录

```sh
rustdx day ./vipdoc/sh/lday ./vipdoc/sz/lday -o stocks.csv --report collection.json
rustdx day ./vipdoc/sh/lday -o stocks.csv --report collection.json --strict
```

报告记录版本、时间、来源文件、代码、行数、日期边界和质量问题。非法数值、重复/逆序日期、文件失败都会返回非零状态，输出通过临时文件原子替换，上次成功结果不会因本次失败被覆盖。缺少交易日不能直接认定为停牌，默认记录警告；`--strict` 将缺口和日历覆盖不足也视为失败。

全空day集合现在失败并保留旧输出；混合数据中的空文件默认只告警。报告在输出提交及数据库导入结束后确定complete/failed，导入失败也记录顶层errors。解析前因子、gbbq、目录或输出失败同样记录报告；输入参数或下载源解析失败发生在采集阶段之前。数据库可能已写入部分行，中间CSV保留供核验，不能把failed视为数据库事务回滚。

gbbq输入是通达信二进制文件。`Fq::new/concat` 现在仅返回交易日行，停牌事件不额外生成伪交易日；`xdxr` 标记在后续实际交易日是否应用过事件。SMA支持周期1，RSI横盘为50，KDJ缺数据不再填0。文件缓存使用新格式和稳定文件键，旧缓存不会复用。

## ClickHouse

新表使用 `ReplacingMergeTree(version)`，逻辑键是 `(date, code)`，`version UInt64 DEFAULT toUnixTimestamp64Nano(now64(9))` 为每次导入生成版本。重复导入会在合并时替换旧行，读取立即一致的结果须使用 `FINAL`。历史 CSV 仍可通过 `input_format_defaults_for_omitted_fields=1` 补足 version；未提供 preclose/factor 时默认为0（表示未计算）。

CLI1.8.1同时检查已有表：date Date、code String、OHLC/amount/vol/preclose/factor Float64、version UInt64；preclose/factor须有DEFAULT，version使用上述纳秒表达式；排序键必须严格为date,code。允许不分区或按date/toYYYYMM(date)/toYear(date)分区，其他分区表达式需另行核实后迁移，避免重复键跨分区。引擎或键即使包含version也不能绕过检查。

HTTP导入及SQL启用wait_end_of_query=1并读完整响应，HTTP错误、正文异常和截断都返回失败；10秒限制用于建连。本轮源码为单次HTTP操作另加300秒总体上限和INSERT query_id。辅助边界查询失败仍仅告警。

旧 `ReplacingMergeTree()` 表不会被自动改写。先备份，创建新表（例如 `rustdx.bars_v2`，CLI 指定 `-t rustdx.bars_v2` 会建表），再按列迁移：

```sql
INSERT INTO rustdx.bars_v2
    (date, code, open, high, low, close, amount, vol, preclose, factor)
SELECT date, code, open, high, low, close, amount, vol, preclose, factor
FROM rustdx.old_bars FINAL;
```

无 factor 的旧表去掉相应两列。迁移后核对 `count()`、代码数量、`min(date)`、`max(date)` 以及抽样值，再切换调用方。版本语义为最后导入优先，回灌前须确认源数据质量。禁止通过直接重跑掩盖未知导入状态。
