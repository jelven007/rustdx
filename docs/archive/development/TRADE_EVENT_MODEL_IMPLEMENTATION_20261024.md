# 成交事件模型与 TradeCollection 实施记录

2026-10-24。本轮落实[成交全量与分类分析](TRADE_PAGINATION_CLASSIFICATION_ANALYSIS_20261024.md)中的事件模型与带终止原因的集合。Rust 2024 / MSRV 1.89，使用现有同步网络层。

## 1. 可调用入口

所有新类型从 `rustdx_complete::tcp::stock` 导出：

| API | 作用 |
| --- | --- |
| `TradeQuery::today(market, code)` | 经校验的0fc5查询上下文；日期为None，报文不携带交易日 |
| `TradeQuery::history(market, code, date)` | 经校验的旧0fb5历史上下文；date只是请求日期 |
| `TradeQuery::recv_page(tcp, start, count)` | 单页请求，附返回连接的主站、进程内唯一连接ID、UTC获取时间 |
| `Client::trade_page(market, code, start, count)` | 当日单页便捷入口 |
| `Client::history_trade_page(market, code, date, start, count)` | 旧历史单页便捷入口，注意参数顺序 |
| `decode_trade_page(&query, start, count, &payload)` | 离线解码；来源为None |
| `TradeCollection::new(query, pages, termination)` | 校验已获取页面，派生覆盖状态、分类计数、时间范围和警告 |

单页数量1–2000、start为u16；这只是客户端参数范围，不表示主站保证每页返回2000条。`TradePage::next_offset()`返回u32，用原始条数推进，调用方须用`u16::try_from`构造下一请求。短非空页与全是竞价快照的页面仍推进；空页不推进。

旧`transaction()/history_transaction()`以及旧协议对象继续返回`TransactionData`。新旧入口共用`trade_decode.rs`的严格解码，旧CSV字段与数值保持一致，历史缺失计数槽仍兼容输出0。

```rust
use rustdx_complete::tcp::stock::{
    Client, TradeCollection, TradeQuery, TradeTermination,
};

fn latest_window(client: &mut Client) -> std::io::Result<TradeCollection> {
    let query = TradeQuery::today(0, "000001")?;
    let page = client.trade_page(0, "000001", 0, 1800)?;
    let termination = if page.raw_count() == 0 {
        TradeTermination::EmptyPageObserved
    } else {
        TradeTermination::CallerStopped
    };
    TradeCollection::new(query, vec![page], termination)
}
```

上例只获取一个观察窗口；非空时coverage为Partial。集合本身不执行网络循环。本工作区独立新增的`transaction_all()/history_transaction_all()`、`TransactionCollection`与增强历史0fc6模型是其他工作，本轮新集合不与它们混用或改变其返回签名。

## 2. 原始事件与分类

`TradeRecord`保留分钟、页内价格差分、累计原始价格、原始量、可选计数槽、状态、尾字段和原始记录字节范围。`TradePage`保留完整解压payload和旧历史4字节前缀；`record_bytes(index)`提供对应记录的原始编码。前缀不添加到累计价格中。

模型字段只读，仅Serialize，不直接Deserialize，防止未经校验的分类、计数或覆盖状态被当作可靠构造结果。页序与页内顺序保留；`pages()`下标就是集合页索引。`records()`不自动排序或去重，重复分钟、计数槽和完全相同的事件仍可保留。记录不是交易所唯一事件ID。

分类规则版本为`TRADE_SEMANTIC_SCHEMA_VERSION=1`：

| 条件 | 事件/阶段 | 资格 |
| --- | --- | --- |
| 状态8，任何时间 | AuctionSnapshot / AuctionProcess，方向Unknown | ExcludedByProfile |
| 非8且09:25 | OpeningMatch / Opening | 已支持0/1/2为当日ConfirmedByProfile；未知状态保持Unknown |
| 非8且15:00 | Trade / ClosingMatch | 已支持0/1/2为当日ConfirmedByProfile；未解释状态不自动确认 |
| 状态5且15:05–15:30，含边界 | Trade / AfterHoursFixedPrice，方向Unknown | 当日ConfirmedByProfile |
| 状态0/1/2且09:30–11:30或13:00–14:59 | Trade / Regular，分别Buy/Sell/Neutral | 当日ConfirmedByProfile |
| 已知数值处于未解释时段 | Trade或Unknown / Unknown | Candidate |
| 新状态值 | 保留值与时间视图 | Unknown |

旧0fb5使用独立`LegacyHistory0fb5`标识：有支持条件的成交分类降为Candidate，`trade_count_raw=None`。它没有与0fc5/0fc6完全等价的语义证据。状态8仍按此兼容profile排除；这也是profile解释，不是官方逐字段认证。

