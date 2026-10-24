# checked协议接口与独立解码

本接口属于2026-10-23起仓库源码，2026-10-24更新财务语义、报价精度与26命令接入。Cargo版本未变，尚未发布。同步TCP、网络许可、重试与整任务时限沿用现有实现。

外部应用共同遵循[调用约定](../development/APPLICATION_INTEGRATION.md)，按市场身份、单位/未知值、完成标志、批量错误、订阅状态和共享预算消费结果。全部高层/低层签名及结果字段见[26命令接口手册](../reference/COMMAND_API.md)，业务含义、候选单位、未知槽及证据范围见[字段解析状态](../reference/RESPONSE_FIELD_SEMANTICS.md)；本页说明构包与解码边界。

checked解码只在其实现的布局、身份和数值条件内拒绝不合法响应，不自动验证所有字段业务含义。原始DTO、标准化模型和兼容视图的保留信息不同；财务、事件、日期、分页及订阅使用各自资格API，不能用解码成功替代这些检查。0feb按日期消费须调用require_trading_date，默认Unsupported，调用方独立参照只资格化具体绑定结果；统一消费步骤见[应用接入第2.1节](../development/APPLICATION_INTEGRATION.md#21-字段消费步骤)。

## 响应身份校验

全部受管7709收帧入口共用响应前缀、opcode及消息号校验，覆盖旧K线/快照/财务/成交请求、新命令、握手、0004心跳和当日IP探测。`Tcp::send_recv`只读头的入口同样核对身份。完整帧在解压、正文解析和成功统计前校验；错配返回`InvalidData`、连接不可复用，不执行该次自动重连重发。

观察器保留已读完整的原始错配响应，并追加失败事件。离线重放使用`decode_response(request, header, body, decompress)`；`decode_frame`继续作为不匹配请求的原始解压工具。裸socket的读取由调用方负责。仅持有解压正文的独立`decode_*`函数不具备响应头身份信息。

| 新请求 | 参数与响应边界 | 独立解码 |
| --- | --- | --- |
| `KlineRequest` | 显式市场、六位ASCII代码、周期0–11、1–800条；日期/时间/数值检查 | `decode_stock_bars`、`decode_index_bars` |
| `QuotesRequest` | 1–80个不重复的市场/代码对；返回身份须匹配且不重复；显式目录精度 | `decode_quotes(request, bytes, precision)`、`decode_quotes_with_metadata` |
| `FinanceRequest` | 单证券；数量0/1、身份一致、浮点有限、日期有效；raw及逐字段单位/证据 | `decode_finance`返回FinanceSnapshot、`decode_finance_raw` |
| `TransactionRequest` | 1–2000条，u16分页偏移；检查响应数量、时分、正价格、非负量和原始笔数槽 | `decode_transactions` |
| `HistoryTransactionRequest` | 上述分页参数及有效YYYYMMDD；历史逐笔没有笔数槽，兼容返回num=0 | `decode_history_transactions` |
| `EnhancedHistoryTransactionRequest` | 独立0fc6，1–1800条、u16偏移、有效YYYYMMDD；五个varint及参考价，完整raw保留 | `decode_enhanced_history_transactions` |
| `SecurityDirectoryRequest` | 044d，market 0/1、u32偏移、1–1600条；完整37字节记录与f32字段 | `decode_security_directory` |
| `TodayMinuteRequest` | 0537，显式目录精度；完整三varint记录、已支持会话最多240点 | `decode_today_minutes` |
| `Heartbeat` | 0004，12字节空体请求；至少10字节确认，原始日期及保留字段 | `decode_heartbeat` |
| `BlockMetaRequest` | 02c5，1–40字节ASCII文件名；至少38字节响应，MD5位于偏移5；保留raw和合法尾部 | `decode_block_metadata` |
| `CompanyInfoDirectoryRequest` | 02cf，market 0/1/2、trim后六位数字；152字节目录记录，GBK名称/文件名与原始字段 | `decode_company_info_directory` |
| `CompanyInfoTextRequest` | 02d0，GBK编码文件名1–80字节、正长度、offset+length不溢出；完整读取先拼字节再解码 | `decode_company_info_text`、`company_info_text` |
| `DailyBarsRequest`（旧名`DelistedBarsRequest`） | 052b，market 0/1/2、trim后六位数字、u16偏移、1–800条；整数股数与非负有限binary32金额 | `decode_daily_bars`、`delisted_bars_all`、`Client::daily_bars_range_with_options`（range市场0/1） |

checked请求参数私有，构造校验后编码只依赖不变参数；Heartbeat使用Default构造。财务结果保留原始136字节主体，成交事件页与增强历史页保留完整原始正文；解码不修改请求，拒绝截断、超请求数量和布局不允许的尾随字节，02c5/02cf/02d0声明允许的尾部保留raw。单页错误不交付部分行；扫描预算是否允许返回不完整集合按接口分别判断。未知逐笔方向值原样保留，不强行解释为买卖。财务日期0表示来源未知，负利润/现金流合法。行情无返回不能据此认定停牌或退市。

增强历史`Client::enhanced_history_transaction`为独立单页入口；其`EnhancedHistoryTransactionPage`保留完整payload、参考价原字节、请求身份/日期和实际响应来源。`status=8`优先分类为快照，非8的09:25为开盘撮合；其他未知状态仍按SDK兼容视图保留，不作为已核实业务资格。`next_start()`按原始条数checked推进，空页和偏移上限分开。旧0fb5布局没有笔数槽，不能替换opcode复用旧解码。字段、频率及实帧范围见[0fc6实施记录](../archive/development/ENHANCED_HISTORY_0FC6_IMPLEMENTATION_20261024.md)。

`Client::quotes/finance/transaction/history_transaction`已迁移新路径。旧`SecurityQuotes/FinanceInfo/Transaction/HistoryTransaction`类型和构造器保留，`try_parse`复用独立解码，`parse`遇错清空data；`SecurityQuotes`离线须`with_precision`，live可加载目录。旧构造器仍可能panic，公开字段和发送字节仍可能被调用方改得不一致，建议迁移新请求。

财务先将f32扩展为f64，再按字段换算：股份×10000、金额候选×1000、EPS/每股净资产/人数原值，未知槽仅raw。`value()`仅输出逐字段证据匹配的已验证值，`candidate_value()`显式选择候选；初始证据限平安银行固定2026-H1样本。完整旧pytdx转换由`decode_finance_legacy/recv_legacy/finance_legacy`提供，行业/省份只保留原编码。报告期、EPS及权益口径不从数值等式推断。

报价先扩宽base/delta为i64相加，再除完整044d目录提供的2/3/4位除数。Client持独立`Arc`目录快照；冷加载和报价共享15秒总体请求时限，显式refresh使用120秒分页时限。live在传输前后检查北京时间自然日及未来时间，失效拒绝；离线允许显式历史快照。完整目录失败不覆盖旧版本，缺证券不触发反复重拉。旧固定/100由`decode_quotes_legacy/recv_legacy/try_parse_legacy`显式提供。细节见[迁移](../release/MIGRATION.md)和[实施记录](../archive/development/FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)。

## 0fc5/0fc6全量分页

`Client::transaction_all(market, code, options)`和`history_transaction_all(market, code, date, options)`返回`io::Result<TransactionCollection>`。已有单页接口保留。非空页按原始`len`推进，短页继续；仅实际空页确认结束，启用`recheck_empty`时在同一连接再查一次。页序反转，页内顺序保留；同分钟/同价量/同num记录不去重，状态5/8和未知值不筛除。

默认1800条/页、100个逻辑页（含终止空页）、67535条原始记录、8MiB解压正文、120秒总时限、页间等待100ms。0fc6每页上限1800，旧0fb5与当日0fc5的构造上限2000不证明所有主站支持2000。网络策略及外层OperationContext可进一步缩短时限；空页复查至少等待300ms且正文也计入字节预算。100ms仅是可调客户端节流。达到条数预算时允许一条记录的空页探测，非空即报错。

每次构包前将宽位偏移checked转换为u16，下一页超过65535返回`OffsetLimit`；页数/条数/字节超限、连接代次或主站变化、当日查询跨北京时间日期、重复整页或逆序时分均明确失败。相同地址重连也可检测；重连发生在首个成功页之前时可建立新枚举会话，之后不拼接。`TransactionPaginationError`可通过io错误的`get_ref().downcast_ref()`取得原因、失败偏移及已收集页/条数；网络/解析/取消/超时保留io kind。

`TransactionCollection`保留请求日期、客户端观察日、主站、连接代次、时间戳及每页偏移/数量/结果索引范围/字节/调用次数，新增`event_pages`保留原始正文、参考字节、笔数/状态/尾槽和来源。`termination=EmptyPageObserved`只证明主站本次可枚举范围结束，不证明冻结盘中数据或指定交易日完整。原始完整帧仍可通过既有FrameObserver留证。当前默认协议及旧入口见[四命令实施说明](../archive/development/FOUR_COMMANDS_MODERNIZATION_20261024.md)。

## 带周期及单位的K线

```rust
use rustdx_complete::{
    data::{Period, Symbol, VolumeUnit},
    tcp::stock::KlineRequest,
};

let symbol: Symbol = "sz000001".parse()?;
let request = KlineRequest::for_symbol(&symbol, Period::Minute5, 0, 3)?;
let bars = request.recv_canonical_stock(&mut tcp, VolumeUnit::Shares)?;
```

`for_symbol`拒绝未核实的北交所TCP映射。`decode_canonical_stock/recv_canonical_stock`与指数对应入口从请求推导周期，调用方必须明确源成交量单位，产物标记TdxTcp及Raw。不推断分钟、指数、基金的量单位；未核实单位须先查源规则。标准CanonicalBar没有指数上涨/下跌家数，需要这些字段时继续使用`decode_index_bars`。旧`CanonicalBar::from_kline`与`Bar::from_kline`继续保留兼容行为。

## 九个新增7709命令

`0452/051b/0547/054b/054c/056a/0fd1/0feb/0ffc`已接入独立checked请求、owned结果及Client入口。[完整API与字段约束](../archive/development/NINE_COMMANDS_IMPLEMENTATION_20261024.md)列出6个批量和3个扫描方法；与此前四命令迁移一起达到22项同码覆盖，字段业务验收另计。

公共frame层构建请求并检查响应前缀/opcode/消息号；新解码使用有界Reader、有限f32和有符号i64变长整数。CommandPage共享原始正文与来源，扫描标识预算终止/非原子状态，并拒绝页间换址、同址重连或跨观察日。054c/0547要求有效目录精度，054b的未知品种精度返回None并保留raw。

0547会话独占连接、关闭该socket的空闲心跳，配对刷新响应和独立PushObserver。poll局部超时保留半帧；队列/暂存和解码内存同时有界，溢出/错帧/游标回退/观察器失败/取消/撤销失效并释放连接。rebuild递增代次，清空旧基线并从游标0重取。未收到全部证券基线前不报告Synchronized。

原九命令阶段两节点132组实帧以独立Python布局及算术核验；后续0feb已取得非空响应，但与同请求日的其他接口存在日期对照冲突。0547真实盘中主动推送仍待交易窗口；当前边界见[真实合同报告](../archive/validation/LIVE_COMMAND_CONTRACT_VALIDATION_20261024.md)，不能把传输/解码回归当作全部业务验收。

## F10与退市结果消费

`Client::f10/f10_categories`和自由函数`get_block_info`的相关元信息/正文路径已迁移；完整F10范围按实际字节读取至目录指定长度，中文跨块时拼齐后一次解码，预算不足或提前空块返回Err。退市日线扫描按原始条数续页、短页继续，预算/偏移终止可返回 `complete=false`；相同日期冲突或整页无新增日期报错。两者不能复用同一种“短页结束”规则。

052b成交量为变长整数股数，052d成交量为相应源编码；052d与052b当前金额均按非负有限IEEE754 binary32解码，052b旧指数算法漏隐含首位已修复。报价金额算法不能通用套用。具体消费条件、批量结果单位及恢复要求由[外部应用调用约定](../development/APPLICATION_INTEGRATION.md)统一说明。

## 交叉验证

2026-10-23 07:31 UTC+8从115.238.56.198:7709保存原始正文；[夹具清单](../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)记录来源、采集时间、请求/正文/预期CSV摘要。参考是pytdx提交`2857fdad08534533610bd2aeca387f760c4baa42`，执行前核对全部引用源码SHA-256，运行时无pytdx依赖。

[验证脚本](../../scripts/validation/verify_protocol_pytdx.py)无`--capture`时离线重解已有正文，带该参数才重新联网采样。Rust回归逐字段核对两个快照、单证券财务及当日/历史逐笔各三条，并覆盖逐字节截断、超量、尾随、返回证券错配/重复、NaN/Infinity、非法日期/时分/负量、旧状态清空和Client实际发送字节。K线原有九条真实样本继续通过；新增标准周期、市场、量换算及北交所拒绝回归。样本范围有限，不代表全市场或所有服务端版本。
