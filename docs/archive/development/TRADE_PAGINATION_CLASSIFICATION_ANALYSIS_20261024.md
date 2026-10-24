# 成交明细全量分页与事件分类：当前问题、影响和补齐设计

日期：2026-10-24。范围为 rustdx 当前 `0x0fc5` 当日明细、`0x0fb5` 旧历史明细，并对照固定 eltdx 3.2.2 的 `0x0fc5/0x0fc6` 及分类模型。**本轮为分析和文档交付，没有修改成交实现，也没有新增实盘采样。** 工作区其他部分已有财务与报价改动，本分析仅按重新读取的成交代码判断，不复用先前全项目快照结论。[1](#source-1)

后续实施：原始事件分类、单页元信息和带终止原因的`TradeCollection`已新增，实际API与边界见[实施记录](TRADE_EVENT_MODEL_IMPLEMENTATION_20261024.md)。本文保留分析时的快照，下面的“当前缺口”和拟议API不作为实施后的实时状态。

**当前问题是“单页查询”和“原始混合事件”被当成“全天真实成交”使用。** 缺少分页器会漏掉请求页之外的数据，缺少分类会混入竞价快照或误丢盘后成交。只加循环还不能证明全量：主站变化、盘中偏移漂移、终止判断、日期和去重都会影响完整性。

## 1. 当前已经有什么，缺少什么

| 层次 | 当前实现 | 主要缺口 |
| --- | --- | --- |
| 请求 | 当日 0fc5、历史 0fb5；start/count 参数 | 无增强历史 0fc6 |
| 单页校验 | 沪深市场、代码、条数 1–2000、历史日期 | 无统一完整页元信息 |
| 解码 | 数量、时分、差分价格、量、当日 num、状态 | 历史 num 补 0；尾字段和历史头 4 字节丢弃 |
| 解码防护 | 拒绝截断、超请求量、多余字节、非法时间、负量 / 负 num | 成交状态 / 单位 / 日期业务解释尚未建模 |
| 返回 | Vec<TransactionData> | 无证券身份、交易日来源、主站、页号、完整性状态 |
| 全量 | 调用者可手动改变 start | 无自动翻页、预算、终止原因、连贯性检查 |
| 分类 | 保留 buyorsell 原值，包括 5/8 | 无 event_kind、phase、实际成交 / 竞价视图 |
| 测试 | 真实单页、参数 / 截断 / 状态保留及本地回放 | 无真实全天多页、09:25 对照、盘中漂移与跨主站证据 |

当日入口：[Client::transaction](../../../crates/rustdx-core/src/tcp/stock/client.rs)；历史入口：[Client::history_transaction](../../../crates/rustdx-core/src/tcp/stock/client.rs)。两者各执行一次 request.recv，没有页循环。[2](#source-2)

## 2. 字段为什么容易被误用

### 2.1 `buyorsell` 实际承载方向和状态

该字段不是只有 0/1/2 的纯买卖方向。当前解码器有意保留未公开值；现有真实夹具包含 5，测试构造报文还验证 8 不被丢弃。[3](#source-3)

| 原值 | 3.2.2 基线解释 / 使用方式 | 当前误用风险 |
| --- | --- | --- |
| 0 | buy | 可作为该协议已命名方向，不能推广其他通道 |
| 1 | sell | 同上 |
| 2 | neutral | 中性方向 |
| 5 | 特定时间段的盘后固定价成交 | 只留 0/1 会误删；不能统一当成竞价 |
| 8 | 竞价快照，非实际成交 | 当中性成交会污染量额 |
| 其他 | 保留原始状态 | 不应自动归入买 / 卖；需注明业务资格是否已核实 |

`examples/test_transaction.rs` 中三处把 8 显示为“中性”，并把它计入 neutral_count；这与基线分类冲突。这里定位到的是示例层误导，**不能据此声称当前库已经在后台求和污染数据库**，因为现有 CLI/runner 没有这条成交采集链路。[4](#source-4)

### 2.2 `num` 不是唯一成交 ID

当前注释“成交编号”、示例“最新成交序号”会让调用者误以为可以用它去重或续传。基线把同一槽命名为 `order_count`，当前真实当日样本的值为 **10、1、1**，已经出现重复。[5](#source-5)

因此：

- 它不能作为主键、单调序号或“下一页 start”的依据。
- 明细一行可能对应聚合切片，不能把返回行数当成交易所真实笔数。
- 旧历史 0fb5 没有这个槽，当前补 0；增强模型应该使用 `None/Unavailable`，而非将 0 解释为真实零笔。
- 当前低层名 num 可为兼容保留，但新模型应命名 `trade_count_raw`，同时附可用性 / 口径。

### 2.3 `time` 只有分钟，价格和量也依赖品种

当前 time 为 HH:MM，无日期、秒或交易所序号。相同分钟内可存在多条不同或完全相同价量记录，不能按时间唯一去重。

价格仍由页内差分累计后 `/100`；`TransactionRequest::recv` 没有使用新报价精度模块。报价精度已改进不代表成交解码自动改进。量字段注释为手，但不能未核实就推广股票、指数、ETF、债券全部品种；`price × vol × 100` 只是“量确为 100 股/手”时的估算金额。[2](#source-2)

## 3. 现有真实样本已经能证明的影响

本轮用独立 Python 字节解码重新读取并核对仓库保留的两个真实响应，摘要与 fixture manifest 一致，没有生成新的行情数据。[5](#source-5)

| 协议 | 3 条记录的时间 | 状态 | 量字段合计 | 当日 num / 历史可用性 |
| --- | --- | --- | ---: | --- |
| 当日 0fc5 | 15:15、15:16、15:28 | 全部 5 | 132 | 10、1、1 |
| 旧历史 0fb5 | 15:15、15:16、15:28 | 全部 5 | 132 | 原布局无 num，当前填 0 |

该页价格都是 11.3，若按股票每手 100 股估算，金额为 149160 元；这不是响应直接提供的成交金额，也不是全天金额。

**只保留 buyorsell∈{0,1} 会把该页 3 条全部丢掉。** 按基线 status=5 且 15:05–15:30 的规则，它们属于盘后固定价成交，应该保留在实际成交中、买卖方向可保持未知。

这些样本不含 status=8、09:25 或全天多页，不能用它们证明所有主站的竞价规则、全天完整性或 1800/2000 极限。现有验证 8 的报文是本地构造样例，时间 09:30，只证明解码保留原值，不是真实竞价数量证据。

## 4. “全量”要分成三个完成层次

| 层次 | 完成条件 | 可以声称什么 |
| --- | --- | --- |
| 页解码完成 | 宣告条数全部解码、没有尾部或非法记录 | 此次单页报文已解析 |
| 主站枚举完成 | 相同上下文连续翻页，实际观察到结束，并记录完整终止状态 | 取得该主站此次可枚举的范围 |
| 指定交易日数据完整 | 交易日确认、主站覆盖可信、无换连接 / 漂移缺口，并与独立日量等证据核对 | 在限定证券 / 日期 / 单位范围验证完整 |

遇到空页只能作为“主站此次返回空”的观察，不自动证明该证券全天无成交。达到 max_pages、超时、offset 溢出、连接切换或响应异常，都不能返回一个没有 incomplete 标记的“全量成功”。

## 5. 分页的具体问题

### 5.1 最新端偏移，不是从开盘顺序向后读

基线的 start=0 取最新页，start 增大访问更早记录。要输出时间顺序，应该反转**页顺序**，保留每页原始顺序；不把整个结果逐条反转。

当前项目的真实样本只取 start=0，没有多页方向验证。新分页器应先用固定主站的至少两页真实样本确认该旧协议的偏移方向和页内顺序，再承诺“完整时间升序”。明细中同一分钟的 wire 顺序需要保留。

### 5.2 必须按原始条数推进，过滤不能参与分页

```text
raw_page_count = 该页全部混合记录数
next_start     = start + raw_page_count

actual_trades  = 对保留的记录执行事件分类后的视图
```

如果先删除 status=8，再用剩余条数推进，就会重新请求已经访问的原始位置；若该页全为竞价快照，还可能误判成空页并漏掉更早实际成交。

`request_count`、原始 count、actual_count、auction_count 应分别记录，不共享一个模糊的 count。

### 5.3 短页不能未经验证就当作结束

3.2.2 高层遇空页才终止，按返回条数推进，短页继续。当前 K 线分页则短页即停，这个规则不能直接复用到成交。

新明细分页可按“非空继续、空页记录终止”的策略起步，同时校验：

- 每页原始数量不得超过本次请求。
- 任何页 / 字节 / 时间预算触发要返回明确 partial / error。
- 空页二次确认策略仅在固定会话及剩余预算内执行；重连后不能简单接着旧偏移追加。
- 多次异常空页、主站数据缺失与真实无数据需要保持不同状态。

**基线自身的 `has_more` 也不是完整性证明。** 固定 3.2.2 的 TradePage/TradeBatch 源码都写 `count > 0`，仅表示非空时可继续尝试；不是“返回条数达到请求数”，也不是主站给出的 total / end 标志。前版手册对此的描述已在本轮修正。[6](#source-6)

### 5.4 u16 偏移限制还在

当前和基线的 start 都是 u16，最大 65535。页大小 1800、max_pages=100，不代表可以查询 180000 条。

内部应使用较宽整数累计，并在**每次构包前** checked 转换到 u16。接近上限的一页可能包含更大位置的返回记录，但下一页偏移不可表示，就无法通过继续请求空页确认终点；此时应记录 OffsetLimit，不宣布已完整。

不能用 `wrapping_add` 或直接强转：溢出会回到较新的位置，可能重复拉取甚至永远循环。提高 max_pages 不能突破协议地址范围。

### 5.5 盘中新增记录会推动偏移

按最新端计数时，抓到第一页面后新增成交会把旧记录向更大偏移推。随后用上一页长度计算的 start，可能重叠已取记录；不同的主站同步深度、截断和记录组成变化，还可能形成缺口。

**同一连接可以降低主站差异，但不能冻结盘中数据。** 当前 Tcp 支持自动重连、主站轮换和每日有效池检查，因此持有一个 `&mut Client` 也不等于整个全量查询不会变化。[7](#source-7)

| 采集场景 | 建议 |
| --- | --- |
| 盘中显示最近记录 | 查询最新页，明确是观察窗口 |
| 盘中尽量追增量 | 固定连接、重叠读、保留来源与缺口；没有稳定 ID 时不能承诺无漏无重 |
| 全天归档 | 相关交易阶段结束后、指定交易日、固定主站 / 连接重新枚举 |
| 页中途重连 / 换主站 | 终止当前会话；有预算可从 0 重启一次全量，不能跨上下文直接拼接 |
| 主站 / 日期无法确认 | 保留结果和 UnknownCoverage，不发布为完整 |

需要纳入盘后固定价成交的证券，应等其对应盘后阶段结束；不能所有品种都统一 15:00 后立刻标记当天完成。

## 6. 去重为什么不能直接复用 K 线实现

K 线“一根一个周期时间”支持按时间去重，明细不具备这项唯一性。

| 去重键 | 风险 |
| --- | --- |
| time | 同分钟全部压成一条 |
| num | 当日已出现重复；旧历史全部 0 |
| time + price + vol + status | 多条合法事件可能完全相同 |
| raw record hex | 价格差分依赖页上下文，相同成交跨页编码可不同；相同编码也不保证同一成交 |
| absolute_index=start+index | 是本次偏移坐标，盘中漂移 / 换主站后不稳定 |

完整归档应保存每页及页内顺序，建立**归档行唯一键**，例如 `(run_id, page_index, row_index)`；这个键仅使存储幂等，不表示交易所事件唯一 ID。

可对重复整页、异常时间方向和边界重叠设置检查，但：

- 同一分钟占满多页是可能的，不能像 K 线那样用“最早时间必须严格变小”判错。
- 非空重复整页可能是服务器重复或合法相似数据，需要返回 RepeatedPage / Ambiguous 状态；不能把重复当最终正常结束。
- 动态增量去重若没有稳定 ID，只能作为带不确定性的策略；最终重新抓稳定完整日进行核对。

## 7. 事件分类的建议规则

### 7.1 分开表示事件类型、交易阶段和方向

事件类型决定是否可以计入实际成交；交易阶段说明开盘 / 收盘 / 盘后；方向负责主买 / 主卖，三者应分开。

| 条件，按优先级判断 | event_kind | phase | direction / 聚合资格 |
| --- | --- | --- | --- |
| status==8，任何时间 | AuctionSnapshot | AuctionProcess | 不计实际成交，方向 Unknown |
| status!=8 且 time=09:25 | OpeningMatch | Opening | 按基线是实际撮合；方向单独取或 Unknown |
| status!=8 且 time=15:00 | Trade | ClosingMatch | 按基线是实际撮合 |
| status==5 且 15:05–15:30 | Trade | AfterHoursFixedPrice | 真实成交，方向可 Unknown |
| status∈{0,1,2} 的其他正常记录 | Trade | RegularOrUnknown | buy / sell / neutral |
| 未核实的新状态或异常时段 | Unknown/TradeCandidate | Unknown | 保留，聚合规则可返回待核实 |

基线的分类器将所有非 8 非 09:25 归为 Trade；本项目增强模型可以保持基线视图，同时额外记录 `qualification=ConfirmedByProfile/Candidate/Unknown`，避免把未来未解释状态默默当作已验证交易所成交。

旧历史 0fb5 不是基线增强 0fc6。可以先补 raw status 与分类视图，但套用规则时必须带 `protocol_profile=Legacy0fb5`、语义证据等级；不能仅因为同一个数值就声称不同协议字段全部等价。

### 7.2 先状态、后时间

```text
if status_raw == 8:
    event_kind = AuctionSnapshot
elif time_minutes == 9*60+25:
    event_kind = OpeningMatch
else:
    event_kind = Trade

phase 和 direction 再分别判断
```

如果先判断 09:25，就会把 09:25 的 status=8 快照误分类为正式开盘成交。056a 的 09:25 仍是过程快照，正式开盘撮合应来自经过上述筛选的成交明细。

### 7.3 量额、笔数和竞价数量

| 统计 | 合适输入 / 约束 |
| --- | --- |
| 原始事件数 | 全部记录数量，包含快照 |
| 实际成交行数 | 按协议语义筛选后的记录数，不自动等于真实笔数 |
| 成交笔数估计 | 仅可用且含义核实的 trade_count 槽；旧 0fb5 返回 None |
| 成交量合计 | 实际成交且 volume_unit 已确定的记录 |
| 成交额估计 | 实际成交价 × 可换算的数量，标注估算而非报文直接金额 |
| 主买 / 主卖统计 | 实际成交且 direction 已知；另列 UnknownDirection 量，不能直接丢掉 |
| 虚拟匹配 / 未匹配量 | 056a 的专用字段；不从 0fc5 / 0fb5 的 volume/num 推断 |

盘后成交方向未知时，可以计入总成交量额，同时排除方向性分析。未匹配量不能从 num 还原，连续竞价快照的数量也不能逐点求和。

## 8. 适合当前同步架构的补齐设计

本节是拟议 API 与模型，**当前尚不能调用**。沿用 Rust 2024 / MSRV 1.89、同步 I/O、有界线程。

### 8.1 原接口兼容，新增完整结果

保留现有单页 `transaction()` / `history_transaction()` 返回签名；新增模块建议为 `trade_models.rs / trade_decode.rs / client/trades.rs`。新模型应由共享原始解码结果派生，避免两套逻辑逐渐不一致。

```text
Client::trade_page(market, code, options) -> TradePage
Client::history_trade_page(market, code, date, options) -> TradePage
Client::trades_all_today(market, code, options) -> TradeCollection
Client::trades_all_history(market, code, date, options) -> TradeCollection
Client::trades_all_batch(stocks, date, options) -> Vec<BatchTradeResult>
```

| 模型 | 至少应包含 |
| --- | --- |
| TradeRecord | time_minutes、price_raw / price、volume_raw、trade_count_raw: Option、status_raw、event_kind、phase、direction、qualification、尾字段 |
| TradePage | market/code、请求日期与日期来源、start/request_count/raw_count、记录、page_index、raw payload、来源主站、连接代次、获取时间 |
| TradeCollection | 页元信息、原始总数 / 分类计数、termination、coverage、时间范围、缺口 / 警告、可选汇总 |
| TradeQueryOptions | page_size、max_pages/max_records/max_bytes、总 deadline、协议版本、RawPolicy、缺失 / 重连策略 |
| BatchTradeResult | 每证券独立结果与失败，不将部分失败压成空列表 |

`TradeCollection` 的“结束条件”和“数据覆盖结论”分开：

```text
termination:
    EmptyPageObserved / OffsetLimit / PageLimit / RecordLimit /
    ByteLimit / Deadline / ConnectionChanged / DecodeError / RepeatedPage

coverage:
    SourceEnumerated / DateVerified / IndependentlyChecked /
    Partial / Unknown
```

没有观察到完整条件时，严格入口抛错；诊断入口可返回 partial，但必须显式标注。流式输出时先写临时归档，最后附完成标记；没有完成标记的文件不作为全量发布。

### 8.2 每证券串行、多证券有界并行

一只证券的一天：

1. 选择已验证主站 / 会话，固定代码、日期、协议、页大小，建立全量任务 deadline。
2. 执行原始单页解码，留证并记录页元信息。
3. 用原始 count 推进 checked 偏移，非空继续，不用 filtered_count 判断终点。
4. 持续检查连接代次、日期、预算、重复页和异常方向；不自动跨主站合并。
5. 终止后按已验证页方向合并，保留页内顺序，分类与汇总，输出完整性。

多证券复用当前有界连接池 / 线程门控，各证券各自持有连接完成所有页，再归还。不能对同一证券的不同页开多连接并发抢抓，主站截面和偏移变化会更难控制。

页迭代器 / visitor 适合流式归档，统计可在不复制全部记录的情况下逐页计算；真正需要正向全天序列时，再按页顺序整理。原始记录先存、分类用索引 / 迭代视图，不为每个视图复制整份数据。

### 8.3 先分类与现有协议分页，再单独接 0fc6

| 阶段 | 范围 | 判据 |
| --- | --- | --- |
| A | 修正文档 / 示例，原始状态 / num / 日期元信息，新分类模型 | 本地边界与已知真实记录对照；未知状态不静默归类 |
| B | 当前 0fc5 / 0fb5 自动全量与完整性报告 | 固定主站真实多页、原始计数推进、u16 / 超时 / 换连接保护 |
| C | 0fc6 增强历史独立请求 / 解码 | 固定真实报文，笔数字段 / 参考价 / 分类核实；不能复用旧布局 |
| D | 定时与归档 | 盘中窗口与盘后全天分任务、批量预算、失败 / 缺口报告 |
| E | 量额 / 方向业务验收 | 指定证券 / 日期 / 单位与独立日量、竞价 / 盘后字段交叉核验 |

0fc6 是单独缺口；当前“增加全量循环”不等于已经接入增强历史命令。分类本身不要求先实现 056a，但完整竞价数量研究必须等待 056a。

## 9. 测试和业务验收应补哪些用例

### 9.1 离线边界与本地传输

| 用例 | 应验证 |
| --- | --- |
| 09:25 status=8 / 非8 并存 | 状态优先，只有非8为正式开盘撮合 |
| 15:00、15:05、15:30、15:31 | 实际成交与盘后阶段边界，时间不被一概过滤 |
| status=5/8/未知值 | 原始值保留；方向与成交资格分开 |
| 某页全部 status=8 | 不把 filtered_count=0 当枚举结束 |
| 短非空页之后还有一页 | 继续翻页，按原始长度推进 |
| 空页 / max_pages / u16 边界 | 终止原因分离，不产生假全量 |
| 同分钟满多页、重复价量 | 不按时间或 num 误去重 |
| 断线、同地址重连、换主站 | 会话变化可检测，停止 / 从 0 重启 |
| 截断、超量、额外尾字节 | 整页报错，不返回前半页“成功” |
| 旧 0fb5 与增强 0fc6 | 无 num 与有笔数槽的布局不会混用 |
| deadline / 取消 / 字节预算 | 覆盖所有页和重启；不每页重置总预算 |

本地构造用例验证算法和边界，不能当作服务端真实支持证据。

### 9.2 真实采样

最少需要两页以上的同证券同日期样本及结束确认，同时记录主站、连接、请求、完整响应头 / 压缩体 / 解压体和摘要。覆盖：

- 普通股票连续竞价；09:25 正式撮合与 status=8 快照。
- 15:00 正式收盘和有盘后固定价交易的品种。
- 活跃证券同一分钟多条记录，页边界可能跨同一分钟。
- 1800 与 2000 返回行为，客户端接受不等于服务端保证。
- 旧 / 增强历史结果对照及没有数据的日期。
- 少量多小数 / 非股票品种，明确哪些字段无法按股票单位聚合。

与日线成交量核对前，要统一交易日、是否含盘后、数量单位、主站覆盖和更新时间。金额若由明细估算，可与独立金额比较但不默认逐分精确一致。总量相等也不能单独证明没有相互抵消的重复 / 缺失，应结合页边界和原始留证。

## 10. 本轮证据与限制

本轮重新运行：

```bash
CARGO_HOME=/Users/bytedance/Documents/rustdx/.cargo \
RUSTUP_HOME=/Users/bytedance/Documents/rustdx/.cargo/rustup \
RUSTDX_SKIP_INTEGRATION_TESTS=1 \
.cargo/bin/cargo +1.89.0 test -p jelven-rustdx-complete \
  --test protocol_decoders --offline --locked
```

结果为 **6 项通过、0 失败**，包括单页真实夹具、截断 / 超量 / 身份 / 日期 / 数值校验、未知状态保留与本地传输回放；测试中包含财务 / 报价的工作区现有改动，不将它们认作本轮成交改动。

[专项证据 JSON](../../testing/evidence/trade-pagination-classification-20261024.json)保存成交源码、测试、示例摘要，独立解码结果，真实夹具出处、状态分布、基线分类 / has_more 行为。本轮未新增 Rust 代码、未采集实盘全天多页、未测试 1800/2000 服务端极限、未运行第三方 SDK。

## 11. 来源

<a id="source-1"></a> **[1] 当前专项快照。** [证据 JSON](../../testing/evidence/trade-pagination-classification-20261024.json)。当前 HEAD 与未提交工作区、读取范围和源码摘要。

<a id="source-2"></a> **[2] 当前单页调用 / 解码。** [Client](../../../crates/rustdx-core/src/tcp/stock/client.rs)、[requests.rs](../../../crates/rustdx-core/src/tcp/stock/requests.rs)、[TransactionData](../../../crates/rustdx-core/src/tcp/stock/transaction.rs)、[旧历史布局](../../../crates/rustdx-core/src/tcp/stock/history_transaction.rs)、[有界 Reader](../../../crates/rustdx-core/src/tcp/reader.rs)。

<a id="source-3"></a> **[3] 状态保留与现有测试。** [protocol_decoders.rs](../../../crates/rustdx-core/tests/protocol_decoders.rs)、[基线 trades.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/trades.rs)。

<a id="source-4"></a> **[4] 示例中的误导命名。** [test_transaction.rs](../../../crates/rustdx-core/examples/test_transaction.rs)、[test_readme_examples.rs](../../../crates/rustdx-core/examples/test_readme_examples.rs)。仅定位问题，本轮未修改示例。

<a id="source-5"></a> **[5] 保留的真实单页。** [fixture manifest](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)、[当日响应](../../../crates/rustdx-core/tests/fixtures/protocol/transaction.response.hex)、[历史响应](../../../crates/rustdx-core/tests/fixtures/protocol/history-transaction.response.hex)、[当日期望 CSV](../../../crates/rustdx-core/tests/fixtures/protocol/transaction.expected.csv)、[历史期望 CSV](../../../crates/rustdx-core/tests/fixtures/protocol/history-transaction.expected.csv)。

<a id="source-6"></a> **[6] 3.2.2 分类、分页和 has_more。** [trade.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/trade.py)、[trade_batch.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/trade_batch.py)、[trades API](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/api/trades.py)。源码事实与 SDK 基线规则，非官方完整协议标准。

<a id="source-7"></a> **[7] 重连、预算与留证。** [tcp/mod.rs](../../../crates/rustdx-core/src/tcp/mod.rs)、[governance.rs](../../../crates/rustdx-core/src/tcp/governance.rs)、[FrameObserver](../../../crates/rustdx-core/src/tcp/evidence.rs)、[现有批量实现](../../../crates/rustdx-core/src/tcp/stock/client/batch.rs)。
