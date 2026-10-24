# 增强历史成交 0fc6：独立接入与验证

> 后续更新：本记录描述首次单页接入。当前`history_transaction/history_transaction_all/history_trade_page/TradeQuery::history`均默认使用0fc6，旧0fb5经显式legacy入口保留。新增无损全量分页及默认迁移见[四命令实施记录](FOUR_COMMANDS_MODERNIZATION_20261024.md)。

日期：2026-10-24。实现基线为仓库留存的 eltdx 3.2.2 `HistoricalTicksRequest` / `parse_historical_ticks_payload`。本次新增同步单页请求、独立解码、原始结果和 SDK 分类视图，已有旧历史入口继续发送 `0x0fb5`。Cargo 版本不变，源码尚未发布。

## 可调用入口

```rust
use rustdx_complete::tcp::stock::Client;

let mut client = Client::new()?;
let page = client.enhanced_history_transaction(
    0, "300308", 0, 1800, 20261023,
)?;

println!("原始事件：{}", page.raw_count());
println!("实际成交视图：{}", page.actual_trades().count());
println!("竞价快照：{}", page.auction_snapshots().count());
for record in page.opening_matches() {
    println!("开盘撮合 {} {}", record.time, record.price);
}
```

低层入口为 `EnhancedHistoryTransactionRequest::new(...).recv(&mut tcp)`；离线入口为 `decode_enhanced_history_transactions(&request, &payload)`。两者返回 `EnhancedHistoryTransactionPage`，解码不依赖 eltdx 或新增运行时依赖。

| 参数 | 约束与含义 |
| --- | --- |
| market | 显式 `u16`，0 深圳、1 上海；不从代码推断 |
| code | 六字节 ASCII 字母或数字，与仓库现有 checked 请求一致；语法通过不代表证券存在 |
| start | `u16`，0–65535；0 从最新端取页，增大偏移访问更早记录 |
| count | **1–1800**；按固定 eltdx 3.2.2 profile 限制，区别于旧接口的 1–2000 |
| date | 非零、真实 YYYYMMDD 日历日期，年 1–9999；不自动检查是否交易日或主站保留范围 |

非法参数在发送之前返回 `InvalidInput`。调用按现有同步 TCP 的网络许可、请求时限、取消、自动重连及 FrameObserver 规则执行；一次请求的传输、解码共用请求时限。空页返回原始空页，不执行旧历史回退。

## 请求与响应布局

请求总长 28 字节，消息 ID 为 1：

```text
12字节帧头，opcode=c6 0f
+ date:u32 LE
+ market:u16 LE
+ code:6 bytes
+ start:u16 LE
+ count:u16 LE
```

解压后的响应：

```text
count:u16 LE
+ price_base_raw_f32:4 bytes
+ count × [
    time_minutes:u16 LE,
    price_delta:varint,
    volume:varint,
    order_count:varint,
    status:varint,
    reserved:varint
  ]
```

旧 `0fb5` 每条只有四个 varint，没有 `order_count`；增强 `0fc6` 有五个。增强解码独立实现，不能把旧响应送入新布局，也不把旧历史缺失笔数填成增强历史的零笔。

价格差分每页从 0 开始累计，除以 100，遵循固定 SDK profile。四字节参考价单独保留，**不加到成交价格上**；真实样本参考价约 590.11，首条成交价 599.00，避免了重复叠加。新报价目录精度模块没有参与此协议转换，多小数品种仍需单独核实。

## 字段与视图

| 页字段 | 含义与限制 |
| --- | --- |
| market / code / date | 来自请求；响应没有证券身份或交易日回显，不能作为主站确认的交易日 |
| start / request_count | 请求偏移与请求条数 |
| records / raw_count() | 全部原始混合事件、数量；保留页内顺序，不先过滤、不去重 |
| price_base_raw_f32 / price_base_raw_bytes | 参考价槽和原四字节；原始位模式保留 |
| raw_payload | 完整解压正文，用于复现；完整压缩帧可由现有 FrameObserver 保存 |
| source_endpoint / fetched_at | 成功响应的主站、UTC 获取时间；离线解码两者均为 None |

