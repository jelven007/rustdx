# 0feb 非空响应与 0547 连续推送边界验证

> 后续更新：已固定24组真实0feb非空帧、5760条独立期望，并补充交易窗口判定测试，见[最新夹具与推送窗口报告](NONEMPTY_FIXTURES_PUSH_WINDOW_20261024.md)。以下正文保留前一轮采样与480项回归快照；0feb响应日期真实性和0547真实盘中推送仍未证明。

日期：2026-10-24。基于 Rust 2024 / MSRV 1.89、同步 I/O 和受管连接实现。**本地非空回放与连续推送用例已通过；真实 0feb 非空数据、0547 交易时段主动推送仍未验收。** 本文补充[九命令实施报告](../development/NINE_COMMANDS_IMPLEMENTATION_20261024.md)，不改变其真实样本来源。

## 1. 执行结果

| 项目 | 结果 | 证明范围 |
| --- | --- | --- |
| 当前工作区全 features 测试 | 480 通过、0 失败、24 忽略 | 包含其他未提交改动；外部数据库/旧 live 测试按既有离线开关排除 |
| Rust 1.89 格式检查 | 通过 | workspace 格式 |
| 全 workspace / targets / features Clippy | 通过，`-D warnings` | 包含新增测试和采样示例 |
| `recent_minutes_boundaries` | 5 通过 | 固定非空字节、全部精度、完整分钟会话、异常与本地压缩传输 |
| `quote_subscription` | 21 通过 | 包含既有会话测试及补充的连续推送、状态、游标和资源边界 |
| `command_boundaries` | 10 通过 | 数量、日期、100 证券、未知尾部、初始游标等相关边界 |
| `validate_quote_push` 示例测试 | 3 通过 | 交易日/窗口判定、验收判定、行情变化与游标的共同条件 |
| Python 独立核验自测 | 3 通过 | 非空手写数据、推送变化与交易窗口判定；自测为合成材料 |

测试日志、源码摘要和采样结果索引见[机器证据](../../testing/evidence/recent-minutes-push-boundaries-20261024.json)。480 是整个当前工作区的结果，不表示新增了 480 条用例，也不计为实际交易时段验收。

## 2. 0feb 非空用例

源码：[recent_minutes_boundaries.rs](../../../crates/rustdx-core/tests/recent_minutes_boundaries.rs)。固定非空样本写在测试源码中，明确标为手写合成字节；不放进标为 `synthetic=false` 的真实行情夹具目录。

| 用例 | 检查内容 |
| --- | --- |
| 4 点固定字节 | 首点价格/均价基值，随后正、负、零差值，零成交量，头部 f32 位模式与完整 raw 正文 |
| 首点基准 | 后续字段相对第一点；避免错误累加至上一点，均价额外除以 100 |
| 分钟会话 | 1、120、121、240 点；09:31、11:30、13:01、15:00 标签及午休跳转；241 点拒绝 |
| 精度 | 同一组整数分别按目录 decimal 2、3、4 换算；缺失/不支持精度报错 |
| 原始整数范围 | i64::MAX 的合法价格/均价/量原值保留；checked 加法溢出、负最终价格/均价/量拒绝 |
| 畸形正文 | 每一个截断位置、超长 varint、NaN/正负无穷、尾随字节，以及空数量带记录拒绝 |
| 日期选择器 | 2024-02-29 的独立 ordinal 常数；非法非闰年日期拒绝 |
| 本地真实 socket 回放 | 压缩响应解压与非空字段、来源连接/地址核对；错消息号或错 opcode 报错且连接不可复用 |

`requested_date` 是请求值，所有结果继续保留 `server_trading_date=None`。非空回放通过不证明服务器返回了请求日期的真实交易数据；合成时间标签也不是报文时间。

## 3. 0547 连续推送用例

源码：[quote_subscription.rs](../../../crates/rustdx-core/tests/quote_subscription.rs)和[command_boundaries.rs](../../../crates/rustdx-core/tests/command_boundaries.rs)。

补充的多证券流先通过普通响应取得一只证券的基线，再由主动帧补齐第二只；随后收到乱序多证券更新和单证券有效零状态。帧包含压缩与非压缩正文，并使用一次 write 合并发送，客户端必须按完整帧读取。

- 只有全部证券均有基线才变为 `Synchronized`。
- 每条返回完整五档和累计量；整条替换，不能把新累计量加到旧累计量。
- 缺席证券保持上次报价、游标和接收时间；有效零价格、零盘口和零量保留。
- 被动接收只执行 `poll_push`，服务端检查没有额外刷新或心跳请求。
- 推送观察器收到正确消息号、连接 ID 和代次；服务端复用基线消息号的推送也可以在被动阶段读取。
- 后续显式 `refresh` 编码已经推进的游标，空响应不重置已同步状态。
- 一页中的后续证券非法或游标回退，整页拒绝；前面的合法证券不能部分覆盖旧状态。
- 重复同游标状态不累计；空推送不修改状态；`u32::MAX` 回绕到 0 必须失效并重建。
- 初始返回游标低于请求游标、坏前缀、损坏压缩体、截断正文、整体取消/截止时间、超量队列和非法预算均覆盖。

