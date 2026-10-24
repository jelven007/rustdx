# 现有0fc5/0fb5成交明细全量分页实施说明

> 后续更新：当前历史默认全量入口已使用0fc6，并新增无损`event_pages`；本记录保留最初0fb5分页实施时的内容。显式旧全量入口为`history_transaction_all_legacy`。最新合同与验收见[四命令实施记录](FOUR_COMMANDS_MODERNIZATION_20261024.md)。

日期：2026-10-24。实现沿用同步TCP、Rust 2024、最低Rust 1.89及现有网络预算。原`transaction`、`history_transaction`单页接口保留；新增严格全量枚举。增强历史0fc6与事件分类属于独立接口，本分页器使用现有原始0fc5/0fb5解码器。

## 调用

```rust,no_run
use rustdx_complete::tcp::{
    TcpConfig,
    stock::{Client, TransactionPaginationOptions},
};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut client = Client::with_config(&TcpConfig {
        recheck_empty: true,
        ..Default::default()
    })?;
    let options = TransactionPaginationOptions {
        page_size: 800,
        page_delay: Duration::from_millis(100),
        ..Default::default()
    };

    let today = client.transaction_all(0, "000001", options.clone())?;
    let history = client.history_transaction_all(0, "000001", 20261022, options)?;
    println!("当日原始事件{}条；历史{}条", today.records.len(), history.records.len());
    Ok(())
}
```

两个接口均返回`io::Result<TransactionCollection>`。`market`仅接受0/1，证券代码必须为六位ASCII字母数字；历史日期必须是合法YYYYMMDD。证券/日期/页大小/零预算错误在发送分页请求前返回`InvalidInput`。

[可运行示例](../../../crates/rustdx-core/examples/transaction_all.rs)输出分页元信息、原始条数、首尾记录及来源，不把原始条数当交易所真实成交笔数：

```sh
cargo run --example transaction_all -- today
cargo run --example transaction_all -- 20261022 115.238.56.198:7709
```

指定地址也须通过独立IP池的当日三轮测试；示例自动执行现有连接初始化。示例固定查询深市000001，可按业务修改市场和代码。

## 分页与顺序

1. 从`start=0`请求最新端，增加偏移访问更早记录；每只证券的一次枚举串行执行。
2. `next_start = start + raw_page.len()`，短非空页继续。包含状态8的整页也继续，过滤后的条数不参与分页。
3. 只有实际解码为空的页才作为结束观察。启用`TcpConfig::recheck_empty`时，同偏移、同连接延迟至少300ms再查一次；第二次非空则正常追加并继续。
4. 内部宽位累计偏移，每次构包前checked转换为u16。下一页不可表示时返回`OffsetLimit`，不能回绕或把最后非空页当终点。
5. 返回记录反转页序，保留每页wire顺序；检查页内分钟非递减、旧页末条分钟不晚于新页首条。分钟相等允许。
6. 不按时间、num、价量或状态去重。完整整页在不同偏移再次出现时返回`RepeatedPage`，表示重复/合法相同页之间存在歧义，不能作为正常结束或直接删页。

例如请求大小3，实际依次返回2、3、2、0条，偏移必须是0、2、5、7。最后输出早页2条、中间页3条、最新页2条；同一分钟的相同记录全部保留。

## 预算与调用间隔

| 选项 | 默认 | 使用约束 |
| --- | ---: | --- |
| `page_size` | 1800 | 1–2000；沿用客户端校验，不证明所有服务器支持2000 |
| `max_pages` | 100 | 最大逻辑页数，包含终止空页；空页复查占同一页、最多额外一次请求 |
| `max_records` | 67535 | 最大保留原始条数；达到准确上限后仍允许count=1的空页探测，非空报`RecordLimit` |
| `max_bytes` | 8MiB | 全部解压正文累计，包含被复查替代的首个空正文；收到协议有界帧后、解码前检查 |
| `timeout` | 120秒 | 覆盖全部页、节流、空页复查与传输重试；全局分页时限及外层OperationContext只能进一步缩短 |
| `page_delay` | 100ms | 收到一页响应后到下一页请求的等待；可设0；不是官方限频或进程级请求配额 |

`max_pages`、`max_records`、`max_bytes`、`timeout`须大于0。100ms只是保守客户端间隔；其他Client、IP池测速、握手及传输层重试仍按各自既有策略执行，不能由此推导账户总频率。

协议start最大65535；最后可请求页可能覆盖更大的记录位置，但如果其后偏移超过65535，就不能继续以空页验证终点，严格接口报错。提高页数/条数预算不能突破这个地址范围。

## 来源、日期与错误

`TransactionCollection`包括：

- `market/code/command`：固定查询身份；command为0fc5或0fb5。
- `requested_date`：历史请求日，当日命令为None；该日期来自请求，不是服务端回显。
- `observed_date`：开始枚举时客户端北京时间自然日，不冒充服务端交易日。
- `source_endpoint/connection_generation`：该枚举的主站及本地连接代次。首个成功页前可自动恢复连接；此后发生同地址重连或换主站都拒绝拼接。
- `started_at/fetched_at`：查询开始和结束时间。
- `records`：全部原始事件，包含5/8/未知状态；旧历史num兼容为0，当日num不是唯一ID。
- `pages`：保持请求顺序，从最新页到终止空页；每页含start、request_count、raw_count、最终records中的record_range、正文累计字节及调用次数。
- `requests/payload_bytes`：逻辑请求调用和解压正文累计，包含空页复查；requests不包括传输层内部重发。
- `termination=EmptyPageObserved`：观察到主站枚举结束，不是全天完整性认证。

