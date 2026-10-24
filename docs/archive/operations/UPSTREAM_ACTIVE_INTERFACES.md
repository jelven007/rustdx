# rustdx 主动获取上游接口与字段字典

更新日期：2026-10-24。源码基线：`408e30e`，核心/CLI Cargo版本仍为1.12.2/1.8.2；已发布同版本不包含后续源码改动。本文说明程序主动发起的TCP/HTTP请求及当前公开业务字段，不把Job配置、数据库发布或本地事实维护当成行情源接口。

字段名、类型和换算以当前源码为依据；固定报文及独立pytdx期望证明样本解码一致，不构成交易所正式协议、全市场完整性或长期可用性承诺。源码注释尚不能独立证明的业务编码明确标注“未证实”。初版没有重新采集行情；10月24日新增K线成交量专项真实采样，见第4节，未生成模拟行情数据。

## 1. 主动获取入口

| 上游 | 入口 | 获取内容 | 当前接入位置 |
| --- | --- | --- | --- |
| 通达信普通行情TCP | `Client::quotes/bars/index_bars/minute/transaction/finance/xdxr/stocks/f10/block`及区间/批量方法 | 快照、股票/指数K线、分时、逐笔、财务、除权、证券目录、F10、板块 | 核心库 |
| 上交所/深交所HTTP | `get_offical_stocks/get_sh_stocks/get_sz_stocks` | 本次接口筛选范围内的证券代码集合 | CLI名单获取 |
| 通达信官网HTTP ZIP | `prepare_hsjday_dirs/prepare_hsjday_dirs_from`，`rustdx day` | 日线包、来源元信息及解压目录 | CLI下载/文件解析 |

当前runner只消费明确本地day目录，不主动调用以上TCP采集接口。库的`Symbol`支持`bj`不等于普通TCP支持北交所；普通TCP市场仅0深圳、1上海。

依据：[Client](../../../crates/rustdx-core/src/tcp/stock/client.rs)、[官方名单](../../../apps/rustdx-cli/src/fetch_code.rs)、[下载](../../../apps/rustdx-cli/src/download.rs)。

## 2. 通用请求与连接字段

### 2.1 行情请求参数

| 参数 | 类型 | 业务含义 | 规则 |
| --- | --- | --- | --- |
| `market` | `u16`，部分旧请求为`u8` | 交易所身份，0深圳、1上海 | 必须与代码一起保存，不单用数字代码关联证券 |
| `code` | 六位字符串 | 上游证券代码，保留前导零 | 普通请求不带sh/sz前缀；生产键另补市场 |
| `stocks` | `(market, code)`列表 | 多证券请求范围 | 快照1–80只，checked请求拒绝重复；批量K线逐项返回结果 |
| `category` | `u16` | K线周期 | 见下一表 |
| `start` | `u16` | 页内历史偏移 | K线从最新记录方向偏移，0表示最新窗口；逐笔为协议分页偏移；不是日期 |
| `count` | `u16` | 本页最多请求多少条 | checked K线1–800；checked逐笔1–2000，实际返回可少于请求 |
| `begin` | `Option<u32>` | 区间起始日，YYYYMMDD，闭区间 | None不设下界；服务端历史深度仍可能有限 |
| `end` | `Option<u32>` | 区间结束日，YYYYMMDD，闭区间 | None不设上界；返回最新K线可能尚未收盘 |
| `date` | `u32` | 历史分时/逐笔查询日，YYYYMMDD | 不提供任意历史可取的保证 |
| `max_parallel` | `usize` | 批量请求的并行连接上限 | 0按CPU核数选择，仍受共享网络许可约束 |
| `adj` | `Adj` | 本地价格复权方式 | Qfq前复权、Hfq后复权；原上游K线不直接提供本项目价格复权结果 |

| category | 周期 | 当前标准Period选择 |
| --- | --- | --- |
| 0 | 5分钟 | Minute5 |
| 1 | 15分钟 | Minute15 |
| 2 | 30分钟 | Minute30 |
| 3 | 60分钟 | Hour1 |
| 4、9 | 日线编码 | 标准Day使用9 |
| 5 | 周线 | Week |
| 6 | 月线 | Month |
| 7、8 | 1分钟编码 | 标准Minute1使用8；不承诺两编码的源历史完全相同 |
| 10 | 季线 | Quarter |
| 11 | 年线 | Year |

`k`等价于日线区间查询；`bars_range/index_bars_range`自动分页、时间排序及去重。股票与指数应选各自解码器，价格单位也不同。

依据：[周期定义](../../../crates/rustdx-core/src/data/model.rs)、[checked K线](../../../crates/rustdx-core/src/tcp/stock/kline_request.rs)、[checked快照/财务/逐笔](../../../crates/rustdx-core/src/tcp/stock/requests.rs)。

### 2.2 TcpConfig与请求包装

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `timeout` | `Duration` | 单连接/读写适用时限；还受总体deadline和网络策略限制 |
| `ip` | `Option<SocketAddr>` | 指定上游端点；None使用内置服务器轮换/故障转移 |
| `auto_reconnect` | `usize` | 首试失败后的重连重试次数，默认0 |
| `retry_delay_ms` | `u64` | 第n次失败按基数×n退避，默认0 |
| `recheck_empty` | `bool` | 对适用Client入口的空结果等待后重连重查，默认false；空结果仍不证明停牌 |

