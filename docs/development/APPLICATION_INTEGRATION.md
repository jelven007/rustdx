# 外部应用调用约定

<!-- lifecycle:registry -->
读者：应用集成者；责任：API维护者。源码核验：2026-10-28；每90日及跨接口消费合同变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

当前源码版本：<!-- version:project -->2.0.4<!-- /version -->。数字由主线 CI 同步；注册表历史发布事实另列。

更新日期：2026-10-28。适用于应用通过 `rustdx_complete::tcp::stock::Client`、checked 请求及公开结果模型使用本仓库核心库。逐命令签名、返回字段和具体参数上限见[26命令接口手册](../reference/COMMAND_API.md)，已解析/候选/未知字段及证据范围见[字段解析状态](../reference/RESPONSE_FIELD_SEMANTICS.md)；本页定义跨接口的调用和消费规则。

已发布的历史核心包为 `jelven-rustdx-complete 2.0.0`，Rust 导入名为 `rustdx_complete`，最低 Rust 1.89、edition 2024。注册表可使用精确依赖 `version = "=2.0.0"`，发布源码与包摘要见[发布记录](../archive/release/PUBLICATION_20261025.md)。集成应用固定源码提交、源码摘要、依赖锁文件和所用接口版本，更新时检查[迁移说明](../release/MIGRATION.md)。

源码path依赖使用 `/path/to/rustdx/crates/rustdx-core`；根目录现在是虚拟Cargo工作区。目录升级、安装和验证入口见[目录迁移](../archive/release/PROJECT_RESTRUCTURE_20261024.md)。以下六类规则继续作为所有外部调用方的共同契约。

## 1. 证券身份与市场范围

应用中的证券键、缓存键和输出记录必须保留 `(market, code)`，或采用明确包含市场的 `Symbol`，并保留代码前导零。不能只用六位数字代码合并沪深证券，不能根据代码开头自动把所有证券映射为深圳。

| 接口范围 | 当前市场/代码约束 |
| --- | --- |
| 普通行情、目录、分时、成交、竞价等 | 市场0深圳/1上海；多数checked请求接受六位ASCII字母数字 |
| F10目录/正文、052b退市日线 | 市场0/1/2；代码trim后为六位ASCII数字 |
| `Symbol`、目录分类提示 | 能表达或提示的市场/类别不等于每个TCP接口都支持 |

市场2的F10/退市参数支持不代表已完成市场2网络验收。`market_of` 和目录 `category/board` 是代码前缀推断，不能作为证券身份、交易状态或上市板块的权威依据。

`QuoteData` 自身没有market字段。需要明确身份、缺失证券或精度来源时，使用 `quotes_with_metadata` 的 `QuotePage`；不要假定快照返回顺序与输入顺序相同。

## 2. 价格、数量、金额和未知值

原始DTO、兼容视图和标准化模型采用不同口径。应用必须按请求命令、品种、周期及结果字段的单位解释数据，转换后才写入自己的统一业务模型。

| 数据 | 调用与消费规则 |
| --- | --- |
| 报价及完整分时价格 | 使用证券目录精度；当前支持decimal 2/3/4，先用 `refresh_quote_metadata` 预热，跨北京时间自然日刷新 |
| 052d/052b K线价格 | 依据相应解码器的千分元口径；不能套用报价整数标尺 |
| 0fc5/0fc6成交价格 | 当前为兼容/100尺度；不推定其他未核实品种已接目录精度 |
| 052b成交量 | `volume_shares`为股；`volume_lots`为股数/100 |
| 052d、分时、快照、成交的量 | 按品种/周期核实源单位；不能统一乘100，标准K线转换显式传 `VolumeUnit` |
| 金额 | 当前052d与052b金额均为非负有限IEEE754 binary32；不能套报价金额算法；052b旧指数算法缺少隐含首位，已修复 |
| 财务 | 逐字段读取unit、semantic_status和evidence；`value()`仅提供已验证值；`candidate_value()`是显式候选换算，已知证据冲突时返回None并提供`candidate_rejection()` |
| 0ffc比例 | 是百分点，不是0..1比例；净额沿源total_amount单位 |

`CanonicalBar::provenance` 保留原始category、command/market/start/count、股票或指数解码类型、转换前成交量、源量单位及依据。网络标准化另存端点、连接ID/代次和接收时间；离线解码不补造网络来源。`Period::Day`不能还原category 4/9，`Minute1`不能还原7/8，应保存请求元数据。旧 `from_kline` 缺少请求信息，明确保留 `request=None`；可重建来源时使用 `from_kline_request/from_index_kline_request`。`CallerSpecified`只表示调用方选定单位，`Referenced`保存调用方维护的版本/证据标识，都不自动代表交易所核验。

`None` 表示字段未提供、未知、缺精度或业务不可用等状态，不等于数值0。业务消费不得用 `unwrap_or(0)` 隐藏未知；保留原始值、资格或不可用原因。缺失证券不补造零行情，空页/空Vec不直接判定停牌、退市或无交易。

