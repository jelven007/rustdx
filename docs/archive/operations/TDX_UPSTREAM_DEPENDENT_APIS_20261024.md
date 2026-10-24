# 依赖通达信上游数据的接口清单

版本：2026-10-24 / 1.0。范围为 pytdxdata **0.6.9 的33个当前方法**，沿用[完整接口文档](TDX_API_REFERENCE_20261024.md)的 P01–P33 编号。固定源码提交为 `2cf3657f4b531827cc892f8656107514d636c921`，版本、方法签名和文件摘要见[原核对证据](../../testing/evidence/pytdxdata-api-review-20261024.json)。

## 1. 筛选口径与数量

按 SDK 现有业务调用链，**31项依赖通达信上游数据**：19项直接查询、4项下载并解析文件、8项使用查询结果继续处理。8项中，P02是价格精度同步辅助接口，其余7项是指标或统计等业务结果。

| 类型 | 数量 | 纳入依据 |
| --- | ---: | --- |
| 直接查询 | 19 | 请求通达信行情、目录、公司资料或服务器元信息；部分路径支持缓存 |
| 下载并解析文件 | 4 | 调用文件下载，再本地解析板块、行业或专业财报 |
| 依赖查询结果处理 | 8 | 调用其他通达信查询接口，再同步精度、计算指标或汇总/排序 |
| 无需主动取得通达信业务数据 | 2 | P03状态查看、P14使用调用者参数计算涨跌停 |
| 合计 | 33 | 每个当前方法只统计一次 |

“依赖上游”表示数据最初由通达信查询取得；**缓存命中不改变数据来源，也不表示每次调用都必须联网**。这里按方法自身的业务调用链统计，不把 `TdxData.start()` 的连接池启动、节点探测和后台精度同步归到每个方法。`start/close` 和弃用方法不在这33项中。

下列通道均指通达信服务器：STD为普通行情通道，MAC为MAC行情协议通道，默认端口7709；EX为扩展行情通道，默认端口7727。pytdxdata是客户端封装，不是另一家数据供应商。

## 2. 直接查询通达信的19项

| 原编号 | 接口 | 主要参数 | 通达信来源/命令 | 取得的数据 |
| --- | --- | --- | --- | --- |
| P01 | `get_universe` | `market` | STD数量`0x044e`、目录`0x0450`；EX数量`0x23f0`、清单`0x23f5` | 证券/商品代码、名称、市场；STD价格精度、单位和昨收。[源码][p01] |
| P04 | `get_quotes` | `symbols, fields, concurrency, pipeline` | A股默认STD`0x053e`；字段报价MAC/EX`0x122b` | 行情价量、五档或自定义字段；两条路径可用字段不同。[源码][p04] |
| P05 | `get_snapshot` | `symbols` | MAC`0x122a` | 个股特征快照：名称、日期/时间、活跃度、换手、均价及价量。[源码][p05] |
| P06 | `get_auction` | `symbols` | MAC`0x123d` | 竞价时间、价格、匹配量和未匹配量。[源码][p06] |
| P07 | `get_capital_flow` | `symbols` | MAC`0x1218`，head=2 | 主力/散户买卖与资金流向字段。[源码][p07] |
| P08 | `get_bars` | `symbols, period, start, count, adjust, start_date, end_date` | STD`0x052d`；A股1分钟/复权MAC`0x122e`；EX`0x23ff` | OHLC、成交量、金额和时间；具体单位/字段取决于通道和品种。[源码][p08] |
| P10 | `get_ticks` | `symbols, date, start, count` | A股MAC`0x122f`；港股EX`0x23fc/0x2406`；其他EX市场使用MAC逐笔请求 | 聚合成交切片、量、状态；部分品种可用笔数。[源码][p10] |
| P11 | `get_minutes` | `symbols, date, sampling` | MAC/EX`0x122d`；EX采样`0x254d` | 分时价格、量、均价/时间；采样分支仅价格点。[源码][p11] |
| P12 | `get_xdxr` | `symbols` | STD`0x000f` | 股本变迁、除权除息和相关事件字段。[源码][p12] |
| P13 | `get_finance` | `symbols` | STD`0x0010` | 最新财务快照、股本、资产负债、收入利润等。[源码][p13] |
| P15 | `get_f10` | `symbol, section, offset, length` | STD目录`0x02cf`、正文`0x02d0` | F10栏目目录或公司资料正文。[源码][p15] |
| P17 | `get_unusual` | `market, start, count` | MAC`0x1237` | 市场异动事件、类型、值和时间。[源码][p17] |
| P18 | `get_server_info` | 无 | MAC`0x120f` | 服务器日期、上一交易日和交易时段。[源码][p18] |
| P19 | `get_kline_offset` | `offset, count` | MAC`0x124a` | K线偏移相关总数和本次返回数；高层没有证券参数。[源码][p19] |
| P20 | `get_boards` | `kind, count` | MAC`0x1231` | 实时板块清单、行情及领涨股信息。[源码][p20] |
| P21 | `get_board_members` | `board, count, sort_type, fields` | MAC`0x122c` | 板块成分证券及其字段报价。[源码][p21] |
| P22 | `get_board_of` | `symbols` | MAC`0x1218`，head=1 | 个股所属板块、板块名称及行情字段。[源码][p22] |
| P26 | `get_index_members` | `index, count, fields` | 指数映射到板块码后请求MAC`0x122c` | 当前指数成分及其报价。[源码][p26] |
| P29 | `get_file` | `name` | STD`0x02c5/0x06b9`或MAC`0x1215/0x1217` | 服务器文件原始字节；按名称前缀选通道。[源码][p29] |