旧公开协议对象的工程字段统一解释如下：

| 字段 | 含义 |
| --- | --- |
| `send` | 已编码请求字节；修改旧对象业务字段不一定同步修改它 |
| `response` | 接收到的解码前协议正文，区别于完整TCP帧 |
| `data` | 解析后的结果集合、文本或字节块 |
| `market/code/category/start/count/date/stocks` | 上述请求身份及分页参数 |
| `filename/block_file/length/size` | 文件接口的名称、偏移和字节长度，见F10/板块章节 |

新调用优先使用Client和私有参数checked请求。旧构造器/parse仍有panic或空结果掩盖解析失败的兼容边界，不能统一宣传同等输入保证。

依据：[TCP配置](../../../crates/rustdx-core/src/tcp/mod.rs)、[协议边界](../../architecture/PROTOCOL_REQUESTS.md)。

## 3. 通达信实时快照

入口：`Client::quotes(&[(market, code), ...]) -> io::Result<Vec<QuoteData>>`。

checked解码核对返回证券属于请求、不重复，拒绝截断/尾随与非法数值。`QuoteData`没有market和完整日期字段；调用方须保留请求身份、采集时点，并按原代码关联返回，不能假设顺序就是身份。

### 3.1 基础行情字段

| 字段 | 类型 | 业务含义 | 单位/口径 |
| --- | --- | --- | --- |
| `code` | String | 六位证券代码 | 不含市场，保留前导零 |
| `price` | f64 | 快照当前成交价 | A股人民币元；解码已还原价格差值 |
| `last_close` | f64 | 上游前收/昨收基准 | 元；公司行为日须核实是否为除权参考价，不盲当昨天原始close |
| `open` | f64 | 当前行情日开盘价 | 元 |
| `high` | f64 | 当前行情日截至快照时的最高价 | 元 |
| `low` | f64 | 当前行情日截至快照时的最低价 | 元 |
| `vol` | f64 | 截至快照的累计成交量 | A股手，100股/手 |
| `cur_vol` | f64 | 源提供的现量/最近成交量口径 | 手；不是每分钟增量，也不保证一条原始交易所逐笔 |
| `amount` | f64 | 截至快照的累计成交额 | 元 |
| `s_vol` | f64 | 源侧成交分类量 | 手；源码注释标作外盘，独立参考仅保留字段名，分类方向未证实 |
| `b_vol` | f64 | 另一源侧成交分类量 | 手；源码注释标作内盘，独立参考未证明该分类方向 |
| `change` | f64 | 库计算的price−last_close | 元 |
| `change_percent` | f64 | 库计算的相对前收涨跌幅 | 百分数值，乘100；last_close为0时库给0，不表示真实零涨幅 |
| `speed` | f64 | 源涨速字段 | 解码有符号rev9÷100；统计窗口未证实，不写成固定“5分钟涨速” |
| `servertime` | String | 源服务器时分秒字符串 | 如HH:MM:SS.mmm，无日期；异常解码为空，不是本机采集时间 |

内外盘通常用于描述主动成交方向，不能代表资金净流入；这里尤其不能未经对照直接将s_vol/b_vol映射为主动买/卖。源未交易、断流、停牌与数据未返回须通过其他事实区分。

### 3.2 五档订单簿字段

以下20个字段均为f64；价格为A股元，数量为手。挂单量是快照时刻尚在订单簿的量，不是已经成交的量。

| 字段 | 业务含义 |
| --- | --- |
| `bid1` | 买一报价，当前最优买价 |
| `bid2` | 买二报价 |
| `bid3` | 买三报价 |
| `bid4` | 买四报价 |
| `bid5` | 买五报价 |
| `ask1` | 卖一报价，当前最优卖价 |
| `ask2` | 卖二报价 |
| `ask3` | 卖三报价 |
| `ask4` | 卖四报价 |
| `ask5` | 卖五报价 |
| `bid1_vol` | 买一挂单量 |
| `bid2_vol` | 买二挂单量 |
| `bid3_vol` | 买三挂单量 |
| `bid4_vol` | 买四挂单量 |
| `bid5_vol` | 买五挂单量 |
| `ask1_vol` | 卖一挂单量 |
| `ask2_vol` | 卖二挂单量 |
| `ask3_vol` | 卖三挂单量 |
| `ask4_vol` | 卖四挂单量 |
| `ask5_vol` | 卖五挂单量 |

某档为0可表示该档缺失，不能由此断定价格为零或触及涨跌停。pytdx的bid_vol1等字段在本库命名为bid1_vol等。