`confirmed_trades()`只包含ConfirmedByProfile。该资格指社区协议profile支持，不是交易所独立核验。盘后方向Unknown不影响总成交视图，`confirmed_unknown_direction`单独计数；`counts().trades`是Trade事件类行数，可能含Candidate，不是已确认笔数。

`price()`保持这两个旧协议的兼容/100口径，不借用仅针对053e报价的精度目录。`volume_raw()`不假设所有证券都是手，不自动×100；计数槽不求和推断真实笔数，尾字段不推断为未匹配量。本轮不新增量额汇总。

## 3. 终止原因与覆盖状态

`TradeTermination`包含实际空页、偏移/页数/记录数/字节数限制、Deadline、Cancelled、ConnectionChanged、DateChanged、DecodeError、TransportError、RepeatedPage和CallerStopped。错误原因可携带message，成功取得的页面仍保留。

模型校验查询身份、固定页大小、由原始条数推进的连续偏移、空页后无追加、已知主站/连接ID一致，以及当日获取没有跨北京时间自然日。跨连接的页面拒绝拼接；调用方应将旧页面以ConnectionChanged终止，并从0建立新集合。`Tcp::connection_id()`即使同地址重连也变化，而且不同Tcp实例不会以“代次都是0”误认为同一连接；持久归档仍需要run_id区分不同进程。

| coverage | 派生条件 |
| --- | --- |
| SourceEnumerated | 历史请求、从0开始、全部页有同一连接来源、实际终端空页 |
| Partial | 任一限制/错误/主动停止，或首start非0 |
| Unknown | 从0读到空页，但缺来源；或0fc5无法确认交易日与盘中偏移稳定性 |

`EmptyPageObserved`必须对应实际末页空页；短非空页不能作为空页结束。`OffsetLimit`必须有不可转换为u16的下一偏移。其他预算原因由调用方声明，模型没有分页调度器，不能自动证明预算被触发。

SourceEnumerated只说明这个源端连接返回范围被枚举到空页，不证明主站保留了全部成交、该日无成交、盘中无缺口或与交易所总量一致。没有DateVerified/IndependentlyChecked的提升入口。未知来源、旧历史语义、未确认事件、盘中日期与非0起点通过`warnings()`公开。

重复整页检测和边界重叠策略留给采集器；集合不会仅因为两个合法同分钟页面完全相同就删行或报完整。原始归档可使用`(run_id, page_index, row_index)`做存储幂等键。

## 4. 验证

新增`tests/trade_models.rs`的7项集成测试，覆盖分类优先级和时段边界、旧历史Candidate、真实单页原始字段与旧JSON/CSV值一致、完整截断检查、负差分、非法数值、偏移上限、原始计数推进、重复分钟与同价量、混合查询/页大小/偏移错误、全部终止原因、缺来源降级以及本地TCP同地址重连。另有模块单元测试覆盖主站变化和UTC同一天内跨北京时间自然日的拒绝规则。

固定真实夹具是2026-10-22已有单页：各3条、状态均为5，15:15/15:16/15:28、价格11.30、原始量合计132；当日计数槽10/1/1，旧历史无此槽。夹具出处见[manifest](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)。本地TCP构造回放仅验证传输和模型，不作为实盘全天支持证据。

最终工程检查均退出0：

| 检查 | 结果 |
| --- | --- |
| Rust1.89 `fmt --all -- --check` | 通过 |
| `clippy --workspace --all-targets --all-features --offline --locked -- -D warnings` | 通过，无警告 |
| `test --workspace --all-targets --all-features --offline --locked` | 332通过、0失败、5忽略，包含新模型8项与旧协议兼容6项 |
| `test --workspace --doc --all-features --offline --locked` | 64通过、0失败、18忽略；新集合使用示例通过 |
| `git diff --check` | 通过 |

上述测试覆盖当前整个未提交工作区，也包含其他成交分页、增强历史和财务/报价改动，不能将332项都归为本轮新增。测试使用`RUSTDX_SKIP_INTEGRATION_TESTS=1`，真实网络用例按此开关跳过；ignored用例含指定CLI/数据库配置的runner验收。文档中的ignored多为示意或联网示例。本轮没有采集真实全天多页、独立核验量额或进行真实数据库验收。

复现环境为仓库内`CARGO_HOME=.cargo`、`RUSTUP_HOME=.cargo/rustup`，命令：

```sh
export CARGO_HOME="$PWD/.cargo"
export RUSTUP_HOME="$PWD/.cargo/rustup"
export RUSTDX_SKIP_INTEGRATION_TESTS=1
.cargo/bin/cargo +1.89.0 fmt --all -- --check
.cargo/bin/cargo +1.89.0 clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
.cargo/bin/cargo +1.89.0 test --workspace --all-targets --all-features --offline --locked
.cargo/bin/cargo +1.89.0 test --workspace --doc --all-features --offline --locked
```
