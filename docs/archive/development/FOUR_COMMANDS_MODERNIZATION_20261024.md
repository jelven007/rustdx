# 四条替代命令迁移与验证

2026-10-24。依据固定 eltdx 3.2.2 协议字段，在现有 Rust 2024 / MSRV 1.89、同步 I/O、有界连接与分页器上独立实现。上游研究许可源码未作为运行依赖或复制进核心实现。保留工作区原有修改；Cargo 版本仍为 1.12.2，源码未发布。

四条默认链路已迁移。沿用[22命令核对](../validation/ELTDX_3_2_2_CURRENT_IMPLEMENTATION_20261024.md)的口径，本阶段为 **13同码、0替代、9未接入**。后续九项也已完成，最新为22同码，见[九命令实施记录](NINE_COMMANDS_IMPLEMENTATION_20261024.md)。命令覆盖统计不表示业务全兼容。

| 能力 | 默认命令与API | 本轮补齐 |
| --- | --- | --- |
| 心跳 | `0004`，`Tcp/Client::heartbeat` | 空请求体、日期确认原字段、30秒空闲自动保活、完整帧串行、失败失效与关闭回收 |
| 证券目录 | `044d`，`stocks/security_directory/security_directory_page` | u32偏移、最多1600条/页、37字节记录、16字节GBK名称、f32昨收及原始槽、完整目录校验与精度缓存 |
| 当日分时 | `0537`，`minute/minute_series` | 固定三varint布局、首点基准、独立均价尺度、合成时分、目录decimal 2/3/4、来源及raw正文 |
| 历史成交 | `0fc6`，`history_transaction/history_trade_page/history_transaction_all` | 默认单页及全量接入、笔数/状态/尾槽、零价快照、分类、参考字节与无损事件页 |

## 调用与约束

```rust,no_run
use rustdx_complete::tcp::stock::{Client, TransactionPaginationOptions};

let mut client = Client::new()?;
let ack = client.heartbeat()?;
println!("server date: {:?}", ack.server_date);

client.refresh_quote_metadata(&[0, 1])?;
let series = client.minute_series(0, "159915")?;
for point in &series.points {
    println!("{} {} {}", point.time_label, point.price, point.average_price);
}

let history = client.history_transaction_all(
    0, "300308", 20261023, TransactionPaginationOptions::default(),
)?;
for page in &history.event_pages {
    for event in page.records() {
        println!("{:?} {:?} {:?}", event.event_kind(), event.phase(), event.qualification());
    }
}
# Ok::<(), std::io::Error>(())
```

`heartbeat()`现返回`HeartbeatAck`，包含6字节保留槽、u32原始服务端日期、可选合法日期和完整正文。0/非法日期保留为原值及`None`。日期确认不意味着另外一个无日期业务帧来自同一天。IP池三轮服务探测仍使用既有044e证券数请求；受管连接的手动、空闲及池健康心跳均使用0004。

每个受管连接最多一个同步心跳worker，数量受连接许可约束。完整业务请求/响应和心跳持有同一帧锁；读缓冲容量为1，避免预读另一个响应。业务流量刷新空闲时间，持续繁忙时不额外发送心跳。后台失败关闭socket、标记不可复用，后续请求重连；不会在分页背后更换连接。macOS关闭后`peer_addr()`的EINVAL也进入恢复路径。等待帧锁遵守业务截止时间；Drop和重连先停止worker，再释放socket和连接许可。可以`set_heartbeat_interval(None)`关闭。裸I/O需先关闭心跳或`into_inner()`；后者先等待正在进行的有界心跳并停止worker。

`security_directory_page`检查市场0/1、数量1–1600，offset为u32，count在wire编码为u32。解码拒绝数量超请求、截断、尾随字节和非法浮点；decimal未知值原样保留，使用价格时由精度层决定是否支持。类别/板块是代码前缀提示，非服务端权威分类，未匹配保留Unknown。完整目录先取得044e数量，短页按实际长度继续；重复代码、提前空页、跨连接或跨北京时间自然日均失败。默认页间100ms，共用分页总体时限。`stocks()`映射原返回字段并原子更新完整精度缓存；单页不会发布成完整市场。

0537首点分别提供价格、均价基值，后续字段相对首点，不进行逐点累计。价格除以`10^decimal`，均价除以`100 × 10^decimal`；量保留原始单位。缺失、陈旧或不支持精度报错，不按代码猜测或静默/100。时间是已支持的240分钟会话标签：09:31–11:30、13:01–15:00；响应无日期，`trading_date=None`。未知会话超过240点明确拒绝。首次目录和分时共用分页总体时限，热目录下单请求沿用请求时限；请求前后都检查精度有效日。