依据：[快照结构及换算](../../../crates/rustdx-core/src/tcp/stock/quotes.rs)、[固定报文/全部字段期望](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)、[已读取的固定pytdx参考](https://raw.githubusercontent.com/rainx/pytdx/2857fdad08534533610bd2aeca387f760c4baa42/pytdx/parser/get_security_quotes.py)。参考证明编码/字段对应，不能证明未说明的成交分类或涨速窗口。

## 4. 股票与指数K线

入口：`bars/bars_range/k/k_batch/bars_batch`返回股票K线；`index_bars/index_bars_range`返回指数K线。上游原价；最新日/周/月窗口可能还在形成，调用方须核实收盘状态。

| 股票KlineData字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `dt` | DateTime | 该周期K线的源日期/时间；本库按UTC+8解释 |
| `code` | &str | 请求证券的六位代码，市场由请求补充 |
| `open` | f64 | 周期内首价，股票元 |
| `high` | f64 | 周期内最高价，股票元 |
| `low` | f64 | 周期内最低价，股票元 |
| `close` | f64 | 周期内末价/尚未收盘时最新价，股票元 |
| `vol` | f64 | 源编码的直接解码量；已测股票日线`9/1`为手、`4/1`及5分钟`0/1`为股；其余周期/品种须核实 |
| `amount` | f64 | 该周期成交额，元 |

10月24日[成交量专项验证](../validation/KLINE_VOLUME_UNITS_20261024.md)覆盖平安银行、浦发银行、沪深300ETF和上证指数，24组真实请求。同周期38/54字节响应正文全部相同；股票/ETF的`9/1`量约为官方`.day`量的1/100，`4/1`及5分钟`0/1`接近`.day`股/份量。ETF按份理解；上证指数日线仅确认源字段比例，指数分钟量与日线聚合量未对齐，仍待核实业务口径。

默认`Period::Day`发9，默认5分钟发0。已测股票标准适配分别传`VolumeUnit::Lots`和`VolumeUnit::Shares`；不能将所有K线统一乘100转股。`Period::try_from(4/9)`均返回Day，原始周期编号和源单位需另行保留。该结论不推广到其他分钟周期、债券、北交所或其他服务器。

DateTime五个字段均为u16：

| 字段 | 业务含义 |
| --- | --- |
| `year` | 四位公历年份 |
| `month` | 月 |
| `day` | 日 |
| `hour` | 小时；非分钟周期在本库补为15 |
| `minute` | 分钟；非分钟周期在本库补为0 |

日线默认15:00是本库时点表达，不证明数据已经收盘。周/月等K线日期与窗口边界还须按源规则处理。

IndexKlineData包含以上同名8个字段，并额外包含：

| 字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `up_count` | u16 | 源统计的上涨证券家数，具体成分/比较基准未单独核实 |
| `down_count` | u16 | 源统计的下跌证券家数，不能用两者之和推断完整成分数量 |

指数OHLC表示指数点位，不是人民币股价。指数vol保留源量，不自动套用100股/手；amount保留源成交额口径。股票/指数同用命令0x052d但响应格式不同。

批量结果的所有字段：

| BatchKline/BatchBars字段 | 业务含义 |
| --- | --- |
| `market` | 该单项请求市场 |
| `code` | 该单项请求代码 |
| `result` | 该证券独立的Ok(K线集合)或Err；外层Ok不表示每一证券都成功 |

`k_adjusted`使用xdxr事件在本地调整OHLC，成交量/额保持源值。Qfq锚定后端可用历史末日，Hfq锚定可用历史首日；首日未必是上市日。应先计算可用历史，再筛选日期；不要把复权价用于原价涨跌停判定。

依据：[股票K线](../../../crates/rustdx-core/src/tcp/stock/kline.rs)、[指数K线](../../../crates/rustdx-core/src/tcp/stock/index_kline.rs)、[时间结构](../../../crates/rustdx-core/src/tcp/helper.rs)、[批量/复权](../../../crates/rustdx-core/src/tcp/stock/client.rs)、[独立K线夹具](../../../crates/rustdx-core/tests/fixtures/kline/manifest.json)。

## 5. 分时与逐笔

### 5.1 分时

入口：`minute(market, code)`、`history_minute(market, code, date)`。

| MinuteTimeData字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `price` | f64 | 源分时点价格，A股元 |
| `vol` | i32 | 源分时点成交量，源码口径为手；点对应区间及累计/增量业务规则须核实 |

该类型没有日期、时分、均价、成交额或市场代码。调用方须保存请求日期和点序；不能假设永远有完整240点后直接推导每点时间。当日接口为空时Client可回退当日历史分时；旧分时解析存在异常返回空集合的兼容边界，空数据不是停牌事实。历史深度由实际服务器决定。

依据：[当日分时](../../../crates/rustdx-core/src/tcp/stock/minute_time.rs)、[历史分时](../../../crates/rustdx-core/src/tcp/stock/history_minute_time.rs)。

### 5.2 当日及历史逐笔

入口：`transaction(market, code, start, count)`、`history_transaction(market, code, start, count, date)`。

| TransactionData字段 | 类型 | 业务含义 | 边界 |
| --- | --- | --- | --- |
| `time` | String | 源成交明细时分，HH:MM | 没有日期、秒或毫秒，多条可同一分钟 |
| `price` | f64 | 成交明细价 | A股元 |
| `vol` | i32 | 该明细成交量 | 手，checked拒绝负数 |
| `num` | i32 | 当日源明细中的编号/统计字段 | 源码称成交编号，唯一性及逐笔标识语义未证实；历史协议没有该字段，库补0 |
| `buyorsell` | i32 | 源买卖方向编码 | 源码常规解释0买、1卖、2中性；其他值保留，不自动归类 |

它是行情服务器提供的成交明细，字段不足以证明逐条对应交易所原始逐笔，也不含委托号、撮合号、买卖账户或逐笔成交额。需要可靠去重时，不能把num当作未经验证的全局唯一ID。

依据：[逐笔结构](../../../crates/rustdx-core/src/tcp/stock/transaction.rs)、[checked解码](../../../crates/rustdx-core/src/tcp/stock/requests.rs)、[独立报文夹具](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)。

## 6. 财务与股本

入口：`finance(market, code) -> io::Result<FinanceInfoData>`。下层FinanceRequest响应0条时返回空集合，高层Client转为NotFound错误，不提供默认全零财务。该接口是源当前财务快照，不是指定会计年度/报告期历史查询。

### 6.1 身份、日期与原始分类

| 字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `market` | u8 | 证券市场，0深圳、1上海 |
| `code` | String | 六位证券代码 |
| `province` | u16 | 源省份编码；本项目不提供已核实名称映射 |
| `industry` | u16 | 源行业编码；不直接当作申万/证监会/交易所某固定分类 |
| `updated_date` | u32 | 源财务更新日期YYYYMMDD，0表示未知；不是经证明的公告日/报告期 |
| `ipo_date` | u32 | 源上市日期YYYYMMDD，0表示未知；重大生命周期事实仍须公告核实 |

### 6.2 股本结构

以下均为f64，单位股；源以浮点万股表达的字段在本库先扩展f64，再乘10000。不保证恢复原始精确整数股数，不能把浮点编码精度当作整数统计精度。

| 字段 | 业务含义 |
| --- | --- |
| `liutongguben` | 流通股本 |
| `zongguben` | 总股本 |
| `guojiagu` | 国家股口径股本 |
| `faqirenfarengu` | 发起人法人股口径股本 |
| `farengu` | 法人股口径股本 |
| `bgu` | B股股本 |
| `hgu` | H股股本 |
| `zhigonggu` | 职工股口径股本 |

源历史分类可能重叠或沿用历史口径，不应直接相加上述分类以重建总股本。

### 6.3 资产、负债、经营与利润

以下除另注外均为f64、元。源万元字段先扩展到f64，再乘10000；这些字段的会计报告期、合并/母公司、单季/累计、审计及重述版本未由该接口显式返回。

| 字段 | 业务含义 |
| --- | --- |
| `zongzichan` | 总资产 |
| `liudongzichan` | 流动资产 |
| `gudingzichan` | 固定资产 |
| `wuxingzichan` | 无形资产 |
| `liudongfuzhai` | 流动负债 |
| `changqifuzhai` | 长期负债 |
| `zibengongjijin` | 资本公积 |
| `jingzichan` | 源净资产口径，具体归属范围未显式返回 |
| `zhuyingshouru` | 主营业务收入口径 |
| `zhuyinglirun` | 主营业务利润口径 |
| `yingshouzhangkuan` | 应收账款 |
| `yingyelirun` | 营业利润 |
| `touzishouyu` | 投资收益；字段拼写保留源码 |
| `jingyingxianjinliu` | 经营活动现金流口径，是否净额等须对照报告 |
| `zongxianjinliu` | 源总现金流口径，未显式给出现金流量表项目标准 |
| `cunhuo` | 存货 |
| `lirunzonghe` | 利润总额口径 |
| `shuihoulirun` | 税后利润口径 |
| `jinglirun` | 净利润口径，不自动等同归母净利润 |
| `weifenpeilirun` | 未分配利润 |
| `meigujingzichan` | 每股净资产，元/股；保留源值，不乘10000 |
| `gudongrenshu` | 股东人数，人数口径；保留源f32扩展值，不乘10000，未额外证明精确整数 |
| `baoliu2` | 源保留数值，业务意义/单位未证实，保留源值 |

利润、收益和现金流可为负值，checked解码只拒绝非有限值，不将亏损改为0。历史回测不能仅用updated_date把当前财务当作当时可知信息。

依据：[财务公开结构](../../../crates/rustdx-core/src/tcp/stock/finance_info.rs)、[实际换算/缺失/日期检查](../../../crates/rustdx-core/src/tcp/stock/requests.rs)、[财务独立期望](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)。固定pytdx财务源码的本次WebFetch返回空内容，本文不据此声称完成新的远端交叉核查；已有仓库参考摘要和夹具保留原轮次。

## 7. 除权除息及股本变迁

入口：`xdxr(market, code) -> io::Result<Vec<XdxrData>>`。

| 字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `market` | u8 | 市场身份 |
| `code` | String | 六位证券代码 |
| `date` | u32 | 源事件日期YYYYMMDD；事件类型决定其业务日期意义 |
| `category` | u8 | 源事件类别，决定后面四个字段的解释 |
| `fh_qltp` | f32 | 类别1：每10股派现，元；一般股本变迁：前流通盘源量 |
| `pgj_qzgb` | f32 | 类别1：配股价，元/股；一般股本变迁：前总股本源量 |
| `sg_hltp` | f32 | 类别1：每10股送转股数；一般股本变迁：后流通盘源量 |
| `pg_hzgb` | f32 | 类别1：每10股配股股数；一般股本变迁：后总股本源量 |

只有类别1的四个数值可按派现/配股/送转公式解释。一般股本变迁字段保留源规模，源码未统一规范到股，使用前须核实源单位，不能一律乘10000。

| category | 源码类别标签 | 使用边界 |
| --- | --- | --- |
| 1 | 除权除息 | 本项目价格复权使用的主要事件 |
| 2 | 送配股上市 | 股本/流通变化，不能当再次派息 |
| 3 | 非流通股上市 | 股本/流通变化 |
| 4 | 未知股本变动 | 含义未证实 |
| 5 | 股本变化 | 需核实原始规模单位 |
| 6 | 增发新股 | 特殊字段映射未完整核实 |
| 7 | 股份回购 | 不能单凭类别推导回购金额/账户 |
| 8 | 增发新股上市 | 股本/流通变化 |
| 9 | 转配股上市 | 股本/流通变化 |
| 10 | 可转债上市 | 事件标签，不能套派息公式 |
| 11 | 扩缩股 | 特殊比例字段；非类别1 |
| 12 | 非流通股缩股 | 特殊比例字段；非类别1 |
| 13 | 送认购权证 | 特殊字段单位/映射须独立核实 |
| 14 | 送认沽权证 | 同上 |

本项目类别1除权参考价为`(前收×10−派现+配股价×配股数)/(10+送转数+配股数)`，再按分舍入。股本变动、上市日期、除权日期和公告可知日期不能相互替代。

依据：[Xdxr定义/分类与源值解码](../../../crates/rustdx-core/src/tcp/stock/xdxr.rs)、[复权实现](../../../crates/rustdx-core/src/tcp/stock/client/adjustment.rs)。

## 8. 通达信证券目录、F10与板块

### 8.1 证券目录

入口：`stock_count(market)`返回u16证券数；`stocks(market)`按数量分页取得完整目录。

| SecurityListData字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `code` | String | 六位证券代码，市场从请求补充 |
| `name` | String | 当前源证券简称/名称，经GBK解码；不能推断历史名称或ST状态 |
| `volunit` | u32 | 源成交量计量单位信息；常见A股100表示一手100股，其他品种须核实 |
| `decimal_point` | u8 | 源价格小数位信息；不自动等同所有制度下的最小报价单位 |
| `pre_close` | f64 | 源前收价，股票元或指数点位等品种口径 |

目录包含源证券，不天然等同沪深A股全集，也不提供有公告证据的停牌/退市生命周期。

### 8.2 F10

入口：`f10_categories(market, code)`获取目录；`f10(market, code)`按目录取得栏目名与正文。

| CompanyInfoCategoryItem字段 | 类型 | 含义 |
| --- | --- | --- |
| `name` | String | 栏目名称，经GBK解码 |
| `filename` | String | 上游服务器文件名，后续内容请求使用，不是本地文件路径 |
| `start` | u32 | 栏目起始字节偏移 |
| `length` | u32 | 栏目字节长度，不是汉字/字符数 |

底层内容请求使用`market/code/filename/start/length`，返回`data: String`（GBK→UTF-8）；`f10`输出`Vec<(String,String)>`分别为栏目名和内容。正文为非结构化文本，接口不提供公告签名、可靠报告期或历史版本。

### 8.3 板块

入口：`block(block_file)`。常用文件为block_gn.dat概念、block_zs.dat指数、block_fg.dat风格、block.dat通用。先请求文件size，再按start/size分块获取data字节。

| BlockRecord字段 | 类型 | 含义 |
| --- | --- | --- |
| `blockname` | String | 源板块名称，经GBK解码 |
| `block_type` | u16 | 源板块类型编码，意义随文件类型变化，未建立统一稳定枚举 |
| `code` | String | 成分证券六位代码；没有市场，不应无条件按前缀推成股票 |

`BlockInfoMeta.size`是服务器文件字节数；`BlockInfoChunk.data`为内容字节段。返回板块定义是源当前观点，不是官方行业事实，也不提供历史生效区间。

依据：[证券目录](../../../crates/rustdx-core/src/tcp/stock/security_list.rs)、[F10](../../../crates/rustdx-core/src/tcp/stock/company_info.rs)、[板块](../../../crates/rustdx-core/src/tcp/stock/block_info.rs)。

## 9. 交易所官方名单

调用`get_offical_stocks("official"|"sse"|"szse")`，当前返回`HashSet<String>`：含市场前缀的代码集合，没有名称、行业、市值、上市日或历史状态。

### 9.1 上交所

接口：`https://query.sse.com.cn/sseQuery/commonQuery.do`，响应JSON或JSONP。

| 参数/响应字段 | 业务含义 |
| --- | --- |
| `STOCK_TYPE` | 1主板、8科创板，当前实现各自获取后合并 |
| `pageHelp.pageSize` | 每页条数，调用校验1–5000 |
| `pageHelp.pageNo/beginPage/endPage` | 当前页号，逐页读取 |
| `pageHelp.cacheSize` | 当前固定1的源分页参数 |
| `isPagination` | 当前true，启用分页 |
| `sqlId` | 源查询模板COMMON_SSE_CP_GPJCTPZ_GPLB_GP_L |
| `COMPANY_STATUS` | 当前筛选码2,4,5,7,8；不解释为本项目TradingState |
| `REG_PROVINCE/CSRC_CODE/STOCK_CODE` | 当前空过滤条件 |
| `type` | 源固定inParams |
| `jsonCallBack` | JSONP回调包装 |
| `result`或`pageHelp.data` | 证券记录数组 |
| `pageHelp.total`或`total` | 当前查询筛选范围总数，校验不超过100000 |
| `COMPANY_CODE` | 实际读取的六位代码，添加sh前缀 |

重复证券、total变化、提前空页或数量不一致均失败；取得的是当前查询条件范围，不代表全部生命周期或所有证券品种。

### 9.2 深交所

接口：`https://www.szse.cn/api/report/ShowReport?SHOWTYPE=xlsx&CATALOGID=1110&TABKEY=tab1`。

| 参数/输出 | 含义 |
| --- | --- |
| `SHOWTYPE=xlsx` | 请求Excel |
| `CATALOGID=1110` | 当前源报表目录 |
| `TABKEY=tab1` | 当前报表页签 |
| 工作表第0张 | 当前解析位置 |
| 第4列（从0计数） | 实际读取的证券代码，补足六位并加sz前缀；布局变化须更新解析 |

跳过表头、拒绝非法代码/重复/空集合。其他Excel列当前没有暴露为产品字段。SHSZ中的sh1/sh8/sz为内部本次分组计数，`count()`求和；不是保存的市场事实。

每次名单响应上限16MiB，上交所分页总体时限120秒，深交所请求适用120秒，并受任务上下文限制。

依据：[实际名单请求与读取列](../../../apps/rustdx-cli/src/fetch_code.rs)。

## 10. 官网日线ZIP主动下载

默认URL：`https://data.tdx.com.cn/vipdoc/hsjday.zip`。程序HEAD探测版本/大小，再GET完整或续传，缓存按来源URL隔离。返回`PreparedSource.dirs`为解压后的lday目录，持有来源锁至消费完成。

| HTTP字段 | 业务含义 |
| --- | --- |
| `ETag` | 源文件版本标记；不等同项目SHA-256 |
| `Last-Modified` | HTTP源修改时间；不是行情截止日或公告日 |
| `Content-Length` | 本次响应字节数；续传时不是完整文件大小 |
| `Range: bytes=n-` | 从本地已保存偏移续传 |
| `If-Range` | 续传版本条件，避免新旧文件拼接 |
| `Content-Range` | 206响应的start-end/total，核对偏移、长度及总大小 |
| `Accept-Ranges` | 源范围请求能力提示，实际206/字节边界仍须校验 |
| 状态200/206 | 完整/分段响应；续传收到200会从头处理，不直接追加 |

缓存Meta全部字段：

| 字段 | 含义 |
| --- | --- |
| `url` | 缓存来源身份 |
| `etag` | 保存的源ETag，可空 |
| `last_modified` | 保存的源修改标记，可空 |
| `size` | 保存的ZIP字节数 |
| `downloaded_ok` | 工作缓存下载完成标记，不表示行情完整 |
| `extracted_ok` | 解压完成标记，不表示业务覆盖通过 |
| `downloaded_at` | 本地下载记录时点，不是源交易日 |

解压day每条32字节，文件公开字段如下：

| Day字段 | 类型 | 业务含义 |
| --- | --- | --- |
| `date` | u32 | YYYYMMDD源行情日期 |
| `code` | String | 从文件名取得的完整市场前缀代码 |
| `open` | f64 | 源u32分÷100，开盘价元 |
| `high` | f64 | 源u32分÷100，最高价元 |
| `low` | f64 | 源u32分÷100，最低价元 |
| `close` | f64 | 源u32分÷100，收盘价元 |
| `amount` | f64 | 源f32成交额元扩展到f64 |
| `vol` | u32 | 源成交量股；兼容CSV序列化为手，标准CanonicalBar保留股 |

保留字节不暴露为业务字段。

HEAD最多30秒、单下载最多2小时，仍受整任务deadline限制。ZIP成功不等于应有证券/日期齐全；无人值守下载失败不依赖交互输入兜底。

依据：[主动下载/续传/缓存](../../../apps/rustdx-cli/src/download.rs)、[day记录定义](../../../crates/rustdx-core/src/file/day/mod.rs)。

## 11. 标准字段与追溯接口

### 11.1 CanonicalBar

| 字段 | 业务含义 |
| --- | --- |
| `bar` | 标准行情实体 |
| `period` | 周期；Minute1/5/15/30、Hour1、Day/Week/Month/Quarter/Year |
| `source` | TdxTcp/TdxDayFile/TdxMinuteFile来源 |
| `price_basis` | Raw/ForwardAdjusted/BackwardAdjusted价格口径 |

Bar全部字段：

| 字段 | 含义 |
| --- | --- |
| `symbol` | 市场与原六位代码的组合Symbol；完整显示为sh/sz/bj加代码 |
| `datetime` | UTC+8本地NaiveDateTime，类型本身不携带时区 |
| `open/high/low/close` | 标准股票价格元；指数原生点位应保持明确品种，不伪装股票元 |
| `volume_shares` | 已显式换算的股量；未核实品种/源单位不能自动适配 |
| `amount` | 元 |

Symbol的market/code为封装内部字段，通过方法访问；不能仅凭无市场code构造唯一身份。VolumeUnit::Shares不缩放、Lots乘100；这是当前固定A股手单位模型，不是所有品种可通用的单位系统。

ReturnIndex.value为累计收益指数；PriceMultiplier.value为价格缩放乘数，均要求正有限，不能互换。factor是兼容CSV字段，不自动具备两者任意一种含义。

### 11.2 TCP证据

| FrameEvent字段 | 含义 |
| --- | --- |
| `endpoint` | 实际上游SocketAddr |
| `request` | 实际请求字节，发送前观察 |
| `header` | 完整响应16字节头，仅Response事件 |
| `body` | 解压前完整响应body，仅Response事件，最大u16长度 |
| `kind` | I/O错误类别，仅Failed事件 |

FrameArchive的WireRecord全部字段为`schema_version`、`phase`、`captured_at`、`endpoint`、`request`、`header`、`body`、`error_kind`。分别表示库存格式版本、request/response/failed阶段、UTC留证时点、端点、原请求、可选帧头、帧体、可选错误类型。它不提供“停牌/不存在证券”等业务结论。

归档inventory.entries逐项保存file、sha256、bytes，表示记录文件名、该记录字节摘要及字节数。原字节可重放；SHA-256不是源真实性签名。load_response/replay核对库存、帧长度及解析请求，再调用checked解码；截断帧只记失败，不冒充完整响应。

依据：[标准模型](../../../crates/rustdx-core/src/data/model.rs)、[标准Bar/Symbol/单位](../../../crates/rustdx-core/src/data.rs)、[TCP事件](../../../crates/rustdx-core/src/tcp/evidence.rs)、[归档格式](../../../apps/rustdx-cli/src/tcp_evidence.rs)。

## 12. 未关闭的字段口径

| 项目 | 当前证据与限制 |
| --- | --- |
| s_vol/b_vol | 解码值有固定参考；源码注释的内外盘映射尚缺独立业务证据 |
| speed | rev9÷100已核对；统计窗口未明确 |
| 分时vol/点序 | 当前两字段模型；累计/增量及每点时间须源规则核实 |
| 逐笔num/方向 | 保留源值，历史num补0；不保证原始交易所逐笔唯一性 |
| 财务报告期/归属 | 接口没有报告期、合并口径、归母标记及可知时点，不能用字段名补齐 |
| xdxr非类别1 | 类别和四个复用字段并存，特殊单位/事件映射未全面核实 |
| 指数/非A股量单位 | 普通源量保留，未声明的品种/周期不自动乘100 |
| F10/板块/目录 | 当前源文本及分类，不提供官方历史生命周期事实 |
| HTTP固定协议参数 | 代码使用固定值不等于完整官方语义已证实 |

本次材料基于当前源码、已有固定报文与已读取pytdx快照参考；未新执行线上行情、全市场覆盖或长期负载验收。源码结构能够解释字段输出，不能替代对源经济含义、权限和生产适用性的核实。

## 13. 与通达信7709协议命令的关系

### 13.1 命令、请求对象、API和字段的层次

7709是普通行情TCP服务常用端口。线上命令是二进制操作，本库请求对象编码它，Client封装一次或多次请求，字段字典描述解码/适配后的结果；它们不是一对一的数量关系。

```text
Client业务API
    → 一个或多个Request与分页/重试
    → TCP二进制请求（命令码、参数、正文）
    → 完整响应与解压
    → 相应解码器
    → QuoteData/KlineData等字段
    → 可选标准适配、计算和发布
```

当前列出的请求在第10、11字节（从0计数）保存小端命令码。识别一次具体操作还须保留命令码以外的市场、代码、周期、偏移/数量、日期、文件名及会话参数；仅凭opcode不足以决定业务身份或解码类型。

2026-10-23公开网络研究已找到“21个7709命令”的来源：社区eltdx 3.0.0于2026-08-16发布的说明。其最新3.3.0安装包的命令清单及源码均为26项，包含心跳和握手；21是旧版本计数，不是通达信官方公布的9月新版协议数量。逐项来源及差异见[近期协议研究](../architecture/TDX_PROTOCOL_RESEARCH_20261023.md)。

按当前rustdx源码请求字节去重，仍为14个业务命令码和1个当前使用的握手命令。与社区最新26项按opcode比较，同码12项、社区另有14项本库未接入，本库另有0x0450/0x051D/0x0FB5三项未列入该清单。清单未列入不证明旧码失效；同码也不证明报文、单位及解码合同相同。下面的映射及字段字典继续描述本库当前实现，不把社区字段覆盖到现有类型。

### 13.2 已核实的命令映射

| 线上命令码 | 业务操作 | 当前请求/Client入口 | 对应字段章节 |
| --- | --- | --- | --- |
| `0x044E` | 证券数量，也用于心跳 | SecurityCount/Heartbeat；stock_count/heartbeat | u16数量，市场从请求保留 |
| `0x0450` | 分页证券目录 | stock::SecurityList；stocks；basic中另有旧同码类型 | 第8节SecurityListData |
| `0x053E` | 多证券行情快照 | QuotesRequest/旧SecurityQuotes；quotes | 第3节QuoteData与五档 |
| `0x052D` | K线，股票/指数共用 | KlineRequest/旧Kline/IndexKline；bars/index_bars及区间/批量入口 | 第4节股票/指数不同结果结构 |
| `0x051D` | 当日分时 | MinuteTime；minute首选路径 | 第5节MinuteTimeData |
| `0x0FB4` | 历史分时 | HistoryMinuteTime；history_minute及minute回退 | 第5节MinuteTimeData，日期从请求保留 |
| `0x0FC5` | 当日成交明细 | TransactionRequest/旧Transaction；transaction | 第5节TransactionData |
| `0x0FB5` | 历史成交明细 | HistoryTransactionRequest/旧HistoryTransaction；history_transaction | 同上，历史num由库补0 |
| `0x0010` | 财务快照 | FinanceRequest/旧FinanceInfo；finance | 第6节FinanceInfoData |
| `0x000F` | 除权除息/股本事件 | Xdxr；xdxr及k_adjusted所用事件 | 第7节XdxrData |
| `0x02CF` | F10栏目目录 | CompanyInfoCategory；f10_categories及f10第一步 | 第8节CompanyInfoCategoryItem |
| `0x02D0` | F10栏目内容 | CompanyInfoContent；f10逐栏目读取 | 第8节文本结果 |
| `0x02C5` | 板块文件大小/元信息 | BlockInfoMeta；block第一步 | 文件size字节数 |
| `0x06B9` | 板块文件分块内容 | BlockInfoChunk；block后续下载 | 原字节，再解析第8节BlockRecord |

当前连接先发送`PACK1`，命令码`0x000D`，用于会话初始化。旧`PACK2`也是`0x000D`但会话参数不同；旧`PACK3`为`0x0FDB`。后两者已弃用，当前正常连接不会继续发送，不能按活跃业务操作统计。

依据：[数量/心跳与握手字节](../../../crates/rustdx-core/src/tcp/basic.rs)、[证券目录字节](../../../crates/rustdx-core/src/tcp/stock/security_list.rs)、[快照](../../../crates/rustdx-core/src/tcp/stock/quotes.rs)、[K线](../../../crates/rustdx-core/src/tcp/stock/kline_request.rs)、[分时](../../../crates/rustdx-core/src/tcp/stock/minute_time.rs)、[历史分时](../../../crates/rustdx-core/src/tcp/stock/history_minute_time.rs)、[逐笔](../../../crates/rustdx-core/src/tcp/stock/transaction.rs)、[历史逐笔](../../../crates/rustdx-core/src/tcp/stock/history_transaction.rs)、[财务](../../../crates/rustdx-core/src/tcp/stock/finance_info.rs)、[除权](../../../crates/rustdx-core/src/tcp/stock/xdxr.rs)、[F10](../../../crates/rustdx-core/src/tcp/stock/company_info.rs)、[板块](../../../crates/rustdx-core/src/tcp/stock/block_info.rs)。

### 13.3 一对多与多对一实例

| 高层API | 实际命令关系 |
| --- | --- |
| `bars/k/bars_range/bars_batch/k_batch` | 都复用0x052D；差别在周期、范围、分页和并行，不是新增opcode |
| `index_bars/index_bars_range` | 同样使用0x052D，但由指数解码器读取额外上涨/下跌家数 |
| `stocks` | 0x044E取得数量，再重复0x0450直到目录完整 |
| `heartbeat/stock_count` | 两种API语义，共用0x044E |
| `minute` | 首先0x051D，空/适用解析异常时用当日日期请求0x0FB4回退 |
| `f10` | 0x02CF取得栏目，再按filename/start/length多次0x02D0 |
| `block` | 0x02C5取得size，再按字节段重复0x06B9，最后本地解析板块 |
| `k_adjusted` | 0x052D原价K线＋0x000F除权事件，再本地计算价格复权 |

新增KlineRequest、QuotesRequest、FinanceRequest、TransactionRequest、HistoryTransactionRequest复用相应旧命令码。它们强化参数不可变、身份/数量/日期/数值及尾部检查，不代表新增五个上游命令。

### 13.4 字段字典与原生报文的差异

字段字典不是线上全部字节的逐位置翻译。QuoteData会丢弃部分保留字段，解码五档相对价格，并计算change/change_percent；日线DateTime的15:00是库补出的时点。字段换算或新DTO不能被统计为协议命令。

交易所HTTP名单、官网HTTP ZIP不属于7709命令集。day/lc/gbbq解析、指标、日历/涨跌停、标准适配、runner账本、数据库核验和发布目录属于本地或下游能力，也不能加入该命令数量。

按外部7709清单进行逐项验收时，应固定其版本、命令码及会话参数口径，再分别记录：请求编码是否存在、响应解码是否存在、字段单位是否核实、样本/异常是否通过、生产入口是否接入。此次26项清单已固定到eltdx 3.3.0源码包；网络研究及当前映射表只确认资料/源码关系，没有补做所有命令的在线验收。官方10月23日tdxaidata 1.3.1及本地17709 HTTP属于另一接口层次，详见研究报告。