| 记录字段 / 方法 | 含义与限制 |
| --- | --- |
| index / absolute_index | 页内索引、start+index；后者使用 u32，不是稳定成交 ID |
| time_minutes / time | 当日分钟、HH:MM 文本；无秒和交易所序号 |
| trade_datetime | 请求日期加分钟构造的无时区时间，秒填 0；不意味着响应提供了秒或日期 |
| price_delta_raw / price_acc_raw / price | 原差分、i64 页内累计、累计/100价格；保留零价格，负累计报错 |
| volume | 原数量，非负；股、手或其他单位依品种另核实，不自动换算成交金额 |
| order_count | 增强布局中的原笔数槽，非负；不是序号、主键或去重依据，竞价时不推断未匹配量 |
| status_raw | 原方向/状态，包括 5、8、未知正值和负值 |
| reserved_raw | 尾 varint，非零、负值均保留，不强制填零 |
| side() | `EnhancedTradeSide`：0 Buy、1 Sell、2 Neutral，其他 Status(raw) |
| event_kind() | `EnhancedTradeEventKind`：先 status=8 → AuctionSnapshot，再非8且09:25 → OpeningMatch，其余 Trade |
| is_actual_trade() | SDK 兼容判断 `status!=8`；未知状态仍可出现在这个视图，不能据此认定已验证业务含义 |
| is_after_hours_fixed_price() | status=5 且 15:05–15:30，包含两个端点 |

页上的 `actual_trades()`、`auction_snapshots()`、`opening_matches()`、`after_hours_trades()` 返回借用记录的迭代视图，不复制或删除原始事件。未知状态保留；这些是固定 SDK 分类规则，不代替交易所业务核实。新的其他成交模型可能采用更严格资格规则，此接口按独立 profile 使用。

## 分页与频率

本入口每次只取一页，不返回“已全量”结论。`next_start()` 用**原始事件数**推进：

- 非空短页仍给出下一偏移；不能凭短页结束。
- 整页都是竞价快照仍继续推进。
- 空页返回 `None`，仅表示此次主站响应为空。
- 超出 `u16` 返回错误，不环绕，不将地址上限当正常结束。

`next_start()` 只提供偏移计算，不冻结盘中数据、验证日期或检测换连接。不同请求的相同分钟、相同价量、相同 order_count 均不能直接去重。页序倒置与页内顺序保留、连接代次、预算、全量覆盖检查应由全量采集层另行负责；本次没有将 `0fc6` 接入已有旧协议全量入口、CLI、数据库或 runner。

历史补数建议按需下载并缓存，稳定交易日每日一次补齐即可。此为调用策略建议，**不是服务端公开 QPS 限额**；不要将历史全天下载当实时轮询。保留深度、空数据日期和其他品种支持仍依主站，不能从本次单证券结果外推。

## 真实采样与独立核对

2026-10-24 04:40 UTC，在 `115.238.56.198:7709` 查询 `sz300308`，请求日为 `20261023`，每次请求量 1800：

| start | 原始条数 | 时间范围 | status=8 | 非8视图 | 开盘撮合 | 盘后固定价 |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| 0 | 1800 | 13:30–15:29 | 19 | 1781 | 0 | 45 |
| 1800 | 1800 | 10:30–13:30 | 0 | 1800 | 0 | 0 |
| 3600 | 1276 | 09:15–10:30 | 68 | 1208 | 1 | 0 |
| 4876 | 0 | 空页 | 0 | 0 | 0 | 0 |

共 4876 条原始事件，87 条竞价快照，非8视图 4789 条。开盘记录为 **09:25、价格572.01、volume=8871、order_count=1681、status=2**。该末端空页仍带六字节数量/参考价头。

原始请求、16 字节响应头、压缩体、解压体、主站、预验证结果、摘要和独立期望保存在[支持包](../../testing/evidence/enhanced-history-0fc6-20261024/README.md)。Python stdlib 独立解码逐行核对全部 13 个记录字段，参考价核对原始 f32 位模式；4876 条全部一致。四次采样使用独立连接，不将结果标记为同会话全天枚举，不声称交易所全天无漏无重，也未运行完整第三方 SDK。

采集和复核：

```sh
cargo run --example capture_enhanced_history_transaction -- \
  /tmp/new-0fc6-capture 115.238.56.198:7709 0 300308 20261023 0 1800
python3 scripts/validation/verify_enhanced_history_transaction.py \
  /tmp/new-0fc6-capture --freeze
```

示例有 30 秒任务时限、12 帧留证上限，显式主站预验证，关闭自动重连，只发一条业务请求；输出目录必须不存在。`--freeze` 仅在原始 Rust `page.json` 逐字段匹配后创建新期望文件，拒绝覆盖已有期望。

## 工程验证

专项测试覆盖固定 SDK 合成报文、上述四组真实压缩帧及全部独立期望、09:25状态优先、盘后边界、未知状态/尾字段、笔数重复、零价格、负差分、i64累加、1800条、偏移上限、非法参数、逐字节截断、超量/尾随/变长溢出、取消，以及本地分片TCP压缩/空页/旧历史回放。fuzz 入口也已加入新解码器。

最终检查命令和结果见[证据汇总](../../testing/evidence/enhanced-history-0fc6-20261024.json)。原有协议真实夹具继续用于兼容回归，不将工作区其他成交分页/分类改动计为本次独立接入成果。