当日查询每页传输前后检查北京时间日期；跨日报`DateChanged`。历史查询固定显式日期，仍检查会话变化。TCP新增内部连接代次，仅在重连成功时递增；即使地址和文件描述符重用，也能检测中途重连。

局部分页失败由`TransactionPaginationError`承载，可通过`io::Error::get_ref().downcast_ref()`取得`reason/start/pages/records`。原因有`PageLimit/RecordLimit/ByteLimit/OffsetLimit/ConnectionChanged/DateChanged/RepeatedPage/InvalidOrder`；不会以Ok返回部分记录。连接/日期变化为`ConnectionAborted`，歧义重复/逆序为`InvalidData`，预算/偏移耗尽为`Other`。传输、解码、取消、总时限错误保留原io kind及接口/偏移上下文。

默认不自动从0重启整个枚举。失败后调用者可在新会话从0重跑；不要继续追加旧偏移。需要部分原始帧诊断时，可使用既有`FrameObserver`归档请求、响应头和压缩正文；本结果保留页元信息，不重复保存正文。

## 验证与证据边界

新增12项本地wire回归，覆盖两个协议的原始计数、短页续取、整页状态8、保留5、页内/跨页相同记录、笔数槽差异、同会话空页复查、准确条数上限后的空探测、页/字节预算、65535合法偏移与65536/66000拒绝、后续坏页、同地址自动重连、总时限、取消、外层时限及发送前参数校验。另有1项内部日期保护回归，模拟客户端观察日变化，并验证历史显式日期不随客户端自然日改变。

验证命令使用Rust 1.89、离线锁定依赖及`RUSTDX_SKIP_INTEGRATION_TESTS=1`；新增本地TCP测试不受该开关跳过。全工作区检查包含并行开发的其他现有改动，不将它们计为本次分页功能。

```sh
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --offline --locked
```

格式检查与全工作区Clippy（零警告）通过；核心库测试335项通过、18项文档示例ignored，新增13项分页回归全部通过。已有联网/真实数据库测试按环境开关跳过，不能将其记作本次实盘或数据库验收。完整工作区首轮在`rustdx-cmd/tests/cli_deadline.rs:71`的HTTP CLI耗时断言失败，独立复测也失败；因此完整工作区未全绿。显式排除该项后其余工作区测试395项通过、23项ignored、1项filtered，包含runner的压缩帧重连归档回放。

同日后续已修复该CLI测试：超时计时从服务端收到HEAD开始，取消计时从写入取消文件前开始，启动阶段另有3秒就绪约束；1600ms/800ms阈值保留。服务端保持GET连接直到CLI退出，不再通过1200ms主动断开制造错误；增加子进程超时终止和失败清理。连续3次复跑通过，完整工作区执行中的该测试也通过；注入1000ms启动延迟时，运行超时约1116ms、取消约307ms，原CSV、失败报告及连接预算释放均验证。`tcp_wire`示例采集使用与SecurityCount回放一致的`heartbeat_legacy`，修复新版心跳返回类型导致的编译失败。后续格式与全工作区Clippy通过；该轮全仓另有`tests/modern_commands.rs`的4项新接口测试失败，后续结果见下段。

这4项随后全部修复并验证。目录夹具按实际16字节名称字段填充，补充满16字节ASCII及8个GBK汉字的解析/后续浮点位置检查；心跳失败用例等待旧线程退出，再明确验证macOS关闭套接字的peer_addr错误和新连接ID，稳定覆盖地址读取失败后的恢复分支。最新`modern_commands`共10项测试全部通过，连续3轮复跑均通过。全仓测试411项通过、0失败、23项ignored、0项filtered；格式及全工作区Clippy（零警告）通过。验证包含工作区并行新增的启动功能，启动测试的Observer补充Debug派生以满足FrameObserver约束；联网和真实数据库验收仍按原环境开关跳过。

固定`115.238.56.198:7709`的两个实际API试跑均成功，使用1800条/页、同连接空页二次确认：

| 命令 | 请求日期 | 非空页条数 | 终止偏移 / 空页确认 | 原始记录 / 解压正文累计 |
| --- | --- | --- | --- | --- |
| 0fc5 | 无日期参数；客户端观察日20261024 | 1800、1800、1142 | start=4742，连续两次空页 | 4742条 / 37146字节 |
| 0fb5 | 20261023 | 1800、1800、1142 | start=4742，连续两次空页 | 4742条 / 31929字节 |

查询分别于北京时间2026-10-24 12:46:48–12:46:49、12:46:12–12:46:13完成，均5次页调用、连接代次0，首尾分钟09:15–15:22；首条保留状态8、末条保留状态5。0fc5没有服务端日期，不能将周六客户端观察日解释为这些记录的交易日。此试跑记录分页摘要，没有保存全部wire帧，也没有独立核对日量额；只能证明该主站此次枚举结束及该范围下的页序/短页续取。

既有真实单页夹具通过独立协议解码回归；本次本地多页/2000条测试证明客户端算法及边界，不是服务端2000条支持、实盘全天完整性或冻结盘中快照的证据。盘中新增事件仍可能移动偏移，同连接也不冻结数据；完整日归档应等相应交易阶段结束，重新枚举，并核对日期、单位、主站覆盖与独立量额。

验证结果及单主站试跑摘要：[实施证据](../../testing/evidence/trade-pagination-implementation-20261024.json)。

代码：[分页实现](../../../crates/rustdx-core/src/tcp/stock/client/transactions.rs)、[公开模型](../../../crates/rustdx-core/src/tcp/stock/transaction_pagination.rs)、[wire回归](../../../crates/rustdx-core/tests/transaction_pagination.rs)。原问题分析：[全量与分类分析](TRADE_PAGINATION_CLASSIFICATION_ANALYSIS_20261024.md)。
