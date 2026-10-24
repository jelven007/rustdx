# 通达信接口参考与 rustdx 实现情况

> 当前源码更新：eltdx 3.2.2的22个7709基线命令已全部有同码调用链，四项迁移和九项新增均完成。最新方法签名、字段、分页/批量、同步订阅与业务验收范围见[九命令实施记录](../development/NINE_COMMANDS_IMPLEMENTATION_20261024.md)。下文保留最初pytdxdata 0.6.9跨通道核对快照；22项STD覆盖不表示MAC/EX已接入。

版本：2026-10-24 / 1.0。参考 pytdxdata **0.6.9**、此前固定的 eltdx 3.3.0，以及 rustdx 当前工作区。本文为源码核对后的接口文档；本次没有修改 Rust 实现，也没有运行第三方 SDK 的实盘验收。

## 1. 版本、范围与实现结论

截至本次查询，pytdxdata 的 `main` 为提交 [`2cf3657f4b531827cc892f8656107514d636c921`][commit]，提交时间为 **2026-10-23 07:24:17 UTC**，源码版本为 0.6.9。PyPI 最新版本同为 0.6.9，上传于当天 07:27 UTC；GitHub Releases 的 latest 仍为 [v0.6.2][release]，不能只看 Release 页判断最新源码。[版本声明][version]、[PyPI](https://pypi.org/project/pytdxdata/)。

对 `TdxData` 的公开异步方法逐个提取后，实际为 **68 个：33 个当前方法、33 个弃用方法、2 个生命周期方法**。上游 `API_REFERENCE.md` 仍写“63 / 28”，文件开头另有“51 个旧接口”的旧描述；本文以固定提交的实际方法为准。完整签名、行号、源码摘要和本库对应关系见[核对证据 JSON](../../testing/evidence/pytdxdata-api-review-20261024.json)。

rustdx 核对对象为 `HEAD=408e30e` 的当前工作区，核心库 Cargo 版本 1.12.2，包含既有未提交改动。下表按能力判断，不要求 Rust 复制 Python 方法名，也不把同命令号等同于完整兼容。

| 统计口径 | 当前结果 | 应如何理解 |
| --- | --- | --- |
| pytdxdata 33 个当前方法 | 12 个有相关基础能力，21 个没有对应查询或完整封装 | 12 个仍有通道、参数、返回模型或语义差异，不代表 12 个完整兼容 |
| 原 eltdx 21 项命令 | 9 项同码、4 项替代命令、8 项没有对应主动 TCP 能力 | 与[前版逐项核对](../validation/TDX_7709_FUNCTIONAL_AUDIT_20261024.md)一致 |
| 协议通道 | rustdx 目前为沪深普通 STD TCP | 没有 pytdxdata 的 MAC、EX 通道及跨市场自动路由 |
| 已有主要业务 | 目录/数量、五档快照、K 线/区间/批量、本地日线复权、分时、单页逐笔、财务/股本、F10、板块文件 | 并非从零开始，但尚未覆盖最新版扩展能力 |
| 未接入的主要扩展 | MAC 竞价/特征快照/资金流向、字段报价、板块与指数成分及汇总、异动、行业/专业财报、EX 行情 | Python SDK 已有这些入口，本库没有相应完整能力 |

“有基础能力”包括已有本地计算；“未实现”表示没有本文所列能力的对应入口或调用链。库级查询、CLI/runner 采集、真实服务器验收分别判断。当前 runner 的采集源是冻结的本地 `.day` 目录，没有把下列 TCP 方法接成持续采集任务。

## 2. 调用方式与共同约定

### 2.1 pytdxdata 初始化与关闭

安装要求 Python ≥ 3.11。推荐异步上下文管理器，亦可显式 `await start()`、`await close()`。[构造及生命周期源码][p-init]。

```python
from pytdxdata import TdxData, KlinePeriod, Adjust

async def query():
    async with TdxData(default_adjust=Adjust.NONE) as td:
        quotes = await td.get_quotes(["sz000001", "sh600519"])
        bars = await td.get_bars("sz000001", KlinePeriod.DAY, count=800)
        ticks = await td.get_ticks("sz000001", date=20261023, count=None)
        return quotes, bars, ticks
```

示例按固定源码签名编写，本轮没有执行它或安装 SDK。网络返回、服务器保存范围及完整性仍需实际验证。

| 构造参数 | 源码默认值 | 含义 |
| --- | --- | --- |
| `standard_servers / mac_servers / ex_servers` | `None` | 各通道服务器列表，缺省取内置节点 |
| `port` | `7709` | STD/MAC 端口；EX 池默认使用 7727 |
| `timeout` | `3.0` 秒 | 连接/请求超时参数 |
| `cache_dir` | `None` | 默认只有内存缓存；指定后加 SQLite 磁盘缓存 |
| `default_adjust` | `Adjust.NONE` | K 线默认复权方式 |
| `max_inflight` | `64` | 缓存层同时进行的获取预算 |
| `pool_min / pool_max` | `3 / 24` | 动态池参数；具体通道还受自身上限和节点数影响 |
| `mac_pool_cap / std_pool_cap / ex_pool_cap` | `4 / 4 / 2` | 对应通道的每服务器连接预算 |

上游构造表中的 `pool_max=12` 已落后于源码的 24；`std_pool_cap`、`ex_pool_cap` 也应纳入文档。MAC 池最大值取 `max(pool_max, MAC 节点数)`；EX 池最大值取 `min(pool_max, 6)`。并发参数不是一定能获得的实际连接数。

rustdx 使用同步 `Client::new()` / `with_config()`，建连与重连自动握手；支持 `heartbeat()`、`reconnect()`、`retry()`。连接池在借出空闲连接时探活，没有对持有连接的后台周期心跳。保持现有同步 I/O、有界线程设计即可，不因 Python SDK 使用 asyncio 而要求引入异步运行时。[本库 Client][r-client]、[网络与连接池](../../architecture/NETWORK.md)。

核对期间工作区另有当日 IP 池实现变动：`TcpConfig::ip_pool` 可选择独立池，默认取进程共享池；业务建连、指定地址、重连和复用均检查当日有效节点，首次可能等待补测。`IpPool` 有初始列表管理、快照和显式刷新，后台按北京时间09:00刷新，源码要求三轮、最多八并发、TCP/握手/心跳各阶段3秒，三轮通过才入池。此处按[当前 IP 池源码](../../../crates/rustdx-core/src/tcp/ip/daily.rs)与[探测源码](../../../crates/rustdx-core/src/tcp/ip/probe.rs)补充连接行为，不计作本轮接口实现或测试通过；上述业务命令覆盖不变。

### 2.2 代码、市场与返回组织

pytdxdata 接受 `sz000001`、`sh600519`、`bj920992`、`hk00700`、`usAAPL`、`cffex:IFL0` 等规范代码。**裸 6 位数字在其通用解析器中按沪市指数解释**，不是自动猜股票市场；例如深市股票应明确写 `sz000001`。指数辅助方法另有指数代码映射规则。[代码解析][p-symbols]。

市场选择器包括 `sz/sh/bj`、`hk/us/cffex/zz/dl/shf/gz` 及扩展市场完整枚举名，`None/"all"` 表示聚合。`market=0/1/2` 是 STD 的深/沪/北编号；EX 市场有独立编号，不可混用。

当前多数 pytdxdata 方法即使传入单只，也返回平铺 `list`，每条记录带 `symbol`；多只通常按输入标的顺序拼接，标的内时间序列升序。少数方法返回单个 dict、tuple 或模型，详见第 3 节。没有财务数据的证券会被跳过，返回条数不保证等于输入数。

rustdx 高层大多显式传 `(market, code)`，普通 TCP 校验仅允许 0/1。`Symbol` 可以表示北交所，并不表示 TCP 已支持 market=2。批量 K 线返回每只对应的 `result`，与 Python 平铺列表的失败/分组语义不同。

### 2.3 通道与分页约束

| 业务 | pytdxdata 0.6.9 实际路径 | 分页或分片 | rustdx 当前情况 |
| --- | --- | --- | --- |
| 沪深证券目录 | STD `0x044e` 数量 + `0x0450` 列表 | 1000 条/页，29 字节记录 | 同旧协议；已自动聚合，非 eltdx `0x044d` 的 1600 条页 |
| A 股默认五档报价 | STD `0x053e` | 80 只/请求，自动分片 | 同命令，单次最多 80；无高层自动分片/流水线 |
| 字段报价/成分报价 | MAC/EX `0x122b`、MAC `0x122c` | 80 只/页 | 未实现 |
| 普通不复权 K 线 | STD `0x052d` | 800 根/页 | 已校验 1–800 |
| A 股 1 分钟或复权 K 线 | MAC `0x122e` | 700 根/页，同证券分页固定连接串行 | 未接入 MAC；本库复权为本地日线计算 |
| 扩展市场 K 线 | EX `0x23ff` | 700 根/页 | 未实现 |
| A 股逐笔 | MAC `0x122f` | 1000 条/页，最多 50 页，同证券固定连接串行 | 当前为 STD `0x0fc5/0x0fb5` 单页 |
| 港股逐笔 | EX `0x23fc/0x2406` | 1800 条/页，最多 50 页 | 未实现 |
| Python 保留的旧 STD 逐笔解析器 | `0x0fc5/0x0fb5` | 源码页常量 800，非当前 `get_ticks` 主路径 | 本库构造器允许到 2000；服务端旧协议极限待核实 |
| 文件 | STD 或 MAC 按名称选路 | 默认 30000 字节/块，短/空块结束 | 板块高层已分块并校验完整性；无通用文件入口 |

这些数值来自 SDK 的常量及调用路径；不能全部当作独立测定的服务端硬上限。原 eltdx `0x0fc5/0x0fc6` 的 1800 条约束仍适用于其接口说明，不能推广到 MAC `0x122f` 或用它证明本库旧协议 2000 条已正确。

K 线的 `count` 是高层总需求量，超过单页会分页。STD 的 `start=0` 表示从最新端查询；MAC/EX 的偏移方向另有探测和反转处理，不能直接套用 docstring 的“从最早 K 线开始”。本库区间分页每页 800、u16 偏移、最多 82 页，短页即结束，再升序/去重/过滤窗口。两个 SDK 都不保证“参数很大就一定取到全部历史”。

pytdxdata 分页器虽然声明 `min_page=100`，已知 total 的规划分支会使用精确 count、不强制最小页；不能根据注释宣称所有 `count<100` 风险均已规避。[Python 分页器][p-pagination]、[本库分页器][r-pagination]。

## 3. 最新 33 个当前方法

### 3.1 总览及实现映射

编号 P01–P33 仅用于本文及证据 JSON，不是协议命令编号。下表“部分”包括有相关业务但没有相同封装；不能据此承诺 Python/Rust 结果完全一致。

其中依赖通达信上游数据的31项及其直接查询、文件解析、派生调用链，见[上游依赖接口清单](TDX_UPSTREAM_DEPENDENT_APIS_20261024.md)。

| # | pytdxdata 当前方法 | 主要通道/来源 | rustdx 实际入口 | 当前判断 |
| --- | --- | --- | --- | --- |
| P01 | `get_universe` | STD 目录 + EX 清单 | `stocks / stock_count` | 部分，仅沪深 STD |
| P02 | `sync_decimal_points` | 目录驱动运行时精度表 | 目录有 `decimal_point` | 未实现报价精度同步 |
| P03 | `decimal_point_status` | 运行时状态 | 无 | 未实现 |
| P04 | `get_quotes` | STD 五档 / MAC·EX 字段报价 | `quotes` | 部分，仅旧五档 |
| P05 | `get_snapshot` | MAC `0x122a` 特征快照 | 无 | 未实现 |
| P06 | `get_auction` | MAC `0x123d` | 无 | 未实现 |
| P07 | `get_capital_flow` | MAC `0x1218`，head=2 | 无 | 未实现 |
| P08 | `get_bars` | STD / MAC / EX K 线 | `bars / index_bars / *_range / *_batch / k_adjusted` | 部分 |
| P09 | `get_indicators` | K 线 + 本地指标 | `indicators` 模块 | 部分，计算已有，组合查询未封装 |
| P10 | `get_ticks` | MAC / EX 逐笔 | `transaction / history_transaction` | 部分，旧单页、无分类/自动全量 |
| P11 | `get_minutes` | MAC·EX 分时 / 派生采样 | `minute / history_minute` | 部分，旧 DTO |
| P12 | `get_xdxr` | STD `0x000f` | `xdxr` | 部分，单只 |
| P13 | `get_finance` | STD `0x0010` | `finance` | 部分，单只 |
| P14 | `get_price_limits` | 本地规则 | `limit` 日期化规则及价格计算 | 有本地基础能力，签名/规则口径不同 |
| P15 | `get_f10` | STD `0x02cf/0x02d0` | `f10 / f10_categories / CompanyInfoContent` | 有基础能力，调用组织不同 |
| P16 | `get_market_stat` | 3 个统计代码快照派生 | 无专门入口 | 未实现统计封装 |
| P17 | `get_unusual` | MAC `0x1237` | 无 | 未实现 |
| P18 | `get_server_info` | MAC `0x120f` | 无 | 未实现 |
| P19 | `get_kline_offset` | MAC `0x124a` | 无 | 未实现 |
| P20 | `get_boards` | MAC `0x1231` | 无实时板块列表 | 未实现 |
| P21 | `get_board_members` | MAC `0x122c` | 无实时成分报价 | 未实现 |
| P22 | `get_board_of` | MAC `0x1218`，head=1 | 无 | 未实现 |
| P23 | `get_board_summary` | 成分报价本地汇总 | 无 | 未实现 |
| P24 | `get_board_ranking` | 板块报价本地排序 | 无 | 未实现 |
| P25 | `get_board_change_ranking` | 板块 K 线派生排序 | 无 | 未实现 |
| P26 | `get_index_members` | MAC `0x122c` 指数→板块映射 | 无 | 未实现 |
| P27 | `get_index_trade_stats` | 成分逐笔分钟汇总 | 无 | 未实现 |
| P28 | `get_index_trade_stats_multi` | 多日复用成分汇总 | 无 | 未实现 |
| P29 | `get_file` | STD `0x02c5/0x06b9` 或 MAC 文件 | `BlockInfoMeta / BlockInfoChunk / block` | 部分，只有板块完整高层路径 |
| P30 | `get_block_parsed` | 文件 + 板块解析 | `block / parse_block_content` | 有基础能力，DTO 不同 |
| P31 | `get_industry_map` | `tdxhy.cfg` 解析 | 无 | 未实现 |
| P32 | `get_financial_file_infos` | `tdxfin/gpcw.txt` 解析 | 无 | 未实现 |
| P33 | `get_financial_records_parsed` | 专业财报 zip/dat 解析 | 无 | 未实现 |

### 3.2 标的清单与精度

**P01 — `get_universe(market: str | None = None) → list[SecurityInfo]`**。[源码][p01]

一次获取指定市场或全部市场的清单；返回 `market/code/name/volunit/decimal_point/pre_close/symbol/kind`。EX 条目没有 STD 昨收/单位，对应值为 0。STD 目录为所有证券，含基金、债券、指数等，不能将 `len()` 当作 A 股数。

北交所 `get_universe("bj")` 在 0.6.9 **直接返回空列表并告警，不联网枚举**；`all` 跳过该目录。作者报告北交所个券报价/K 线/逐笔可用，本轮未独立实测。EX 原始清单可能含非证券条目和重复 `(market, code)`，SDK 没有保证全量清单纯净、唯一。rustdx 的 `stocks(0/1)` 自动汇总旧目录，并校验数量、重复和提前空页，尚无跨市场统一模型及全 A 股便捷封装。

**P02 — `sync_decimal_points(markets: Sequence[str] = ("sh", "sz")) → dict`**。[源码][p02]

用目录 `decimal_point` 同步报价解码运行时精度表，返回 `prefixes/codes/mixed_prefixes/stock` 等统计。`start()` 已启动自动同步：有当日缓存则装表，否则后台同步；后台失败会回退静态前缀规则。手动调用仍经过目录缓存，不能理解为无条件刷新网络数据。rustdx 已保存目录精度，但报价解码仍固定 `/100`，尚未把目录值接入。

**P03 — `decimal_point_status() → dict`**。[源码][p03]

查看精度表来源、证券数量、同步状态等，供排障；无业务参数。rustdx 没有对应状态查询或精度同步生命周期。

### 3.3 报价、特征快照、竞价与资金

**P04 — `get_quotes(symbols, *, fields=None, concurrency=None, pipeline=True) → list[SecurityQuote]`**。[源码][p04]

`symbols` 为单只或列表；缺省 A 股用 STD `0x053e`，带五档 `bid/ask`、主动买卖量、状态等，但没有名称。传 `fields` 改用 MAC/EX `0x122b`，返回名称及字段字典，五档为空；扩展市场也走字段报价。字段位例如 `0x00` 昨收、`0x04` 现价、`0x07` 金额、`0x38` 主力净流入；字典键使用 `"0x7"` 等 `hex()` 形式。

自动按最多 80 只分片，支持同连接每组 3 片流水线及失败降级。`concurrency` 受池预算约束；作者的毫秒耗时属于其样本，本文不作为性能承诺。rustdx `quotes(&[(0, "000001"), ...])` 返回五档 `QuoteData`，没有字段选择、跨市场、自动分片、流水线、交易状态字段及目录精度驱动。

**P05 — `get_snapshot(symbols) → list[SymbolSnapshot]`**。[源码][p05]

仅 A 股，MAC `0x122a`。返回名称、日期/时间、活跃度、OHLC、量额、换手和均价；列表输入逐只查询，走快照缓存。这是**特征快照**，不能将名称相似理解为 eltdx `0x054c` 一档基础快照。rustdx 未实现。

**P06 — `get_auction(symbols) → list[AuctionItem]`**。[源码][p06]、[报文及解码][p-mac-auction]

仅 A 股，返回 `symbol/time/price/matched/unmatched`。MAC `0x123d` 底层有 `start=0,count=100`，高层固定用默认值，**没有 date、start、count 或自动翻页参数**。未匹配量按有符号整数保留，模型没有单独 direction 字段。不承诺可用它下载任意历史日或完整竞价过程。rustdx 未实现；它也不是 eltdx `0x056a`。

**P07 — `get_capital_flow(symbols) → list[CapitalFlow]`**。[源码][p07]

仅 A 股，MAC `0x1218` 的 head=2 变体，逐只查询。返回 `main_buy/main_sell/retail_buy/retail_sell/buy5/sell5/big5/mid5` 及身份；无数据会跳过。字段的“资金流”是 SDK 解析结果，不应未经业务核验当作交易所逐单资金定义。rustdx 未实现。

### 3.4 K 线、指标、逐笔和分时

**P08 — `get_bars(symbols, period=KlinePeriod.DAY, *, start=0, count=800, adjust=None, start_date=None, end_date=None, concurrency=6) → list[SecurityBar]`**。[源码][p08]

支持多标的、自动指数识别、通道路由、跨页汇总及时间升序。`adjust=None` 使用实例 `default_adjust`；`Adjust.NONE/QFQ/HFQ` 为不复权/前复权/后复权，A 股 QFQ/HFQ 路由 MAC，1 分钟也走 MAC。EX `0x23ff` 请求没有编码 adjust，当前仅将它加入缓存键，不能承诺扩展市场已复权。

日期参数注解为 `datetime`，实现也归一化 `date` 输入；日期分支优先处理，不与 start/count 叠加。最终按 `datetime >= start_dt`、`datetime <= end_dt` 取闭区间；end_date 的时刻为00:00时扩为该日23:59:59.999999。单给 start_date 取不早于该时刻的记录。EX 日期定位探针与普通 EX K 线拉取使用不同请求，组合兼容仍需验证。

返回 OHLC、量额、日期/时分和 `symbol`；美股有北京时间与交易日处理。指数 `IndexBar` 实际没有 `up_count/down_count` 字段，STD 解码跳过 4 字节，不能根据描述承诺涨跌家数。EX 适配路径的 `amount` 当前填 0，不表示该日无成交额。尚无定点复权、锚点日期或 eltdx `bar_time` 开关。

rustdx 已有股票/指数单页、区间、股票批量区间和本地日线 Qfq/Hfq；指数 DTO 反而保留涨跌家数。没有 MAC/EX、跨市场路由、所有周期服务端复权或定点复权。本库旧 STD 请求探测“不支持服务器复权”的结论只适用于该旧请求，不能推广为 MAC 服务端也不支持。

**P09 — `get_indicators(symbols, indicators, *, period=DAY, count=30, adjust=None, params=None) → list[IndicatorSet]`**。[源码][p09]

组合查询 K 线并本地计算 MACD/KDJ/RSI/BOLL/MA/EMA；每只一个结果，包含 `symbol/bars/indicators`。指标序列与 bars 等长，部分预热位置为 `None`；`params` 为指标参数字典。rustdx `indicators` 已有相应计算函数，但没有联网+多标的+指标的一体化高层方法；算法初始化与预热行为仍需单独比对。

**P10 — `get_ticks(symbols, *, date=None, start=0, count=None, concurrency=6) → list[TransactionRecord]`**。[源码][p10]

`date=None` 为当日，历史传 YYYYMMDD 整数；`start` 从最新记录往回计数，`count=None` 尽量取全，仍有 50 页封顶。单只 A 股所有页固定在同一 MAC 连接上串行，页偏移按原始返回条数推进，避免不同主站的竞价记录差异造成漏条/重叠。多只之间可并发。

返回 `time/price/vol/trade_count/bs_flag/trade_count_available/symbol/fields`。A 股为带秒时间的约 3 秒聚合切片，**不是交易所原始逐笔**；港股为 HH:MM、增仓等字段。分页结束后过滤 `bs_flag=8`，排序后写入缓存；给定 `count` 是筛选前需求，结果可能少于 count。没有 eltdx `TradePage.actual_trades/opening_matches/auction_snapshots` 分类模型；非 8 的 09:25 正式撮合仍在正常结果中，见第 5 节。

rustdx 只有 STD 当日 `0x0fc5`、旧历史 `0x0fb5` 单页方法，保留 `buyorsell` 数值；无批量、全量分页、事件分类和 `trade_count_available`。旧历史 `num=0` 是无字段的补值，不能理解为真实“0 笔”。

**P11 — `get_minutes(symbols, *, date=None, sampling=False, concurrency=6) → list[MinuteBar]`**。[源码][p11]

当日或 YYYYMMDD 历史分时，A 股用 MAC `0x122d`，扩展市场使用同构请求。返回价格、量、均价、时分、日期、身份和扩展字段。`date=None` 时高层使用本机当天日期填充结果；休市时主站可能保存最近交易日数据，不能只凭返回 date 认定数据确属当日交易。

`sampling=True` 返回只有价格与 `fields["i"]` 点位序号的采样点，量/均价为 0、没有真实时间；A 股由 `0x122d` 派生，EX 用 `0x254d`，EX 采样请求没有历史日期参数。它不是 `0x0fd1` 的 selector/window/base_price 接口。

rustdx 当日用 `0x051d`，必要时回退当日 `0x0fb4`；历史用 `0x0fb4`。当前 DTO 只有 price/vol，缺时间、均价、批量和采样入口。历史分时 u32 日期未校验真实性；调用者应先校验。

### 3.5 公司信息与本地限制

**P12 — `get_xdxr(symbols) → list[XdxrRecord]`**。[源码][p12]、[解析器][p-xdxr]

仅 A 股，STD `0x000f`；列表输入仍逐只串行请求，不是 eltdx 单报文批量 75/200 的协议。字段含日期、类别、名称、分红/配股/送转、扩缩股、权证和前后股本。源码名称表为 1–14，类别 15 没有专用语义分支。

pytdxdata 类别 1 的分红、送转、配股比例已从原始值 `/10` 变为每股口径，配股价不变；rustdx 本地复权公式使用原始“每 10 股”数值。迁移时不能将 Python 已归一化值直接代入本库旧公式。其他类别按各自字段验证。rustdx 单只查询已有，但无列表封装及增强类别语义。

**P13 — `get_finance(symbols) → list[FinanceRecord]`**。[源码][p13]、[解析器][p-finance]

仅 A 股，STD `0x0010`；列表逐只串行，无数据跳过。返回股本、地区/行业、更新/上市日期、资产/负债/收入/利润、现金流、每股净资产等旧财务字段。

**以解码器为准：股本和总量金额已乘 10000，实际是股/元尺度。** 模型注释“万股/万元”未同步；rustdx 对对应字段也乘 10000，不应在映射时再整体放大。每股净资产、股东数等不乘相同系数。两库都仍有旧 `zhigong_gu/zhigonggu` 字段，没有独立已核实 EPS；不能直接照搬 eltdx 的 EPS 字段解释。rustdx 返回单个 DTO，无数据为 `NotFound`，与 Python 跳过条目不同。

**P14 — `get_price_limits(symbol, name=None, pre_close=None, listed_days=9999) → (upper, lower)`**。[源码][p14]

本地计算，无网络；输入名称用于 ST 判断、昨收用于价格计算、上市天数用于上市阶段。指数/基金返回 `(None, None)`。它不是 `0x0452` 服务端特殊品种表。rustdx 已有普通价格函数、板块/ST 判断和日期化规则；签名、日期证据及上市阶段口径不同，应按本库[日期规则](../../requirements/LIMIT_RULES.md)使用。

**P15 — `get_f10(symbol, section=None, *, offset=0, length=65535) → list | str`**。[源码][p15]

`section=None` 取目录，条目含 `name/filename/start/length`；给 `section` 时实际作为文件名传入正文请求，并按 offset/length 读 GBK 文本。报文命令为 STD `0x02cf/0x02d0`，源码注释的 `0x109b/0x109c` 是头内其他字段，不能当作业务 opcode。

rustdx `f10_categories` 取目录，`f10` 自动读取全部栏目，低层 `CompanyInfoContent::new` 可按目录的 filename/start/length 读取。已有基础能力，但没有同签名的可选栏目高层接口。[本库 F10][r-f10]。

### 3.6 市场、服务器与板块

**P16 — `get_market_stat() → MarketStat`**。[源码][p16]

查询 `sh880005/sh880001/sh880006` 旧快照，本地解释为涨/跌/平家数、总量额、市值和涨跌停家数。不是新命令；某些代码缺失时模型可能保持 0，不应把零当作完整统计的证明。rustdx 可查旧快照，没有相应统计解释封装。

**P17 — `get_unusual(market="all", *, start=0, count=600) → list[UnusualItem]`**。[源码][p17]

MAC `0x1237`，返回证券/名称、异动 type/desc/value/time。选择器经 `market_targets` 后只保留 CN 通道；没有高层全量翻页循环。docstring 将 all 简写为深+沪，实际 CN 市场集合还包含 BJ，是否有返回需实测。rustdx 未实现。

**P18 — `get_server_info() → ServerInfo`**。[源码][p18]

MAC `0x120f`，返回 `date/last_trade_date/sessions`，sessions 为时间段 tuple 列表。可用于认识服务器当前交易日，但不能据此覆盖交易所节假日及证券状态完整规则。rustdx 未实现。

**P19 — `get_kline_offset(offset=0, count=1) → tuple[int, int]`**。[源码][p19]

MAC `0x124a`，返回总根数与本次返回数。高层没有 symbol 参数，不要当作“查询某只证券日线总数”的通用方法。rustdx 未实现。

**P20 — `get_boards(kind="all", *, count=10000) → list[BoardInfo]`**。[源码][p20]

kind 支持 `all/industry/concept/style` 或数字类型，MAC `0x1231`。每条包括板块价/昨收/涨速，以及 `symbol_*` 领涨股信息。当前分页请求 150，偏移沿用 `len(batch)//2`，源码明确写“实盘未校验”；count 是循环预算，结果未严格截到 count。需要验重复和完整性，不能据默认 10000 声称全量验收。rustdx 板块文件解析不是实时板块列表，未实现该接口。

**P21 — `get_board_members(board, count=100000, sort_type=0, *, fields=None) → list[SecurityQuote]`**。[源码][p21]

board 为板块码，如 `"881001"`；先转换为服务端码，再用 MAC `0x122c`、80/页取成分报价。默认字段为基础价量；**传 fields 时替换默认字段集合**，要同时取得基础字段及净流入，应传基础位 + `[0x38]`。`sort_type` 为底层排序参数，不等同于 eltdx 的中文 sort_by/ascending API。count 为循环上限，末页可能超出该数且不截断。rustdx 未实现。

**P22 — `get_board_of(symbols) → list[BelongBoard]`**。[源码][p22]

MAC `0x1218`、head=1，逐只查询所属板块；返回 board_code/board_name/close/pre_close，`symbol` 指被查询个股。rustdx 的 `limit::board_of` 是上市板块规则分类，不是行业/概念所属板块查询。

**P23 — `get_board_summary(board) → dict`**。[源码][p23]

成分报价附加 `0x38` 后本地汇总，返回 `member_count/amount/main_net_amount/up_count/down_count`。结果完整性依赖成分覆盖和字段可用性，不是新的服务器汇总命令。rustdx 未实现。

**P24 — `get_board_ranking(kind="industry", *, top_n=20) → list[dict]`**。[源码][p24]

用板块现价/昨收计算当日涨幅，本地排序；底层取板块列表时 count=200。不是 eltdx `0x054b` 全市场服务端排行榜，也不保证覆盖所有板块。rustdx 未实现。

**P25 — `get_board_change_ranking(kind="industry", *, days=20, top_n=20) → list[dict]`**。[源码][p25]

先取板块清单，再对前 `top_n*3` 个候选查询不复权日线 `days+5`，以首末收盘计算 change_pct 后排序；失败候选被跳过。**不是对全部板块精确取 N 个交易日窗口的全量排名**。返回 code/name/close_start/close_end/change_pct。rustdx 未实现。

### 3.7 指数成分与成交汇总

**P26 — `get_index_members(index, *, count=100000, fields=None) → list[SecurityQuote]`**。[源码][p26]

指数代码映射到服务端板块码后取 MAC `0x122c` 成分报价，例如 `000300→31300`。支持裸指数代码或带市场前缀；返回被截到 count。fields 与 P21 一样会替换默认位。**取的是当前成分，不是指定历史日成分**。

作者报告综合指数名单可能漏近期新股，样本指数在其样本中较完整；这不是本次独立验证结果。历史回测必须有历史成分证据，综合指数汇总必须核对全市场覆盖。rustdx 未实现。

**P27 — `get_index_trade_stats(index, date=None, *, concurrency=24, batch_size=50, members=None, skip_errors=True, on_progress=None) → IndexTradeStats`**。[源码][p27]

查询或使用调用者传入的 members，分批并发获取成分逐笔，按分钟聚合 volume/trade_count/amount。date 为当日或 YYYYMMDD；batch_size 用于限制内存峰值；进度回调为 `(已处理, 总数)`。skip_errors 默认忽略单只失败，并记录 `failed_members`，因此成功返回模型不等于全部成分成功。

返回 member_count/active_members/trade_count_members/minutes、总量/总笔数/估算总额、failed_members/elapsed。minutes 是**分钟增量**，用 `cumulative()` 或 `cum_trade_count()` 获取累计。成交额按股票 `price × 手 × 100` 估算；自定义 members 若含不同量纲品种，应先过滤或单独换算。

指数本身的逐笔字段不提供真实笔数，不能直接相加；北交所成分笔数也被判不可用。`trade_count_available` 只表示至少有成分能提供笔数，不证明每个成分完整。深市部分指数快照 amount 是板块口径，不能和样本成分总额直接比。rustdx 未实现这条完整链路。

**P28 — `get_index_trade_stats_multi(index, dates, *, concurrency=24, batch_size=50, skip_errors=True, on_progress=None) → list[IndexTradeStats]`**。[源码][p28]

dates 为 `Sequence[int | None]`；成分只取一次，多日串行复用后按 dates 顺序返回 `IndexTradeStats` 列表，每项含 date。不会自动生成交易日或重建各日历史成分。仍需检查每一天的 failed_members、历史保存范围与笔数可用性。rustdx 未实现。

### 3.8 文件、行业与专业财报

**P29 — `get_file(name) → bytes`**。[源码][p29]

源码按名称前缀选路：`block_*` 用 STD 元信息 `0x02c5` + 数据 `0x06b9`；`tdx*` 用 STD `0x06b9`；其余用 MAC `0x1215` 元信息 + `0x1217` 数据。默认每块 30000，短块或空块结束。高层没有 offset/size 参数，也没有通用 300 字节路径保证。

STD 元信息名 40 字节，STD 文件数据名 100 字节，MAC 名 70 字节；编码存在截断策略，不应照搬 eltdx path≤300 的格式。源码注释的 `0x1869/0x186a` 是其他头字段，实际业务 opcode 为 `0x02c5/0x06b9`。Python `_download` 即使已知 size 也可遇短块退出，没有本库板块高层同等的完整长度校验。rustdx 底层块请求已有，但高层只完整封装板块下载，没有通用资源读取和统计文件解析。[本库文件][r-file]。

**P30 — `get_block_parsed(filename) → list[TdxBlock]`**。[源码][p30]

下载并解析 `block_zs.dat/block_gn.dat/block_fg.dat` 等；返回 name/category/count/codes，category 由文件名推断。声明 count 可能大于实际解析 codes 数。rustdx `block` 已完成同类下载和解析，返回扁平 `BlockRecord`；层次和类别来源不同，不能只做字段同名映射。

**P31 — `get_industry_map() → dict[str, IndustryInfo]`**。[源码][p31]

下载 `tdxhy.cfg`，返回以 6 位代码为键的通达信/申万行业分类。键不带市场前缀，跨市场连接时要明确代码冲突策略。rustdx 没有该文件解析与高层查询。

**P32 — `get_financial_file_infos() → list[FinancialFileInfo]`**。[源码][p32]

下载 `tdxfin/gpcw.txt`，解析文件名、MD5 hash、filesize。它是专业财报文件索引，不是 P13 最新财务快照。取得索引不等于 SDK 已验证下载内容的 MD5。rustdx 未实现。

**P33 — `get_financial_records_parsed(filename) → list[FinancialRecord]`**。[源码][p33]

支持索引里的 `gpcwYYYYMMDD.zip` 名称，内部尝试服务器实际 `tdxfin/gpcwYYYYMMDD.dat` 等候选，兼容 zip/dat，再解析 code/market/report_date/fields。报告期优先从文件名日期取，缺失时用 dat 头；无内容返回空列表。

fields 是报告原始 float 序列，字段数量/含义随 report_size 变化，不是已命名的统一财务字典。解析文件成功也不等于所有字段业务语义已核实。rustdx 尚无专业财报下载、候选纠正、解压和记录解析链路。

## 4. 原 21 项命令的更新对照

原清单属于 eltdx 7709 命令映射，不能当作 pytdxdata 的统一协议目录。pytdxdata 的类似业务可能走旧 STD、MAC 或 EX；两份接口应并列保留。

| # | 原命令 / 原能力 | pytdxdata 0.6.9 对应情况 | rustdx 当前实现情况 |
| --- | --- | --- | --- |
| 1 | `0x0004` 心跳 | 池内部保活，无同名公开 session 方法 | `heartbeat()`，发送 `0x044e`；无后台周期心跳 |
| 2 | `0x000d` 握手 | 各通道池内部各自初始化 | STD 建连/重连自动 `0x000d` |
| 3 | `0x000f` 股本变迁 | P12 同命令、列表逐只；非批量报文 | 同命令单只，无增强批量 |
| 4 | `0x0010` 财务批量 | P13 同命令、列表逐只；旧财务字段 | 同命令单只，无批量/EPS 新语义 |
| 5 | `0x044d` 新目录 | P01 实际旧 `0x0450`，1000/页 | 旧 `0x0450` 全量沪深 |
| 6 | `0x044e` 数量 | 内部同命令；新公开方法以 universe 为中心，旧 count 保留 | 同命令沪深证券总数，非 A 股数 |
| 7 | `0x0452` 特殊涨跌停表 | 没有该命令；P14 是本地计算 | 无特殊表；本地规则已有 |
| 8 | `0x051b` 分时副图 | 无同等副图接口 | 未实现 |
| 9 | `0x052b` 退市日线 | 无退市专用接口 | 未实现 |
| 10 | `0x052d` K 线 | P08：STD 同码，复权/1m 用 MAC，EX 另码 | STD 同码；区间/股票批量、本地日线复权已有 |
| 11 | `0x0537` 当日分时 | P11 用 MAC `0x122d` | 旧 `0x051d`，必要时回退 `0x0fb4` |
| 12 | `0x053e` 旧五档快照 | P04 A 股默认路径同码，80 自动分片 | 同码，最多 80 单次，字段/精度部分缺口 |
| 13 | `0x0547` 增量/推送 | 没有游标 refresh/poll_push 接口；重复报价为快照查询 | 未实现，且无推送队列 |
| 14 | `0x054b` 分类行情排行 | P21/P24 有成分报价/派生板块排序，但不是同命令或全市场排名 | 未实现 |
| 15 | `0x054c` 一档基础快照 | 无同码；P05 `0x122a` 为另一种特征快照 | 未实现 |
| 16 | `0x056a` 当日/历史竞价过程 | P06 MAC `0x123d`，高层无日期/全量分页 | 未实现 |
| 17 | `0x06b9` 文件读取 | P29 STD 同码及 MAC 文件，按名称选路 | 同码底层和板块高层，缺通用资源/统计 |
| 18 | `0x0fb4` 指定历史分时 | 高层 P11 用 MAC `0x122d`；另保留旧解析器 | 同码单只，缺时间/均价/批量/日期校验 |
| 19 | `0x0fc5` 当日逐笔 | 当前 P10 用 MAC `0x122f`；旧 STD 解析器仍存在 | 同码单页，无全量及事件分类 |
| 20 | `0x0fc6` 增强历史逐笔 | 当前 P10 用 MAC `0x122f`；旧历史为 `0x0fb5` | 旧 `0x0fb5`，非增强布局 |
| 21 | `0x0fd1` 小走势图 | P11 sampling 为 MAC 分时派生 / EX `0x254d`，参数和模型不同 | 未实现 |

因此，以原 21 项统计仍是 **9 同码 + 4 替代 + 8 无对应主动 TCP 能力**，没有因参考新 SDK 而增加本库实现数。详细参数探测、170 个既有测试及其范围见[原核对报告](../validation/TDX_7709_FUNCTIONAL_AUDIT_20261024.md)。

## 5. 业务语义与迁移约束

### 5.1 竞价快照和 09:25 正式撮合

按用户确认的 eltdx 事件分类，判断顺序如下：

```text
status_raw == 8
    → AuctionSnapshot（竞价快照，即使时间是 09:25）
status_raw != 8 且 time_minutes == 9*60+25
    → OpeningMatch（09:25 正式撮合）
其他
    → Trade（再按实际状态判断盘后等细分）
```

`0x056a` / MAC `0x123d` 是竞价过程查询，不应仅凭 09:25 时间将过程记录视作正式成交。正式撮合从成交明细中取**非 8 且 09:25**的记录。[eltdx 分类依据](../validation/TDX_7709_FUNCTIONAL_AUDIT_20261024.md#41-竞价快照与正式撮合)。

pytdxdata `get_ticks()` 对应 `bs_flag`，先用原始条数完成分页，再排除 8；不能在解码阶段先过滤而用过滤后的短页判断终点。它保留正式撮合，但没有 `opening_matches` 专门字段。想研究状态 8 的过程，应另用竞价入口并核对范围，而不是期望 get_ticks 返回被剔除的记录。

rustdx 只保留 `buyorsell` 原值，未知值 8 不会被拒绝，尚未实现上述分类。`TransactionData` 的状态/方向混在旧字段内，应保留原始信息后再分类；MAC 的 1=买、0=卖，与本库旧注释 0=买、1=卖不能不经验证统一翻译。盘后状态也不能仅凭时间猜测。

### 5.2 笔数、量与金额

| 数据对象 | 源码或既有证据说明 | 使用限制 |
| --- | --- | --- |
| pytdxdata MAC 个股逐笔 | vol 为切片量，trade_count 为切片笔数；股票常用手 | 先检查 `trade_count_available`，不是累计值 |
| 指数 MAC 逐笔 | 作者样本将 vol 解释为百元金额、原笔数槽为均价×1000；SDK 笔数置 0/不可用 | 不能按股票手数或真实笔数聚合 |
| ETF/多数 LOF/BJ 逐笔 | `has_order_count` 判不可用，笔数置 0 | 0 是无字段，不是实际无成交 |
| 港股 EX 逐笔 | 没有笔数，额外字段可含增仓 | 与 A 股模型同形，不代表字段都可用 |
| 股票/可转债量 | Python 作者样本：股票逐笔手、日线股；可转债逐笔及日线手，每手10张 | 这些为作者验证范围；不能套到所有通道/周期 |
| rustdx K 线量 | 本库真实样本：股票/ETF日线 `9/1` 为手，日线 `4/1` 和5分钟 `0/1` 为股/份 | 用显式 VolumeUnit；不能对所有 K 线统一×100 |
| FinanceRecord | Python/Rust 总量股本与金额均在解码时×10000 | 不再按模型“万股/万元”注释重复换算 |
| Xdxr 类别1 | Python 部分比例已/10为每股；Rust旧值为每10股 | 复权公式要先统一单位 |

pytdxdata `SecurityBar/MinuteBar` 的泛化注释不是所有品种/通道的实测规则；例如分时“手”与“同日线同口径”的描述存在不充分之处，不能据此推导统一比例。单位结论应带证券、周期、通道和原始请求。本库量纲证据见[K 线成交量报告](../validation/KLINE_VOLUME_UNITS_20261024.md)。

### 5.3 周期编号与时间

| 语义 | pytdxdata 枚举值 | rustdx `Period::tdx_category()` | 迁移说明 |
| --- | ---: | ---: | --- |
| 5m / 15m / 30m / 60m | 0 / 1 / 2 / 3 | 0 / 1 / 2 / 3 | 编号相同仍需验单位 |
| 日线 | 4 | 9 | 本库另接受4，但与9的量纲已测有差异 |
| 周 / 月 | 5 / 6 | 5 / 6 | 各通道保存范围仍不同 |
| 1m | 7 | 8 | 不要直接互抄数值 |
| 3m | 8 | 无独立 `Period` | Python 枚举存在，但 `is_minute` 没纳入3m，MAC时间解码不能据枚举存在视作验收通过 |
| 季线 | 10 | 10 | 具体返回须验 |
| 年线 | 9（另有 YEAR_ALT=11） | 11 | Python MAC构包会把YEAR映射11 |

pytdxdata 美股 K 线把“美东交易日 + 北京时间时分”的混合值归一化，并通过 `trading_date` 保留交易日；分时 `hour≥24` 拆成日偏移。本轮未验证这些规则对所有美股/时段的适用性。MAC 非分钟 K 线默认时分初始化为 15:15；这不是可直接当作交易所收盘时刻的证据。

rustdx 没有统一 start/end bar_time 开关。既有5分钟样本时间为09:35–15:00，不能因为另一个 SDK 写“原始start”就再次偏移本库时间。

### 5.4 缓存与新鲜度

pytdxdata 有内存缓存、可选磁盘缓存和获取合并。其默认 TTL 来自[缓存策略源码][p-ttl]：

| 类别 | 默认 TTL |
| --- | --- |
| 报价 | 5 秒 |
| 分时、当日逐笔 | 60 秒 |
| 历史逐笔 | 0，策略中表示永久不过期 |
| MIN_1/5/15/30/60 K 线 | 60 秒 |
| 非分钟前/后复权 K 线 | 86400 秒 |
| 其他 K 线 | 到次日00:30，最长12小时 |
| 目录/数量/股本/财务 | 86400 秒 |
| 未专门定义的缓存类别 | 60 秒 |

不是所有接口都通过缓存层；竞价和资金流等方法直接执行。历史分时仍走60秒策略；重复查询报价可能命中5秒缓存，不能把每次 await 都解释为重新访问主站。README 关于固定15分钟行情延迟的说法是作者描述，本文没有验证成对所有主站成立的统一延时。

## 6. rustdx 当前可调用入口

以下为本库现有 API，非建议中新接口。`R` 在表中表示 `std::io::Result`；`CR` 表示 `crate::Result`。[完整 Client 定义][r-client]。

| 方法及主要参数 | 返回 | 现有能力/边界 |
| --- | --- | --- |
| `new()` / `with_config(&TcpConfig)` | `R<Client>` | STD建连、自动初始化 |
| `tcp::ip::IpPool::new/default/list/get/add/remove/update/replace/snapshot/refresh/start_daily_refresh` | 对应列表/快照/Result | 核对期间新增的当日有效IP池源码；不是行情业务命令 |
| `heartbeat()` / `reconnect()` / `retry(f, attempts)` | 各自Result | 显式探活/重连/调用者指定重试 |
| `quotes(&[(u16, &str)])` | `R<Vec<QuoteData>>` | 0/1市场，0x053e，最多80 |
| `bars(market, code, category, start, count)` | `R<Vec<KlineData>>` | 单页1–800 |
| `index_bars(...)` | `R<Vec<IndexKlineData>>` | 指数解码，含涨跌家数 |
| `bars_range/index_bars_range(market, code, category, begin, end)` | 对应K线Vec | 闭日期区间，排序、去重及分页防护 |
| `k(market, code, begin, end)` | `R<Vec<KlineData>>` | category=9日线区间 |
| `k_batch(stocks, begin, end, max_parallel)` | `R<Vec<BatchKline>>` | 日线有界线程并发，各只结果独立 |
| `bars_batch(stocks, category, begin, end, max_parallel)` | `R<Vec<BatchBars>>` | 任意周期股票批量区间 |
| `k_adjusted(market, code, Adj, begin, end)` | `R<Vec<KlineData>>` | 本地日线Qfq/Hfq，只用类别1，量额不变 |
| `minute(market, code)` / `history_minute(market, code, date)` | `R<Vec<MinuteTimeData>>` | 仅价格/量，历史日期需调用者验证 |
| `transaction(market, code, start, count)` | `R<Vec<TransactionData>>` | 当日旧单页，count≤2000客户端校验 |
| `history_transaction(market, code, start, count, date)` | 同上 | 旧历史单页，日期校验已有，num补0 |
| `finance(market, code)` | `R<FinanceInfoData>` | 单只旧财务，空结果NotFound |
| `xdxr(market, code)` | `R<Vec<XdxrData>>` | 单只旧股本记录 |
| `stock_count(market)` / `stocks(market)` | `R<u16>` / `CR<Vec<SecurityListData>>` | 沪深证券总数/全目录 |
| `f10_categories(market, code)` / `f10(market, code)` | 目录Vec / `R<Vec<(String, String)>>` | 目录 / 全栏目正文 |
| `block(filename)` | `CR<Vec<BlockRecord>>` | 板块文件下载、完整长度检查和扁平解析 |

示例：

```rust
use rustdx_complete::tcp::stock::Client;
use rustdx_complete::data::Period;

fn query() -> std::io::Result<()> {
    let mut client = Client::new()?;
    let quotes = client.quotes(&[(0, "000001"), (1, "600519")])?;
    let bars = client.bars(0, "000001", Period::Day.tdx_category(), 0, 800)?;
    let ticks = client.transaction(0, "000001", 0, 800)?;
    println!("quotes={}, bars={}, raw_records={}", quotes.len(), bars.len(), ticks.len());
    Ok(())
}
```

ticks 在这里是未分类的旧记录，不能直接宣称全部是实际成交。网络示例本轮未运行；现有协议/解析测试结果沿用前次明确范围，不由这个示例产生新的实盘证据。

## 7. 尚需实现与验收

| 优先级 | 工作 | 完成时应具备的证据 |
| --- | --- | --- |
| 1 | 现有逐笔事件分类、分页/日期/文件参数策略、报价精度接入 | 状态8与非8的09:25夹具；页边界完整性；目录多小数品种；非法参数拒绝 |
| 2 | 现有目录/财务/股本/分时/逐笔批量封装及字段单位 | 单只与批量一致；无数据/失败语义；EPS及类别15独立核实 |
| 3 | 选择需要接入的新版STD/MAC能力：基础快照、竞价过程、字段报价、服务端复权 | 每个通道实际握手、构包/解码、固定原始响应和限定范围线上验证 |
| 4 | 增量推送 | 0x0547请求与游标、首帧基线、未配对帧路由、队列及增量合并；两个参考SDK不能互相代替 |
| 5 | 板块/指数成分与汇总、异动、行业/专业财报、EX扩展行情 | 成分覆盖/历史口径、部分失败、量纲/笔数标记、文件摘要和可重放样本 |
| 6 | CLI/runner 持续采集接入 | 库方法之外的调度、时限、留证、完整性及生产运行验收 |

这是缺口排序，不代表已经执行。若产品只要求沪深现有 STD 能力，可以按实际需求选择补齐项；不以“第三方方法数更多”自动要求全部复制。

## 8. 弃用方法迁移附录

固定源码保留 **33 个**弃用方法并发出 `DeprecationWarning`，注释计划1.0移除。[弃用方法源码][p-aliases]。它们不另外计为33种业务能力。下表同时记录实际返回差异，避免只改名称。

| 旧方法 | 当前推荐入口 | 实际注意事项 |
| --- | --- | --- |
| `get_kline` | `get_bars` | market/code改symbol |
| `get_kline_since` | `get_bars(start_date=...)` | since变日期参数 |
| `get_kline_batch` | `get_bars` | 旧方法仍返回以code为键的dict，新方法平铺list |
| `get_index_kline` | `get_bars` | 自动识别指数；注意default_adjust |
| `get_stock_kline_with_indicators` | `get_indicators` | 旧返回单个dict，新返回IndicatorSet列表 |
| `get_stock_quotes` | `get_quotes(fields=...)` | bits改fields |
| `get_stock_quotes_list` | `get_board_members` | 旧实际走raw分类成分函数，保留start；不是新0x054b |
| `get_transactions` | `get_ticks` | 旧默认count=2000，新默认None；高层会跨页并滤8 |
| `get_minute` | `get_minutes` | market/code改symbol |
| `get_minute_batch` | `get_minutes` | 旧code字典，新平铺list |
| `get_chart_sampling` | `get_minutes(sampling=True)` | 旧为float列表，新为无真实时间的MinuteBar列表 |
| `get_security_count` | `len(get_universe(...))` | **旧实际仍调用0x044e**；BJ空目录不能替代该数量语义 |
| `get_security_list` | `get_universe`后切片 | 新高层先全量，再本地分页 |
| `get_security_list_all` | `get_universe` | 统一模型与市场选择器 |
| `get_financial_file_list` | `get_financial_file_infos` | 旧文本行list，新结构化文件索引 |
| `get_financial_records` | `get_financial_records_parsed` | **旧返回原始dat bytes**，新为记录list |
| `get_company_info_category` | `get_f10` | section=None |
| `get_company_info_content` | `get_f10(section=filename)` | offset/length保留 |
| `get_block_info` | `get_file` | 原始bytes；解析改用get_block_parsed |
| `get_report_file` | `get_file` | 文件名选路 |
| `get_file_meta` | `get_file` | **旧实际仍返回MAC元信息**，新返回bytes；不可等价替换 |
| `download_file` | `get_file` | 旧filesize入参已不使用 |
| `get_board_list` | `get_boards` | board_type改kind |
| `get_belong_board` | `get_board_of` | market/code改symbol |
| `get_symbol_info` | `get_snapshot` | 旧单模型或None，新list |
| `get_goods_list` | `get_universe` | 旧按EX市场获取再切片 |
| `get_goods_count` | `len(get_universe(...))` | 仍注意EX非证券/重复项 |
| `get_goods_quotes` | `get_quotes` | 跨市场统一字段模型 |
| `get_goods_quotes_list` | 清单选择后`get_quotes` | 旧只取该市场清单前count项 |
| `get_goods_chart_sampling` | `get_minutes(sampling=True)` | float列表变MinuteBar列表 |
| `get_goods_tick_chart` | `get_minutes` | ymd=0变date=None |
| `get_goods_transaction` | `get_ticks` | 旧默认count=1800，高层不等于单页大小 |
| `get_goods_transaction_all` | `get_ticks(count=None)` | 尽量全量，仍受50页封顶 |

## 9. 来源与验证记录

本文来源优先级为：固定版本实际请求/解析及高层调用代码 → 项目测试/变更说明 → README/API文档的描述。pytdxdata 为社区 SDK，以上并非通达信官方完整协议规范。

本次下载并核对 **66 个选定源码/文档文件**，按 Git tree blob SHA 校验内容，并保存 SHA256、固定提交 URL、68个公开方法签名及本库来源摘要。详细材料见[证据 JSON](../../testing/evidence/pytdxdata-api-review-20261024.json)。GitHub head、PyPI和Release版本分别记录，避免将不同发布渠道混写。

本轮操作仅新增文档、证据与索引，没有编辑 Rust、没有再次执行 Cargo 检查或服务器探针。核对期间工作区另外出现的 IP 池与相关测试变动已明确标注，源码摘要按记录时快照固定。前次170个已有测试通过、外部实盘测试关闭的事实，见[前版验证章节](../validation/TDX_7709_FUNCTIONAL_AUDIT_20261024.md#5-本轮验证与证据边界)，不延伸为新增 IP 池测试通过。上游CHANGELOG中的性能、361测试通过和作者实测数字是上游报告，不记为本库本轮测试。

主要固定来源：

- [pytdxdata API_REFERENCE][p-reference]、[CHANGELOG][p-changelog]：辅助了解对外说明及版本变化，冲突处以源码为准。
- [统一API及构造器][p-init]、[模型][p-models]、[路由][p-router]、[代码解析][p-symbols]：方法、类型和跨通道行为。
- [STD目录][p-list]：业务opcode为044e/0450，目录请求16字节。0.6.9修复多发2字节污染连接；本库目录已经是16字节，不能将该上游旧缺陷算到本库。
- [STD K线][p-std-bars]、[MAC K线][p-mac-bars]、[MAC/STD逐笔][p-transactions]、[EX命令][p-ex]：真正构包/解码；不同头内字段不当作同一个命令号。
- [财务][p-finance]、[股本][p-xdxr]、[文件/F10][p-std-ext]、[MAC扩展][p-mac-ext]：单位、文件名与新增能力。
- [rustdx Client][r-client]、[checked请求与解码][r-requests]、[K线请求](../../../crates/rustdx-core/src/tcp/stock/kline_request.rs)、[本库目录](../../../crates/rustdx-core/src/tcp/stock/security_list.rs)、[数据周期](../../../crates/rustdx-core/src/data/model.rs)：当前实现直接证据。
- [原21项核对](../validation/TDX_7709_FUNCTIONAL_AUDIT_20261024.md)、[主动获取字段](UPSTREAM_ACTIVE_INTERFACES.md)：旧约束与业务字段背景。

原始上游 CHANGELOG / BJ 告警把数量/目录写成 `053d/053e`，实际类中报文是 `044e/0450`；不能照抄。财务单位注释、指数涨跌字段描述、偏移方向、弃用方法返回类型等差异已在上文分别纠正。

[commit]: https://github.com/openbot-coder/pytdxdata/commit/2cf3657f4b531827cc892f8656107514d636c921
[release]: https://github.com/openbot-coder/pytdxdata/releases/tag/v0.6.2
[version]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/pyproject.toml
[p-reference]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/docs/API_REFERENCE.md
[p-changelog]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/CHANGELOG.md
[p-init]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L235-L324
[p-models]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/models/__init__.py
[p-router]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/router.py
[p-symbols]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/symbols.py#L147-L280
[p-pagination]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/scheduler/paginator.py
[p-list]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/security_list.py
[p-std-bars]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/kline.py
[p-mac-bars]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/mac.py
[p-transactions]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/transaction.py
[p-ex]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/ex_ext.py
[p-finance]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/finance.py#L11-L64
[p-xdxr]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/xdxr.py
[p-std-ext]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/std_ext.py
[p-mac-ext]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/mac_ext.py
[p-mac-auction]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/mac_ext.py#L152-L181
[p-ttl]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/cache/ttl.py
[p-aliases]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1985-L2221
[p01]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L329-L358
[p02]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L360-L387
[p03]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L389-L391
[p04]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L475-L509
[p05]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L652-L669
[p06]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L671-L681
[p07]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L683-L693
[p08]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L699-L938
[p09]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L940-L972
[p10]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L978-L1157
[p11]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1159-L1233
[p12]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1239-L1255
[p13]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1257-L1273
[p14]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1275-L1283
[p15]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1285-L1299
[p16]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1305-L1325
[p17]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1327-L1337
[p18]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1339-L1341
[p19]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1343-L1345
[p20]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1362-L1381
[p21]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1383-L1406
[p22]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1408-L1418
[p23]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1420-L1434
[p24]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1436-L1439
[p25]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1441-L1462
[p26]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1525-L1571
[p27]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1573-L1695
[p28]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1697-L1726
[p29]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1732-L1775
[p30]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1777-L1784
[p31]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1786-L1789
[p32]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1791-L1794
[p33]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1796-L1858
[r-client]: ../../src/tcp/stock/client.rs
[r-requests]: ../../src/tcp/stock/requests.rs
[r-pagination]: ../../src/tcp/stock/client/pagination.rs
[r-f10]: ../../src/tcp/stock/company_info.rs
[r-file]: ../../src/tcp/stock/block_info.rs