P01的北交所目录在该版本直接返回空列表，不联网枚举；参数不支持、无记录或空结果也不等于接口摆脱了数据源依赖。P11的A股采样分支从查询到的分时派生价格点，因此仍列在直接查询方法中，不另重复计数。

## 3. 下载通达信文件后解析的4项

这4项本地执行解析，但 SDK 方法会先调用下载；`filename` 是服务器文件名，不是读取本机文件的路径参数。

| 原编号 | 接口 | 必需的上游内容 | 调用链 | 输出 |
| --- | --- | --- | --- | --- |
| P30 | `get_block_parsed(filename)` | `block_zs.dat / block_gn.dat / block_fg.dat`等 | `get_file(filename)` → `parse_block_dat` | 板块名称、类别、声明成分数和代码列表。[源码][p30] |
| P31 | `get_industry_map()` | `tdxhy.cfg` | `get_file("tdxhy.cfg")` → `parse_tdxhy_cfg` | 证券的通达信/申万行业映射。[源码][p31] |
| P32 | `get_financial_file_infos()` | `tdxfin/gpcw.txt` | `get_file(...)` → `parse_financial_file_list` | 专业财报文件名、MD5和大小。[源码][p32] |
| P33 | `get_financial_records_parsed(filename)` | `tdxfin/gpcwYYYYMMDD.zip/.dat`等 | `_fetch_financial_dat` → 候选名`get_file` → 提取dat → `parse_financial_dat` | 报告期、证券和财报原始字段序列。[源码][p33] |

P29的STD文件路径按名称前缀分流：`block_*`查元信息再分块，`tdx*`直接分块；其他名称使用MAC文件元信息与下载。当前高层 `get_file` 没有通用文件内容缓存，以上4个解析方法也没有自行读取本地备份的分支。离线处理已保存文件应调用解析函数，不能假定这些SDK方法会自动改读本地文件。

## 4. 使用上游查询结果处理的8项

这些方法没有单独的业务二进制命令，内部计算所需的数据仍来自通达信查询。SDK当前参数也没有为大多数方法提供“直接传入全部原始数据”的入口。

| 原编号 | 接口 | 直接依赖 | 最终通达信来源 | 上游数据之后的处理 |
| --- | --- | --- | --- | --- |
| P02 | `sync_decimal_points(markets)` | `get_universe` | STD目录`0x0450`的`decimal_point` | 构建运行时价格精度表，输出同步统计；属于辅助接口。[源码][p02] |
| P09 | `get_indicators(symbols, indicators, ...)` | `get_bars` | STD/MAC/EX K线 | 本地计算MACD/KDJ/RSI/BOLL/MA/EMA等。[源码][p09] |
| P16 | `get_market_stat()` | `get_quotes` | `sh880005/sh880001/sh880006`的STD快照`0x053e` | 解释为涨跌家数、量额、市值和涨跌停统计。[源码][p16] |
| P23 | `get_board_summary(board)` | `get_board_members` | MAC`0x122c`基础字段及`0x38`资金字段 | 汇总成分数、金额、主力净流入及涨跌家数。[源码][p23] |
| P24 | `get_board_ranking(kind, top_n)` | `get_boards` | MAC`0x1231`板块报价 | 按现价/昨收计算涨幅并本地排序。[源码][p24] |
| P25 | `get_board_change_ranking(kind, days, top_n)` | `get_boards` + `get_bars` | MAC板块清单 + 板块指数K线 | 在候选范围内用首末收盘价计算变化并排序。[源码][p25] |
| P27 | `get_index_trade_stats(index, date, ...)` | `get_index_members` + `get_ticks` | MAC`0x122c`成分 + 各成分MAC`0x122f`逐笔 | 按分钟汇总成分成交量、可用笔数和估算金额。[源码][p27] |
| P28 | `get_index_trade_stats_multi(index, dates, ...)` | `get_index_members` + `get_index_trade_stats` | 复用一次当前成分，获取多日成分逐笔 | 按dates顺序返回多日分钟统计。[源码][p28] |