本地模拟服务端在需要测试解析失败时保持连接到客户端关闭，避免提前 EOF 干扰预期的解析错误。断线行为另由专门故障用例验证。

## 4. 真实采样结果

### 0feb

[轻量采样工具](../../../crates/rustdx-core/examples/capture_recent_minutes_boundaries.rs)使用 `115.238.56.198:7709`，先通过当日池三轮探测，再从 checked 044d 目录取精度。请求覆盖 2026-10-19 至 2026-10-23 五个交易日、`000001 / 159915 / 510300` 三个证券，合计 **15 次 0feb 响应全部为空**。

所有请求、响应头、压缩正文、解压正文、接收时间、地址和 DTO 保存在[轻量采样目录](../../testing/evidence/command-boundaries-20261024/recent-lite-115.238.56.198)。[独立核验](../../testing/evidence/command-boundaries-20261024/recent-lite-115.238.56.198/independent-verification.json)通过传输/请求/空头核对，但 `live_nonempty_verified=false`。采样及核验工具均以退出码 **2** 表示非空未验收，不用空数据冒充成功。

工具默认只查最近 5 个交易日、3 个证券，180 秒总时限，查询间隔 200ms，最多 128 帧 / 16MiB 原始与解压材料。也可指定单一日期。捕获非空后仍须独立重解全部原始整数、f32 位模式和换算字段，不能仅凭 Rust DTO 生成自己的期望。

### 0547

[盘中采样工具](../../../crates/rustdx-core/examples/validate_quote_push.rs)在北京时间 2026-10-24 周六 14:11 执行严格门禁，得到 `market_closed`、退出码 **2**，未建立业务订阅或读取推送。[门禁留证](../../testing/evidence/command-boundaries-20261024/push-session-gate/summary.json)包含日历版本/覆盖区间、休市日期和下一交易日建议开始时间；[离线复核](../../testing/evidence/command-boundaries-20261024/push-session-gate/independent-verification.json)的 `live_trading_push_verified=false`。

盘中通过条件同时要求：

1. 内置日历覆盖且为交易日，完整观察窗口位于同一个连续交易时段；午休、收盘、周末、节假日和跨窗口拒绝。
2. 全部订阅证券的基线完整，连接 ID / 代次保持不变。
3. 初始请求后只被动读；观察期请求数为 0，自动心跳关闭，自动重连为 0。
4. 等待最初 5 秒后，至少收到两帧带严格推进游标和已知价格/量/额/盘口变化的推送。
5. 完成配置的 30–600 秒窗口，帧/解压/观察器/队列均未失败。

游标单独变化、相同状态重复、未知尾部变化、普通配对响应和 `--allow-closed` 诊断均不满足验收。没有收到推送也不证明服务器永远不支持推送。

## 5. 复现与后续真实验收

以下命令从仓库根执行，使用当前源码；本机 Rust 位于 `.cargo` 时按工程文档设置 `CARGO_HOME` / `RUSTUP_HOME`。采样输出目录必须尚不存在，避免覆盖原始证据。

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test \
  --test recent_minutes_boundaries --test quote_subscription \
  --test command_boundaries --example validate_quote_push --offline --locked
python3 scripts/validation/verify_recent_minutes_boundaries.py --self-test
python3 scripts/validation/verify_quote_push.py --self-test

# 非空采样：默认最近五个交易日，或在地址后追加 YYYYMMDD。
cargo +1.89.0 run --example capture_recent_minutes_boundaries -- \
  /path/to/new-recent-capture 115.238.56.198:7709
python3 scripts/validation/verify_recent_minutes_boundaries.py \
  /path/to/new-recent-capture --freeze

# 建议在下一交易日 2026-10-26 09:35 或 13:05 开始；不预约自动任务。
cargo +1.89.0 run --example validate_quote_push -- \
  /path/to/new-push-capture 115.238.56.198:7709 --seconds 180
python3 scripts/validation/verify_quote_push.py /path/to/new-push-capture --freeze
```

`--freeze` 只首次创建摘要；再次核验要求所有原始文件 SHA256 不变。0feb 捕获程序的 0 表示已取到所测全部证券的非空响应，仍需核验脚本返回 0 才能认定本组非空样本通过。0547 捕获程序的 0 表示严格盘中条件满足，再独立复核帧和请求轨迹。

源码边界覆盖已经补齐。还需取得真实 0feb 非空样本和真实盘中 0547 主动帧才能关闭这两项实盘缺口；本轮没有改写为已验收，也未提交、推送或发布版本。