0fc6每页1–1800条，u16偏移，合法YYYYMMDD来自请求。参考f32原字节独立保留，页内价格累计从0起；不把参考槽加入累计。原始笔数槽不是交易ID。状态8优先分类为快照并从已确认成交视图排除；09:25开盘撮合、15:00收盘和盘后状态5按版本化模型分类，未知资格不静默升级为已确认。

全量结果的`records`是兼容价格/量/笔数视图，按较早页面在前输出；`event_pages`按请求顺序保留完整正文、字段槽、参考字节和`TradeSource`，可取得每条原字节范围。偏移始终按全部原始事件数推进，短页继续，同分钟重复不去重。保留既有100页、条数、8MiB正文、120秒总时限和100ms节流预算；空页复查计入预算、同连接执行，至少等待300ms。预算耗尽、重连、重复整页、逆序、截断、取消和超时均报错，不成功交付部分结果。空页只证明本次主站可枚举范围结束。

## 兼容入口

| 原协议 | 显式入口 |
| --- | --- |
| 044e证券数探活 | `Tcp::heartbeat_legacy`、`Client::stock_count`、`SecurityCount` |
| 0450旧目录 | `Client::stocks_legacy`、`stock::stocks_legacy`、旧`SecurityList` |
| 051d及旧历史回退 | `Client::minute_legacy`、旧`MinuteTime` |
| 0fb5旧历史成交 | `history_transaction_legacy/history_trade_page_legacy/history_transaction_all_legacy`、`TradeQuery::history_legacy`、旧请求和解析器 |

旧协议夹具仍用显式旧入口验证。当前默认历史`num`来自0fc6笔数槽；旧0fb5返回0表示缺失槽，不能混为已知真实笔数。详细破坏性API变化见[迁移文档](../../release/MIGRATION.md)。

## 真实帧与独立核对

新采样主站`115.238.56.198:7709`，采集时间2026-10-24 05:05 UTC，先经同地址当日三轮验证。保存42组完整请求/响应头/压缩正文/解压正文，使用[Python标准库独立核对脚本](../../../scripts/validation/verify_modern_commands.py)重解，未导入eltdx。

- 0004原始服务端日期20261023，保留字段及日期解析匹配。
- 044d深市24460条、沪市27711条，合计52171条；逐记录核对代码、长名称、量单位、decimal、f32槽与原字节，计数和分页连续。
- 0537平安银行、创业板ETF、沪深300ETF各240点，合计720点；逐字段核对首点基准、价格、均价和量，末点11.2/2.702/4.178均与053e快照相符。
- 两只ETF分钟量合计与快照相等；平安银行分钟量合计1448806，快照1448805，相差1个原始量单位，未将其改写或作为全天覆盖证明。
- 0fc6复用已有真实300308历史单页1800条，新的事件模型逐条价格累计、笔数、状态及尾槽与原增强解码器一致；全量分页的故障/预算/偏移行为使用本地线级测试。

新[完整采样目录](../../testing/evidence/modern-four-commands-20261024)、[独立核对结果](../../testing/evidence/modern-four-commands-20261024/independent-verification.json)及[原0fc6留证](../../testing/evidence/enhanced-history-0fc6-20261024)均保留可重放原始数据。这是单主站、有限证券的休市样本，未验证全服务器、全部品种和交易所逐笔完整性；decimal4分时另有合成算术回归，不冒充真实样本。

## 工程检查

Rust 1.89离线锁定依赖执行`cargo fmt --all --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`和`cargo test --workspace --all-features --no-fail-fast`。工作区406项通过、0失败、23项ignored，外部网络测试以`RUSTDX_SKIP_INTEGRATION_TESTS=1`关闭；真实验证通过上述独立采样另行执行。

新增`tests/modern_commands.rs`包含10项协议/字段/实际I/O回归，覆盖长名称与u32、均价和2/3/4位精度、缓存默认链路、短页/重复/空页、手动与空闲心跳、业务串行、失败重连、裸socket交接、等待截止时间和Drop中断。既有12项全量分页测试同时覆盖当日及0fc6历史路径。Tcp栈内对象实测256字节，维持固定大小上限；线程、帧、记录和正文仍受明确预算约束。