`requested_date` 是请求日期，`fetched_at/observed_date` 是获取或本地观察时间，不能互相替代。0537/0feb的分钟标签是按索引合成的；0feb旧日期错位已定位为请求编码错误，修复后的两节点、六证券、六交易日样本与同日0fb4和日线一致，见[修复证据](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。读取日期字段不等于完成业务日期验收。

0feb解码默认为`Unverified { reason: ServerDoesNotEchoTradingDate }`，`require_trading_date()`返回Unsupported。调用方确有独立带日期参照时，可用`MinuteDateReference::new`声明完整证券、日期、标尺、全部分钟价格点、HTTPS来源和原始文件SHA-256，再调用`with_date_reference`。该入口重解原始正文，要求全部价格点匹配，并绑定请求、节点、连接、获取时间、目录代次、正文和解码字段；成功后为`IndependentlyReferenced`。这只表达调用方证据范围内的日期参照，不是主站回显、通用日期验收或量/均价/合成标签资格。任何公开字段或来源被修改，getter、日期门禁和序列化均降为未核实。按交易日分区或发布必须在最终使用前调用门禁；非空、240点或请求日本身不能资格化。

`MinuteSeries::compare_reference(&reference, cumulative_volume_raw)`对完整独立价格点及可选原始累计量作诊断，返回全部价格/累计量是否匹配、差异索引、合成标签及会话边界位置。累计量须与参考价格逐点对齐、非负且不递减；重解正文后拒绝被改写的DTO。该接口不提升日期、时间或量单位资格。2026-10-27平安银行209点参照中，11:30价格raw为1146/1147、累计量913147/913222，末点另有量差；首点09:31的39200已包含此前会话量。保留差异，不整体平移标签、补09:30或删除119点以通过日期门禁，见[本轮修复](../archive/development/LIVE_MARKET_LEGACY_FIX_20261027.md)。

`RECENT_MINUTES_DATE_SCHEMA_VERSION=2` 的selector为有效YYYYMMDD整数的32位补码，即 `date.wrapping_neg()`，按小端发送。旧schema1使用ordinal公式，20260817/18实际发送了20260609/10的selector；历史缓存必须保留旧请求参数、字节与schema，不能直接给旧数据重新标为8月。离线核验工具对缺少版本字段的旧材料按schema1读取，新采样必须写入 `recent_minutes_date_schema_version=2`。

2026-10-27收盘后扩大到两节点各228次请求，独立回放共43,200点。当前采样最早非空日期为sz000001的20000609、sz159915的20111209、sh510300的20120528；各自前一个已采交易日期为空。33组日期/证券的两节点分钟原值存在差异，sz000001与官网day的106组开末价对照中35组有差异；当日20261027不在该冻结day文件中。以上只登记请求日期可得性及源差异，不推导连续历史、准确全点、固定保存年数或永久保存。原帧及逐点差异索引见[能力补证](../archive/development/UPSTREAM_CAPABILITIES_FIX_20261027.md)。

成交事件的 `num/trade_count_raw` 不是稳定成交ID。保留状态5/8和未知事件，先保存原始事件再按业务资格筛选；`ConfirmedByProfile` 是协议配置规则确认，不是交易所独立核验。末价×聚合量属于估算，不是精确源成交额。

### 2.1 字段消费步骤

1. 使用对应命令的checked请求/解码入口，先处理错误，再按请求与返回身份关联证券。正文解码成功只说明该布局检查通过，不能给全部返回字段统一加“业务已验证”。
2. 明确日期和时间的来源：区分响应日期槽、请求日期、本地获取日、bar时点和合成标签。需要实际交易日的消费必须满足相应日期资格；0feb按上文门禁拒绝未经核实的日期分区。
3. 依据命令、品种、category及目录精度选择单位，按[字段状态](../reference/RESPONSE_FIELD_SEMANTICS.md)读取相应资格getter或元信息。财务默认业务值、显式候选值、成交协议分类和源金额/估算额分别标记；其他DTO没有统一semantic_status，不能补造Verified。
4. 按第3至5节检查分页完成、逐项批量错误及订阅有效状态。字段合法与范围完整分别判定；需要独立核验的业务只接受证据适用范围内的值，候选/未知不能静默进入该业务结果。
5. 在应用模型和缓存中保存市场/代码、请求/协议、单位与价格口径、日期依据、可得来源/获取时点、规则/证据版本、资格及完成状态。None、错误、缺失证券及未完成原因随结果保留，不用零值或删除失败项代替。

财务使用`FinanceSnapshot::require_fields(&[...])`声明非空、唯一的必需字段；任何候选/未知、未核实报告期或被改写的语义版本都拒绝整组消费。该门禁验证数值资格，不自动证明TTM、年度化、基本/稀释或合并口径。固定样本矩阵在[finance-sample.json](../../config/consumption/finance-sample.json)，通过`scripts/validation/verify_consumption_fields.py`同时核验原响应、参考数据和发行人原文件摘要；其他证券、期次和字段须独立补证。

默认财务另覆盖市场0/000001、更新日20261024、报告期2026-09-30的17个逐位匹配字段。现金净变动源值85.15亿元与公告58.72亿元冲突，`value()`及`candidate_value()`均为None，`candidate_rejection()`为`EvidenceConflict { expected_raw_bits }`；raw原值保持可见。该拒绝也适用于自定义profile已登记但未匹配的字段；不能改用候选值绕过已知冲突。未适用profile的字段仍按原基线候选规则处理。Q3独立期望及公告原字节见[夹具](../../crates/rustdx-core/tests/fixtures/finance-q3/expected.json)，旧H1矩阵不变。

该固定记录可调用`FinanceSnapshot::cash_flow_reconciliation()`取得`FinanceCashFlowReconciliation`诊断：公告经营283,221、投资6,503、筹资−281,209百万元合计8,515，汇率影响−2,643后净增加5,872百万元，严格解释两值差额。返回源bits、不含汇率合计、各活动/汇率/披露净额（均为元）及公告URL。入口检查证券、更新日、报告期、语义/profile版本、经营现金流和现金槽原位模式，未覆盖范围返回Unsupported。该解释不重新命名或资格化原`CashFlowNetChange`，不会替换其原值/拒绝规则；固定独立期望见[现金口径夹具](../../crates/rustdx-core/tests/fixtures/finance-cash-basis/expected.json)。

`DayFileReference::new`校验调用方提供的day原字节、SHA-256、市场/代码、HTTPS来源、32字节布局及严格递增唯一日期；`compare_page`重解052b正文，拒绝错证券、被改写的DTO或参考缺日。结果逐条同时给出未修改的`tcp`、来源为`TdxDayFile`的`reference: CanonicalBar`及`differing_fields`，另存双方来源与参考摘要。业务显式选择reference时须同时留存差异；接口不静默替换Client、CLI或runner的052b原值。摘要证明字节一致性，day文件本身没有证券身份槽，外部来源/摘要属于该证券及原TCP请求的真实性仍由调用方核实。四证券27,237条核对中有1条股数差9股及43条金额差1ULP，不能补股或放宽误差把源差异变成精确一致。

| 实际消费范围 | 单位/日期与资格 | 证据与边界 |
| --- | --- | --- |
| 0010固定sz000001更新20260815、2026-H1的10字段 | 逐raw_bits换算；报告期及必需字段门禁 | 固定矩阵核验；14命名槽仍候选、7槽未知 |
| 0010固定sz000001更新20261024、2026-Q3的17字段 | 逐raw_bits及公告报告期；EPS为本年前三季度基本/稀释口径 | 7命名槽候选，其中现金净变动拒绝候选换算；7槽未知 |
| 052b与显式day参考核对 | 原TCP及参考标准值并列、逐字段差异、来源摘要 | 参考不改变原TCP资格；全点对比保留44条源差异，调用方核实来源身份 |
| 052b runner沪深A股、已结束日线 | 整数股数仅转f64一次，日期来自原响应；固定volume证据摘要 | 精确原帧重放与全部字段回读；真实独立核验仍限已有24条 |
| 052d其他品种/category、近期分时量 | 调用方声明单位与来源，保留转换前值；没有通用Verified | 不继承052b或其他category资格；扩大范围先补证 |
| 0fc5/0fc6事件、估算额与源额 | 协议事件资格、估算与报告金额分别存储 | 独立业务核验仍待补；估算不能替代源额 |
| 统计资源候选字段及日期 | require_complete仅检查资源末尾；raw槽仍候选 | ZIP/CRC/GBK与键核验不证明业务单位/日期 |

F10文本的complete表示指定字节范围读齐，不代表指标已经结构化；供应商板块/资金分类不作为官方行业、上市状态或资金账户事实。需要进一步抽取或核验时，由应用另外记录抽取规则和独立依据。

板块成分使用`Client::block/get_block_info`，完整字节使用`Client::block_file/download_block_file`。下载成功表示02c5声明的长度及32位十六进制MD5与完整文件一致，不能由单独的`block_metadata`查询推导；空文件同样验算MD5。`BlockFile::decode_records/decode_block_content`严格检查槽位整除、容量、GBK及六位ASCII数字代码，畸形文件返回Err。旧`parse_block_content`仍为容错兼容入口，不用于证明完整性；下载MD5匹配也不代表成分类别具有官方权威性。

通用资源使用`Client::file_content/resource_file`，采用300字节路径、声明块长和未知尾部合同。完整下载必须检查`complete/termination`或调用`ResourceFile::require_complete()`；字节/块数/响应预算耗尽得到的前缀不能当全文件，即使前缀本身能解析。短/空块只说明观察到主站资源末尾，不提供声明MD5或下载期间原子性。`resource_stats`要求完整文件、ZIP大小/中央目录/CRC、严格GBK及唯一CFG键；`StatsRow.candidate_values`全部仍是候选语义，保留`fields_raw`、`stats_date_raw`和`unparsed_lines`，不能按字段名自动升级为已核验价格、股本、金额或交易日期。资源字节与raw_payload须另存，serde输出不含它们。

DTO序列化不等于原始证据归档。`CommandPage.raw_payload`、`DelistedBarsPage.raw_payload`及`BlockFile.content/BlockMetadata.raw_payload`在serde输出中被跳过，普通QuoteData/KlineData也未暴露全部原始槽；需要重放时显式保存原字节或启用FrameObserver，并关联请求、响应身份和运行/归档ID。离线结果缺少的网络来源不补造为现场采样。

完整分钟档案使用独立合同，见[ADR-0002](../architecture/decisions/0002-minute-archive.md)；
候选结构与入口为[示例](../../config/consumption/minute-archive.example.json)和
[audit_minute_archive.py](../../scripts/validation/audit_minute_archive.py)。
每个JSONL行包含market、code、date、slot、price_minor（人民币分）、volume_shares、
amount_minor（人民币分）、cumulative_volume_shares及cumulative_amount_minor；
每个分区分别固定日期、槽位、价格、量、额独立证据和全天控制量额。Trading必须完整覆盖声明槽，
停牌/摘牌明确免行情且有点时事实；不从请求日期或合成标签赋予资格。
每个证据含相对path、bytes和SHA256；独立资格另含HTTPS source及independently_reviewed，
原取得另含endpoint、captured_at、response及按来源要求的request/origin_evidence。
校验covered只说明固定字节和调用方声明一致，不证明来源真实权威。
retention的days、max_bytes、max_files、max_freshness_seconds由业务部署方明确填写，
缺值拒绝；revision/source_as_of不回退、保留窗口内分区不丢失，结果记录原始来源分类及稳定重放摘要。
输出放在档案外的新文件；候选校验不发布、不删除，也不把RequestedRefresh升级为PassivePush。

## 3. 分页完成状态与失败

不得对所有命令使用“返回条数少于count就结束”的统一规则。手动分页按该命令的**原始返回数量或字节长度**推进，不能按过滤后的成交数、去重后数量或文本字符数推进偏移。

| 查询 | 短页/短块 | 正常完成依据 | 未完成处理 |
| --- | --- | --- | --- |
| 完整044d目录 | 按实际条数继续 | 达到044e预期总数量 | 提前空页、重复或跨来源报错 |
| 052d/052b区间K线 | 状态入口按raw条数继续；052d旧Vec在短页停止 | 本连接观察到空页或已越过begin | 状态入口预算/短页停止为 `complete=false`；Vec不证明完整历史 |
| 0452特殊表、052b退市日线 | 继续 | 实际空页 | 预算/偏移终止可能返回Ok，但 `complete=false` |
| 054b分类排行、056a竞价过程 | 短页结束 | 短页或空页 | 预算终止可能返回Ok，但 `complete=false` |
| 0fc5/0fc6完整成交枚举 | 按raw条数继续 | 观察到空页，必要时同连接复查 | 预算、偏移、排序、来源变化等返回Err |
| 02d0完整F10范围 | 按实际字节继续 | 达到目录指定字节长度 | 提前空块、预算不足、GBK无效等返回Err |
| 02c5/06b9完整板块文件 | 每块精确匹配请求字节数 | 完整长度与声明MD5一致；成分另经严格解析 | 任意短块、hash错误、换节点/连接/北京时间观察日等返回Err，重采从元数据开始 |
| 06b9通用资源 | 按声明chunk_len推进，未知尾部另存 | ShortChunk/EmptyChunk才complete；统计ZIP另验CRC/GBK/键 | ByteBudget/ChunkBudget/ResponseBudget是不完整前缀；换来源或畸形块返回Err |

消费 `CommandCollection/DelistedBarsCollection` 前必须检查 `complete`、`termination` 和 `atomic`。不完整数据如需保留，必须在应用自身结果中明确标记，不能发布为完整历史或完整榜单。

`bars_range_with_options/index_bars_range_with_options` 返回 `BarRangeCollection`，先检查 `complete/termination/coverage`，再读取请求/实际窗口及每页信息。`SourceLowerBoundReached/SourceEnumerated`仅表达该来源的枚举过程；缺页、停牌、交易所全历史、未闭合K线和盘中冻结一致性仍需独立依据。相同时间戳仅内容完全相同才去重，冲突、分页不推进、来源或北京时间观察日期变化返回Err；空页复查保持同连接。预算拒收的页 `accepted=false`，其收到的raw行数/字节仍计量，记录不并入records。max_bytes限制累计解压字节，可因最后一个有界页/确认响应超出预算；超限页不会成为可用记录。

`TransactionCollection` 成功只说明本次主站枚举观察到空页；`SourceEnumerated` 同样不证明交易所全量历史、主站保留期或冻结盘中快照。网络/解码/取消/超时的Err不得转换为成功空结果。重连、换主站或适用的跨日检查失败后，应重新开始该次枚举。

## 4. 批量结果与对象生命周期

`io::Result<Vec<io::Result<T>>>` 的外层成功只表示批次交付结果，应用须逐项检查内层错误。K线批量返回 `BatchKline/BatchBars`，检查每项 `result`。

| 批量入口 | 结果单位 |
| --- | --- |
| `snapshots_batch` | 一个最多80证券的请求分片，不是一只证券 |
| 副图、小图、近期分时、资金流向、竞价批量 | 一个证券/请求 |
| 板块元数据批量 | 一个文件 |
| F10正文批量 | 一个指定文件字节范围 |
| 退市单页批量 / 全量批量 | 一个单页请求 / 一个证券分页集合 |

批量结果按输入请求或分片顺序返回；不要把该约定推广到单次快照内的证券顺序。需要继续其他证券时保留失败项和原因，不能静默删除失败证券。

所有Vec批量共享`Client::set_batch_result_budget(BatchResultBudget)`：默认总100万行/256MiB、单项65536行/32MiB，输入仍最多10万项。超限停止调度并返回外层Err，不交付截断成功Vec；取消/总期限也返回外层Err。普通查询错误保留种类、输入顺序及最多2048字节诊断。结果槽位在分配前核算；退市集合的合并记录与原页记录两份行存储都计入预算。

`max_retained_bytes/max_item_retained_bytes`核算嵌套Vec/String容量、原始Arc正文及日期资格绑定，共享Arc按每个引用重复计费以保守控制。此预算只约束保留结果：最多工作线程数个在途结果仍受各自协议/分页上限约束，解码临时值、目录、输入、线程栈、socket、分配器不在其中。进程内存限制须结合RSS测量及部署配额；不能把256MiB解释为进程峰值上限。

`k_batch_stream/bars_batch_stream` 和扩展命令的 `*_batch_stream` 在调用线程按输入顺序逐项回调，单项Err保留索引/身份。最多并行许可数个项目在采集或等待交付；交付后才为该工作线程调度下一项。回调返回Err、总行/JSON字节预算耗尽、取消或总deadline到期时停止，已交付数据由调用方保留并标记任务状态。回调自身采用协作取消；阻塞回调不能被强行中断，其耗时会在下一项交付前检查。

`BarBatchStreamOptions`包含区间预算和 `BatchStreamOptions`交付预算。量预算按输出点/条目计数，F10正文和板块元数据各计一个对象；字节预算是成功值JSON长度，不是RSS或原始证据字节，序列化只计量不构建JSON副本。单项超限变为逐项Err，总量超限返回外层Err；分页集合还须检查complete。证券级流式仍保留单证券历史，调用方持有已交付对象会自行增加内存；原始payload、解码对象和分配器开销不计入JSON字节。

`KlineData<'a>/IndexKlineData<'a>/BatchKline<'a>/BatchBars<'a>` 的代码借用输入字符串，输入生命周期必须覆盖结果使用期。应用跨线程存储、缓存或持久化时应采用拥有证券身份的自有模型或明确的标准化模型，不把全部Client结果视为完全owned。

完整离线调用链见[checked_owned_bars](../../crates/rustdx-core/examples/usage/checked_owned_bars.rs)：
显式Symbol/Period → KlineRequest → checked decoder → owned CanonicalBar → 原子JSON。
借用请求释放后仍保留市场、周期、价格口径、来源单位及依据、原请求页和解码器；
缺少网络取得信息时保留None。示例不授予日期、量额或生产发布资格。
`LegacyIndicatorError`是原根路径错误的明确别名；
`IncrementalIndicatorError`是原`indicators::IndicatorError`的明确别名，适用于try_ema/try_macd及增量状态。
两条旧路径保留，两类错误不隐式互换。

多Client应用可按业务进程生命周期持有`Arc<QuoteMetadataStore>`，先预热全部所需市场，再用`with_quote_metadata`共享，见[示例](../../crates/rustdx-core/examples/usage/shared_quote_metadata.rs)。借用查找不分配临时代码String；每市场刷新原子替换该市场，其他市场及旧Arc读者保留原世代。跨日仍拒绝过期目录，进程间Store不共享。

## 5. 0547订阅与推送状态

每个 `QuoteSubscription` 独占一条受管连接，并关闭该连接的自动空闲心跳；不能把订阅socket交回普通查询池。`subscribe_depth` 额外占用一个连接许可，原Client查询连接仍保留。

业务消费用`QuoteSubscription::synchronized_snapshot()`，它同时检查受管连接、目录新鲜度和全部订阅键基线。`refresh/rebuild`返回Ok仍可能是部分基线；队列溢出、重建、部分证券先恢复及另一证券后续推送补齐均由该门禁回归覆盖。普通`snapshot()`用于诊断，可以返回AwaitingBaseline/Invalid/Closed。

| `DepthSyncStatus` | 应用消费规则 |
| --- | --- |
| `AwaitingBaseline` | 显示等待基线或部分数据，不能当作完整订阅集合 |
| `Synchronized` | 可按证券完整状态消费，保留来源、接收时间与游标 |
| `Invalid` | 停止将残留quote视为当前有效行情，刷新目录后 `rebuild` |
| `Closed` | 会话已关闭，后续需要重新创建 |

通过 `refresh/poll_push/poll_update/snapshot/rebuild/close` 管理会话；局部poll超时的None不证明没有更新。收到更新后**替换该证券状态**，不要再次累加累计成交量、成交额或内外盘。

需要持续更新时调用`poll_update(idle_timeout)`，等待基线时立即主动refresh；其余状态先等被动推送，局部空闲超时再主动refresh。idle_timeout必须大于0，调用继承受管连接、父任务期限、取消、队列及失效处理。返回`QuoteUpdate::PassivePush(QuotePush)`或`RequestedRefresh(QuoteRefresh)`，调用方必须保留来源区别，随后用`synchronized_snapshot()`检查完整消费资格；无后台线程或自动重连。`poll_push`仍只读被动推送。2026-10-27收盘前两节点各4次调用均为RequestedRefresh，行情及游标变化已独立回放；这关闭无被动推送时客户端无持续更新入口的问题，不证明主站被动推送能力。

refresh发送前本地已收到的半帧即使message id碰巧等于新请求id，补齐后也按被动帧入队，继续等真正响应。该边界只覆盖已观察到的本地前缀；不能据此断言服务器未读帧的产生时点。模拟回归已验证先7字节、同id推送游标11、真正响应游标12的归属。固定eltdx3.3.0源码未发现0547额外激活步骤；另一TQ接口的`subscribe_hq`不能作为0547被动推送证据，见[补证报告](../archive/development/UPSTREAM_CAPABILITIES_FIX_20261027.md)。真实被动推送资格仍须盘中单独取证。

队列默认且最大256帧/8MiB。队列溢出、游标回退、断线、精度跨日等失效需要显式恢复，不能静默丢帧并继续标记同步。`close/Drop`释放连接。同步会话实现及工程测试通过不代表真实盘中主动推送已完成验收，实际交易窗口边界见[推送验证](../archive/validation/QUOTE_PUSH_VALIDATION_20261024.md)。

严格验收工具还要求最终快照完整同步、连接/代次不变、全部订阅键和报价身份一致；观察起止使用精确UTC+8时点，不用整数秒取整跨越午休/收盘。证据核验检查事件时钟单调、接收时间范围及无被动请求。2026-10-24仍休市，工程修复不能关闭盘中门禁；新结果与可运行入口见[资源与推送验收修复](../archive/development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md)。

## 6. 连接预算、超时、重试和缓存

1. 在首次读取网络policy/stats或建立连接前，调用一次 `configure_network`。默认全部受管连接共享**进程内8个许可**，不是每Client8个。需要同机多个进程共用上限时显式配置 `SharedNetworkPolicy`，目录与容量应一致。
2. 应用需要普通行情TCP时可在启动调用 `tcp::startup::initialize_with_config`；首次TCP补测也可用。CLI/runner本地文件与运维默认不启动行情TCP，`--tcp-warmup`显式预热；显式TCP来源自行核验冻结端点。显式地址也须属于所选当日有效IP池；复用Client，不按每次请求新建连接。首次匹配客户端可接收预热连接。
3. 批次内按预算有界并行，多个批次在同Client排队；当前批量连接结束后释放。`max_parallel=0`自动选择并受预算限制，不能用裸socket或不受管连接后仍声称受该上限保护。
4. 根据业务设定单请求、分页、批量及整任务的期限/取消。当前总体默认请求15秒、分页120秒、批量300秒，Tcp读写基础超时5秒；外层 `OperationContext` 只能缩短期限。队列等待、目录加载、分页、重试、延迟和空页复查都消耗预算。
5. `auto_reconnect` 默认0，`recheck_empty` 默认false。需要重试时有界配置；重连可能使已有分页/订阅失效，不能直接续接旧偏移。空结果复查方式随接口不同，不能绕过来源检查。
6. 目录、历史、F10和板块尽量缓存，行情只刷新业务需要的末端。缓存键保留市场、代码、协议/周期、区间、单位/价格口径和所需版本；需要身份/精度来源的查询保留相应元信息。Client自动缓存的是精度目录，不是所有行情。

`MemoryCache`按到期时间优先驱逐，增量计量键/值字节；`get_shared`返回Arc视图，驱逐后已有视图仍有效，其占用由调用方负责。`FileCache`默认目录4096项/256MiB，`try_with_capacity`可配置；同目录实例应使用相同预算并通过OS锁协调写入/删除。读取过期项删除它，写入清理过期/中断暂存并优先驱逐最早到期项；提交文件受总预算限制，原子暂存另可能占一个最大64MiB项。目录扫描仍有成本，200000条目上限拒绝异常目录；不在缓存文件命名范围内的文件不清理。

文件缓存目录锁等待上限为5秒，父任务期限/取消可提前终止。Windows使用一个持续排队的
原生锁请求，避免连续写入抢占等待中的线程或进程；等待期间至多20毫秒重查任务。
取消或超时后先撤销请求并确认内核完成，再释放请求内存；若取消与获取锁同时完成，
还须显式解锁。底层驱动的取消确认及已开始的文件I/O无法强行抢占。

`Cache::get_or_fetch_checked(key, &context, fetch)`使合并锁等待、二次读取及返回
共用父任务期限/取消，`CacheFetchError::Operation`与`Fetch`分开保存。
同线程同条带重入返回WouldBlock；慢fetch无法被强行抢占，闭包须协作检查任务
或设置传输超时，超期返回不会成功交付或写入fetch结果。旧`get_or_fetch`
仍无合并等待期限，同条带重入直接fetch；不要将它用于必须遵守期限的任务。
连接池满额等待至多20毫秒重查父期限/取消；预留槽位在失败/提前退出时自动释放。

本地复权使用完整Symbol，所有日线和GBBQ事件必须同证券，非除权类或旧日期事件
也不能混入。day字节封装使用`Fq::new_checked/concat_checked`传播身份错误；
增量基准证券匹配且日期早于首日。日历2.0所有原名称及try入口返回Result，
未知覆盖不是休市或有效交易日，跨范围导航/计数/列表均拒绝。

响应前缀、opcode或消息号错配返回`InvalidData`，受管连接失效且该次不自动重试；应用不得把错误转换成空行情。需要重采时应按分页/订阅合同重新建立查询。离线归档重放使用`tcp::decode_response`或CLI的`tcp_evidence::replay`，原始`decode_frame`只检查长度和解压，不证明请求身份。

库采用同步I/O和有界线程。异步应用应在受控的阻塞工作线程中调用，仍共享同一网络预算。手册中的建议间隔不是官方QPS或数据刷新保证；全市场周期按实际分片、页数、冷目录加载和重试次数计算。

网络策略与配置细节分别见[网络预算](../architecture/NETWORK.md)、[启动与IP池](../architecture/DAILY_IP_POOL.md)；接口具体上限以[26命令手册](../reference/COMMAND_API.md)为准。

所有独立池、每日刷新和手动诊断共享单独的探测配额，默认进程8个连接。启用共享业务许可目录时，探测自动使用其`probes/`子目录的8个OS锁槽；首次探测前可通过`ip::configure_probes(ProbePolicy)`独立配置，同目录所有进程须保持容量一致。等待许可消耗总体deadline并检查取消；探测许可不占业务许可，统计见`ip::probe_stats()`。

日常握手/证券数量通过仅说明普通协议可用。`ip::capability_observation(endpoint, opcode)`记录最新的传输身份通过、正文解码通过或传输失败；有来源附加的checked命令在解码后记`DecodedResponse`。`pool.capable_nodes(opcode, max_age)`只返回仍在当前有效池、最新状态为正文通过且未过期的节点，不主动探测或兜底。观察限4096组，溢出淘汰最旧项。空正文通过也可能进入该状态，业务仍须检查所需证券、日期、覆盖、单位；0547可解码不能证明实际主动推送。旧入口未附加来源时只有传输级观察，不能因历史累计成功数自行升级资格。

runner的`Error::Cancelled`与`Error::TimedOut`分别表示取消和超时，`category()`穿透`Stage`；账本detail保存该分类。runner CLI的执行错误以JSON写stderr，含status/error_category/error并退出1；参数解析错误仍按argh处理。超时在既有状态机中作为隔离失败，待核对进程/发布状态时仍优先进入reconciliation。目录计量每秒采样并在worker退出后完整核对；它是软监控，目标磁盘硬限额仍由部署环境落实。

## 7. 接入示例

以下示例配置受管网络、初始化与精度预热，保留快照缺失证券、检查每个批量结果，并在退市分页不完整时返回错误。应用根据自己的业务选择数据、错误处理和预算。

```rust,no_run
use rustdx_complete::tcp::{
    configure_network, startup, NetworkPolicy, TcpConfig,
    stock::{Client, CommandScanOptions, IntradayAuxKind},
};
use std::{io, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_network(NetworkPolicy {
        max_connections: 8,
        request_timeout: Duration::from_secs(15),
        pagination_timeout: Duration::from_secs(120),
        batch_timeout: Duration::from_secs(300),
        ..Default::default()
    })?;
    let config = TcpConfig::default();
    startup::initialize_with_config(&config)?;
    let mut client = Client::with_config(&config)?;
    client.refresh_quote_metadata(&[0, 1])?;

    let stocks = [(0, "000001"), (1, "600000")];
    let quotes = client.quotes_with_metadata(&stocks)?;
    for missing in &quotes.missing_codes {
        eprintln!("本次未返回证券：{missing:?}");
    }
    for (stock, result) in stocks.iter().zip(
        client.intraday_aux_batch(&stocks, IntradayAuxKind::VolumeComparison, 2)?,
    ) {
        match result {
            Ok(series) => println!("{stock:?} 副图点数={}", series.page.records.len()),
            Err(error) => eprintln!("{stock:?} 查询失败：{error}"),
        }
    }

    let archive = client.delisted_bars_all(
        0, "000038",
        CommandScanOptions { max_records: 20_000, ..Default::default() },
    )?;
    if !archive.complete {
        return Err(io::Error::other(format!("退市枚举未完成：{:?}", archive.termination)).into());
    }
    println!("该主站可枚举退市日线={}条", archive.records.len());
    Ok(())
}
```

集成验收应核对应用实际证券/市场、日期、单位、预算、缺失/失败处理和订阅恢复。源码可用、编译成功、返回非空、枚举到主站空页分别是不同条件，不能相互替代业务验收。

## 8. 指标增量消费

`indicators::{EmaState, MacdState, MacdParameters, try_ema, try_macd}`提供参数checked入口及跨块常量内存状态。EMA使用每段首个有限价格作初值，`alpha=2/(period+1)`；MACD信号也用首个有限差值作初值。默认柱状图为`MACD-Signal`，双倍口径显式设置`histogram_scale=2.0`。周期为0、非有限或非正倍数返回Err，即使输入为空也校验；快慢周期大小顺序不限制。

`update(None)`、NaN、Infinity重置该段EMA并输出None；有效输入的EMA溢出也重置。MACD差值溢出重置信号线，柱状图溢出仅该值为None。旧`ema`把缺口表示成NaN、非法周期全NaN，旧`macd`非法周期全None且仍用一倍柱状图；批量和增量共享递推。

调用方按证券、频率、价格/单位口径独立持有状态，以时间升序更新已收盘bar。API不推断时间戳、收盘资格、停牌日或复权版本；重复输入会再次递推。盘中预览或修订bar前clone检查点，修订时恢复并重放，不能在已推进状态上再次追加同一bar。

## 9. TCP日线采集与runner发布

runner输出仍使用`a-share/day/raw/cny/shares/v1`，新采集输入合同为`a-share/tcp-052b/day/raw/cny/integer-shares/v2`，请求及逐证券收据schema=2。配置`tcp_day`显式端点、单位依据SHA-256及页/单证券原始行/单证券正文/全部报文预算；与本地day目录互斥。新runner冻结`Request.schedule`，支持最多10000只排序唯一沪深A股，最多8个同步采集线程并受共享网络许可约束。旧无schedule请求维持串行、32只上限及原字节离线重放；新采集拒绝旧052d合同。不隐式映射北交所，不接收五分钟、复权或当天日线。

只读`plan`按实际应有键核对事实范围，同时给出每目标数据库操作/证明上限、全部目标证据字节/条目及余量；每行1024字节是采集前保守上界，封存后仍按真实字节重算。预算不足时`can_start=false`，建议窗口只在日期之间分割并保留同日完整名单；单日无法容纳仍拒绝。按时间顺序执行窗口，保持同一事实版本、完整catalog连续性及修订/source_as_of单调；plan不会核验既有catalog、创建账本、读取凭据或联网。实现锚点为[plan.rs](../../apps/rustdx-runner/src/pipeline/plan.rs)与[capacity.rs](../../apps/rustdx-runner/src/database/capacity.rs)。

计划枚举完整应有键后再判定`max_rows`，超限报告`expected_row_budget`；窗口同时满足行数与全部数据库预算，`row_budget_fits=false`的单日不能通过拆证券名单执行。运行时保持原行数硬上限。

[日调度器](../../scripts/ops/daily.py)按独立交易日历、批准发布时刻/滞后及固定catalog生成完整日期任务，保留超出批次上限的积压。来源就绪记录绑定模板摘要、实际获取日、有效期和明确修订范围；普通刷新只补缺日期，修订已有日期须明确提高来源修订。每个任务在recover/run前按实际文件摘要、完整键集合与目标证据重新准入，缺证据保留积压并返回失败；恢复耗时后再次核验来源、事实和准入。

原价日线使用`SecurityHistory::from_state_dataset`核验公告支持的状态事实；这不延长规则截止。默认`TryFrom`仍严格验证规则，`context_at`只在Trading且`rule_at`已核实的日期返回上下文。维护记录须绑定该dataset、实际日历/独立参考/证券历史/名单摘要和任务覆盖范围；规则到期单独告警，不能靠续写日期取得涨跌幅资格。

可用`Config.tcp_day_schedule`显式冻结`max_parallel/timeout_seconds/max_raw_rows/max_payload_bytes`。省略时并行取`workers.min(8)`，期限取整任务扣除发布预留，原始行总量取单证券上限×证券数（至多10亿），正文总量取`max_source_bytes`。总行、CSV、所有wire/receipt归档仍独立限制。每个窗口启动前预留单证券最大原始行、正文上限加131070字节的最后一页/空页确认余量、receipt及wire空间；不足时减小窗口或拒绝启动，不截断为成功。正文预算计解压后页面，不是RSS，网络压缩字节和探测报文另计。保守预留可能在仍有少量余额时拒绝下一证券，应按计划调整单证券上限。

最多一个窗口的单证券历史在采集；线程只返回计数，等待窗口全部join后，在调用线程按输入顺序独立回放并写CSV，未使用的预留才可复用。任一线程失败取消本采集上下文、等待所有线程退出并保留部分原码；不取消父任务。父期限始终可缩短计划期限。CSV写入先核预算；失败不生成complete报告或推进目录。schema8的`acquisition`记录实际总原始行、正文、归档、wire记录、证券数及窗口并行度，runner再次校验冻结计划和归档总量。

分钟、快照与订阅的持续采集是单独的应用合同，不随日线范围扩展启用：

| 待接入任务 | 应有键及完成条件 | 留证与修订条件 |
| --- | --- | --- |
| 分钟bar | 完整证券×明确交易日×周期×时点，收盘与日期资格明确；缺时点不可完整 | 原请求/帧、目录精度及日期依据；固定窗口重放，新修订独立release |
| 快照 | 明确证券集合×采样窗口；全部证券有符合窗口及精度资格的状态 | 原请求/帧、实际接收时点与缺证券；采样窗口不宣称原子交易所快照 |
| 0547订阅窗口 | 全部订阅键完整基线、游标单调，断线/溢出后全部恢复；窗口结束有明确状态 | 基线与全部主动帧、队列/连接/重建轨迹；事件身份和恢复资格单独设计，不能套用日线catalog |

这些任务尚无runner执行或发布入口；工程合同准备不代表已经采集或业务验收。

[任务准入工具](../testing/TASK_ADMISSION.md)读取版本化证据矩阵，按任务独立检查应有键、
时间/最终性、修订、断流/溢出、全部必需字段资格及目标部署证据。日线历史不能替代
分钟量单位、快照窗口或0547盘中被动推送；Unknown拒绝，缺runner入口也拒绝。
证据封套一致不自动改变本页getter资格或执行数据发布。

已有旧来源release需要精确股数时，冻结新版来源材料、提升`source_revision`并重新采集，形成新release；旧release及固定版本导出继续保留原字段，不从旧052d报文补造股尾数。

`planned_date=source_as_of`绑定实际北京时间采集日；所有目标bar日期严格早于采集日，收盘依据记录为`prior_exchange_date_local_clock`。`acquired_on`随请求冻结并在回读核对；单证券分页不能跨观察日期。日线DTO的15:00是解码合成值，心跳日期/主站空页不用于证明收盘或真实交易日。日期资格依赖可信本地时钟和冻结日历；今天补采的旧bar不能标记为历史时点版本。整批跨日会被后续采集/回读拒绝。

单位材料必须先由维护者审核，再以实际字节SHA命名放入facts。`Referenced`与哈希表示可追溯依据，不自动认定独立核实。新入口`Client::daily_bars_range_with_options`读取052b非负u32整数股数，直接精确扩宽为f64；金额按响应binary32扩宽，原价不做复权。其预算、空页复查、越过from停止及连接/观察日期检查与既有区间入口一致；重叠比较绝对价格、量、金额原码和尾槽，页内差值不作为跨页相等依据。`DailyBar/DailyBarsRequest/DailyBarsPage/decode_daily_bars`是旧`Delisted*`名称的通用别名。schema8 CSV的`vol`为股，schema6/7 day CSV仍为手，消费方必须检查报告版本、`source_contract`与`volume_unit`，不能仅凭相同列名套旧转换。

2026-10-24原052d category9样本仍存在−94至+1股差异，原字节与[历史结果](../testing/evidence/acceptance-stage5-20261024/tcp-independent.json)不变。category4的binary32股量同样可能量化，不能恢复精确股尾数。修复后两个实际主站、3只在市沪深A股、2026-10-19至22的24条052b日线经独立原帧解码，与收据、CSV及已留存官网day的全部8字段精确一致，两个CSV字节相同。见[新证据](../testing/evidence/tcp-day-volume-fix-20261024/independent.json)与[修复报告](../archive/development/TCP_DAILY_INTEGER_SHARES_FIX_20261024.md)。这只确认所列证券、日期、节点；其他品种及交易所完整历史仍按实际范围验收。

`raw/<code>/wire`包含请求及完整解压前帧，`receipt.json`关联连接ID/代次、端点、时间、分页及标准结果。发布者通过`visit_records/decode_response/decode_stock_bars`独立重读每个序号与全部字段，复核原始偏移、空页确认及重复边界；不信任CSV与其自报摘要。可枚举范围只证明此来源停止位置，schema8明确`business_coverage=unverified`，最终由日历×名单×证券状态逐键核对。缺键、交易状态冲突或多余键拒绝发布；不能用行情为空推断停牌/退市。

所有来源共用release及catalog。重复同Job无新采集；补采使用更高source_revision，源截止不得降低。封存候选恢复只从归档继续核验/CAS，未封存中断隔离，显式修订后从偏移0重采，不能拼接新连接页面。消费方固定catalog版本，已发布版本不会因新采集变动。

示例见[runner配置](../../config/runner/tcp-day.example.json)及[操作说明](../../apps/rustdx-runner/README.md#tcp日线job)。本地压缩/重叠页模拟、篡改拒绝、缺键、修订与恢复证明工程行为；真实证券单位/日期/覆盖、全市场吞吐和目标Linux仍需独立验收。

## 10. 恢复后的固定版本消费

消费会话固定dataset及catalog版本，数据库导出选择该版本归档的目标ID和release身份；控制层restore校验文件/账本和请求证据，不连接或恢复数据库服务端。恢复后文件导出成功不能单独证明数据库恢复成功，必须分别读取所需历史版本并精确核对全部键、字段及行数。

查询使用已有账本只读打开，不初始化/迁移；迁移和写入持控制锁，空root不是有效的空数据集。备份/恢复目标以已有父目录实际路径检查隔离，清单和完成标记计入总字节预算。恢复账本的原Attempt路径作为审计事实保留；新的`restore-validated.json.original_roots`把逻辑定位映射到当前`attempts/<id>`，留存预览验证该映射及数据库proof原字节。恢复不会通过改写旧路径、摘要或Receipt来适配新root。

`PinnedDataset.catalog()`返回固定目录的共享只读引用，清单保留在会话内。一次导出只有一个活动数据库会话，连续同release复用；目录日期切换release时，先检查完整release总数及进程退出，再释放网络许可。A/B/A重新打开A并再次检查完整总数，各日期仍精确核对全部键/字段和终止空页。查询审计使用独立`consumer-audit`会话目录及SHA收据，至多50,000请求、200,000条目，字节上限为选中release冻结workspace预算最小值；不追加发布`target_io`。完成标记只证明回读核验，CSV仍须原子提交。消费者持审计共享锁，控制层备份另取排他审计锁，活动回读时拒绝备份；静止后成功与失败审计均计入备份预算、库存摘要及隔离恢复。审计总磁盘硬配额/保留期仍由部署批准，预览继续保护该目录。

账本schema4把原观测与可重建计数/阶段汇总在同一事务写入；metrics区分失败、unavailable及缺失记录。旧schema3备份可原字节恢复，但当前查询要求显式`migrate`；迁移异常回滚，`rebuild-metrics`在根锁内重建。不要通过改写历史JSON或只降低schema号回退。归一化输入固定后只检查变化部分，最终静止树完整核对及release摘要/清单预算仍执行。

`plan --config`只读现有事实/源/磁盘，不启动网络、创建root或迁移账本；输出应有键总数/摘要和有上限样本、初始有效并发及限制项、已知最小固定输入字节、最坏预留缺口和中断期限。初始可启动不保证源历史可枚举完整，Unknown仍拒绝；运行时重新冻结。该计划不授予分钟、快照或订阅资格。

归档的数据库目标包含端点、库/表/集合及凭据环境变量名。restore不会自动替换端点或修改原始收据；目标迁移时须按原身份核查恢复编排，不能改冻结请求/收据或摘要来指向另一库。第六阶段在相同loopback端点/命名空间、另一组全新数据目录原生恢复后，固定v1/v2与原文件精确一致；生产新端点迁移未据此通过。执行方法和范围见[部署恢复](../../deploy/production/runner/README.md#5-备份与隔离恢复)及[原生恢复验收](../archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)。