P02通过 `decimal_sync.refresh` 间接调用目录，仍会使用目录缓存；该实现仅处理sh/sz，其他市场名会被跳过。P27传入 `members` 可以省掉成分查询，**没有省掉各成分逐笔查询**。P28高层没有members参数；内部先取一次成分，再逐日复用。

“本地计算”描述计算位置，不能据此判为整个接口无需通达信数据。若要将计算层独立出来，可保存上游原始数据，并对保存数据执行计算；这是调用组织的变化，不能当作这些现有SDK方法已经支持离线输入。

## 5. 未纳入的2项

| 原编号 | 接口 | 排除依据 | 业务输入可能怎样取得 |
| --- | --- | --- | --- |
| P03 | `decimal_point_status()` | 读取进程内精度状态，不发起目录/行情查询，也不取得新的市场数据 | 状态可以反映过去的上游同步，但查看本身不依赖重新取数据。[源码][p03] |
| P14 | `get_price_limits(symbol, name=None, pre_close=None, listed_days=9999)` | 只解析代码和按本地规则计算，没有自动查询昨收、名称或上市日期 | 这些参数可以由调用者提供；若调用者另外用通达信查询，依赖属于外层流程。[源码][p14] |

P14的输出正确性仍依赖输入事实和适用规则，但接口本身不要求这些事实必须来自通达信，也不是服务端特殊涨跌停表查询。

## 6. 使用这些接口时需要记录的来源信息

同一逻辑接口可能切换协议通道，也可能命中旧缓存。采集或保存结果时，建议同时记录以下信息，以便解释后续计算：

| 信息 | 需要记录的原因 |
| --- | --- |
| 方法、代码/市场、请求日期/周期及参数 | 明确查询目标，避免代码或周期编号混用 |
| STD/MAC/EX通道、业务命令、服务器和获取时间 | 同一方法可能走不同协议；提供来源及新鲜度依据 |
| 实际交易日、缓存命中/缓存时间 | “date=None”和本机日期不保证等于主站数据对应交易日 |
| 分页终止原因、实际条数、失败成员及成分版本 | 区分完整结果、短页、预算封顶和部分失败 |
| 字段可用性、量/金额单位和原始状态 | 空字段/占位0不等于真实0，派生计算需要正确量纲 |
| 文件名、内容摘要、报告期及解析规则版本 | 服务器财报候选名与索引可能不同，字段含义会随记录布局变化 |

依赖判断与业务语义应分别验证。特别是：

- `get_ticks()`在全部分页结束后过滤状态8；状态8是竞价快照，**非8且09:25**才是正式撮合。`get_auction()`取得的过程快照不能直接当正式成交。
- `get_index_members()`取得当前成分；历史汇总不能自动当作历史成分口径。`get_index_trade_stats()`默认忽略单只失败，应检查 `failed_members` 和 `trade_count_available`。
- `get_board_ranking/get_board_change_ranking`是SDK本地派生排名，候选范围有限，不代表服务器全市场排名。
- `get_finance()`的总量股本/金额已经在解码时乘10000；不能因旧注释写“万股/万元”而重复换算。

这些限制和参数签名详见[完整接口文档](TDX_API_REFERENCE_20261024.md)。依赖接口清单不表示rustdx已经实现了这些能力；实现映射继续以该文档的P01–P33表为准。

## 7. 核对来源与范围

本清单复用已固定的上游源码与证据，并再次检查了 `TdxData` 当前方法的调用链。31/19/4/8/2均为这33个方法的分类计数，不是上游证券数、市场覆盖率或实盘成功率。本轮只生成文档，未执行SDK联网查询或修改Rust实现。

- [API调用链源码][api]：公开方法、派生调用和文件下载/解析路径。
- [精度同步源码][decimal]：P02目录依赖与P03状态读取。
- [缓存TTL源码][ttl]：部分结果缓存命中时不发起新的获取；缓存不改变数据的初始来源。
- [文件/F10报文源码][files]、[MAC扩展命令][mac]、[EX清单/逐笔命令][ex]：核对实际业务命令。
- [完整接口文档](TDX_API_REFERENCE_20261024.md)与[固定方法/源码证据](../../testing/evidence/pytdxdata-api-review-20261024.json)：33项全集、签名及SHA256摘要。

[api]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py
[decimal]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/decimal_sync.py
[ttl]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/cache/ttl.py
[files]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/std_ext.py
[mac]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/mac_ext.py
[ex]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/protocol/commands/ex_ext.py
[p01]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L329-L358
[p02]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L360-L391
[p03]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L389-L391
[p04]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L475-L650
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
[p33]: https://github.com/openbot-coder/pytdxdata/blob/2cf3657f4b531827cc892f8656107514d636c921/src/pytdxdata/api.py#L1796-L1849
