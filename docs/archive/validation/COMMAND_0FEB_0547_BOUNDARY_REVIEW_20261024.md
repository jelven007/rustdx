# 0x0feb / 0x0547 边界检查与修复报告

日期：2026-10-24。基于当前工作区的 Rust 2024 / Rust 1.89 实现。

**结论：发现并修复3个解码/状态边界缺陷，另补齐1个订阅超时参数校验。** 新增15项回归；四组专项测试46项通过，工作区测试框架合计480项通过、0失败、24项忽略。格式、workspace全targets/allfeatures Clippy拒绝警告、严格rustdoc均通过。

本报告补充[九命令完整实施记录](../development/NINE_COMMANDS_IMPLEMENTATION_20261024.md)，不改变22项命令的覆盖统计。代码边界检查与真实行情业务验收分开记录；本地构造非空响应及推送不记作实盘样本。

## 1. 发现的问题与修复

| 问题 | 触发条件与原行为 | 修复后的行为 | 源码 |
| --- | --- | --- | --- |
| 0feb昨收/今开缺少非负校验 | 10字节空头或非空响应中，昨收=-1或今开=-1仍返回成功；此前只拒绝NaN/Inf | 读取两个f32后拒绝负值，返回InvalidData；0有效，保留浮点位模式 | [recent_minutes.rs](../../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)，85–87行 |
| 0547首次基线可能低于请求游标 | `RefreshCursor.cursor=10`，首次响应cursor=9；旧merge仅在已有quote时检查回退，离线解码亦未检查请求游标 | 解码逐证券验证返回cursor≥请求cursor；merge另外无条件验证当前状态游标。异常整页不发布，订阅失效并释放连接 | [quote_refresh.rs](../../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs)，77–90行；[quote_subscription.rs](../../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs)，412行起 |
| 合法多证券响应耗尽分区搜索预算 | 请求数合法、每条核心完整，搜索仍枚举不可能凑齐声明数量的分支，最终报“报价边界解析预算耗尽” | 当剩余候选数不足以构成剩余记录时剪枝；合法1/12/32/100条0547及80条054c响应通过，歧义仍拒绝 | [extended_quote.rs](../../../crates/rustdx-core/src/tcp/stock/extended_quote.rs)，249–271行 |
| 极端订阅超时没有在构造时拒绝 | `SubscriptionOptions.refresh_timeout=Duration::MAX`满足非零条件，却无法形成Instant deadline，错误延迟到refresh | options.validate提前检查Instant.checked_add；非法参数在创建连接前返回InvalidInput | [quote_subscription.rs](../../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs)，36–49行 |

首次新增7项测试时，三个代码缺陷均复现。另一个失败来自测试误把六位ASCII字母数字代码当作“必须全数字”：已有公共输入合同允许字母，已纠正测试，并修正实施报告的文字。`00000a`通过格式校验不证明主站存在该证券；标点、非ASCII、长度不等于6及市场2仍拒绝。

同时处理采样示例中的格式和两个不必要的字符串分配，以使全workspace格式/Clippy检查通过；没有增加运行时依赖。

## 2. 0feb 当前边界合同

入口：[RecentMinutesRequest / decode_recent_minutes](../../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)、[Client::recent_minutes / recent_minutes_batch](../../../crates/rustdx-core/src/tcp/stock/client/extended_queries.rs)。

| 边界 | 当前规则与本次验证 |
| --- | --- |
| 市场和代码 | 请求市场仅0/1；六位ASCII字母数字代码。空值、五位、标点、全角字符、市场2拒绝 |
| 日期 | 八位YYYYMMDD，1000-01-01至9999-12-31，且必须是真实日历日期；闰日和两端日期通过，1900-02-29、无效月/日、u32::MAX拒绝 |
| selector | `0xfed62304 - num_days_from_ce()`使用checked_sub；离线检查闰日和边界日期的编码。有效日期不表示处于服务端保存窗口 |
| 正文 | 解压后≤65535字节；至少完整读取数量u16及昨收/今开两个f32。0条对应完整10字节头，缺头或额外正文拒绝 |
| 点数 | 0..240；同时检查正文至少能承载每点3个最短变长整数。241和65535条声明拒绝 |
| 昨收/今开 | 有限且非负；NaN、正负Inf、负数拒绝。零值保留，不根据零值擅自认定停牌或数据缺失 |
| 每点字段 | price/average字段和volume用有符号i64变长整数。截断、超长、整数溢出拒绝；原始负差值可以合法 |
| 还原价格 | 首点确定基值，后续差值始终相对首点；checked_add后，价格/均价/数量必须非负。首点i64::MAX可保留，后续+1拒绝 |
| 精度 | 使用明确目录中的2/3/4位精度，价格除divisor、均价再除100；缺精度NotFound，不支持精度Unsupported |
| 时间标签 | 0/119/120/239号点分别为09:31/11:30/13:01/15:00，午间分界正确；标签是客户端合成值 |
| 日期语义 | `requested_date`只是请求日期；响应没有回显真实交易日，因此`server_trading_date=None` |
| 网络入口 | recv在发包前后检查精度目录时效；外层帧必须匹配opcode/消息号。已有本地压缩响应回放覆盖正常及错配路径 |

离线`decode_recent_minutes`允许使用调用者明确提供的历史精度快照，便于保存报文重解；网络`recv`的当日时效检查是独立约束，不能据离线解码成功认定目录当前有效。

## 3. 0547 当前边界与状态合同

入口：[RefreshRequest / decode_quote_refresh](../../../crates/rustdx-core/src/tcp/stock/quote_refresh.rs)、[QuoteSubscription](../../../crates/rustdx-core/src/tcp/stock/quote_subscription.rs)。

| 边界 | 当前规则与验证 |
| --- | --- |
| 请求数 | 1..100只；空列表、101只及重复证券拒绝。市场/代码沿用公共checked合同 |
| 游标 | u32原值；0建立初始基线；等于请求值有效，u32::MAX可以解码；小于请求或当前状态值拒绝，需rebuild |
| 响应数 | 不得超过请求数或100；空更新允许，空更新带额外正文拒绝；缺席证券保留旧状态 |
| 解码层次 | 外层按帧头解压，再逐字节XOR 0x93；缺失数量头、错证券、重复记录、核心截断拒绝 |
| 记录分区 | 候选≤512、搜索≤2048步。新增剪枝不放宽预算；512候选可通过单记录合法尾部，513候选拒绝 |
| 五档/数值 | 必须完整五档；价格和量非负；0价格、0量、0游标有效；差值按checked i64加法计算 |
| 未知尾部 | 按结构分区保留tail_raw；尾部中的裸代码标记不自动成为新记录；多个合法分区时拒绝。未知尾部业务含义不因此得到证明 |
| 队列预算 | 1..256帧、1..8MiB，可缩小；排队推送与当前帧检查字节预算。预算超限报错、清队列并关闭，不静默丢帧 |
| 超时参数 | refresh_timeout必须为正且能构造Instant deadline；Duration::ZERO / MAX提前拒绝 |
| poll时限 | 本地poll超时返回None，保留半头/半体和有效会话；OperationContext总deadline、取消或撤销则Err并关闭 |
| 刷新/推送顺序 | refresh配对当前消息号；前置推送先入队并合并，返回响应仍不得覆盖更高游标。取出旧通知不再次合并 |
| 整页发布 | 全部记录合法后才修改状态；第一只更新合法、第二只游标回退时，两只均保持原cursor/quote |
| 基线状态 | 全部订阅证券收到完整quote才Synchronized；空首帧或部分证券基线保持AwaitingBaseline |
| 重建/关闭 | rebuild先关闭旧连接，递增代次并清空游标/基线，再通过当日池重建；失败保持Invalid。close/Drop释放许可 |

队列字节核算是当前实现的保守资源预算，包含报文与owned记录等开销，不等同于整个进程或会话RSS上界。缩小至1字节虽通过参数范围校验，但无法容纳帧头，将在读帧阶段明确失效。

## 4. 回归覆盖与工程结果

本次新增[command_boundaries.rs](../../../crates/rustdx-core/tests/command_boundaries.rs)的10项测试，并向[quote_subscription.rs](../../../crates/rustdx-core/tests/quote_subscription.rs)增加5项测试。当前工作区同时包含其他分时/被动推送验证更新；下表报告实际执行的测试集合，不把全部增量归属于本次修复。

| 测试集合 | 结果 | 重点 |
| --- | --- | --- |
| command_boundaries | 10通过 | 非法头价格、0/240/241点、日期、截断/溢出、100条响应、初始游标、候选预算、零盘口、80条共享快照分区 |
| quote_subscription | 21通过 | 首帧回退、响应/推送整页拒绝、本地/总超时、畸形帧、参数预算；并覆盖既有半帧、排队、被动推送、重建与许可释放 |
| protocol_7709_decoders | 11通过 | 9命令请求、真实保存夹具全部owned记录、XOR/盘口、随机畸形输入及其他命令兼容 |
| extended_queries | 4通过 | 批量顺序/逐项错误、分页预算、重复页、同址重连拒绝 |
| 上述四组专项合计 | **46通过、0失败** | 全部为离线解码或本地TCP测试；保存的真实夹具来源另有原始证据 |
| workspace/allfeatures | **480通过、0失败、24忽略** | 包含相关recent_minutes_boundaries、其他库/CLI/runner/集成与文档回归 |
| fmt --all --check | 通过 | 最终退出0 |
| Clippy workspace/alltargets/allfeatures -D warnings | 通过 | 首次发现采样示例两个unnecessary_to_owned警告，修复后复检退出0 |
| 严格rustdoc | 通过 | `RUSTDOCFLAGS='-D warnings'`，最终退出0 |
| git diff --check | 通过 | 空白检查 |

workspace测试显式设置`RUSTDX_SKIP_INTEGRATION_TESTS=1`，并使用offline/locked。某些外部测试按既有开关提前返回并被框架计为ok；480是测试框架统计，不是480次真实行情/数据库验收。

最终命令、退出码、测试统计及源码SHA-256见[机器检查证据](../../testing/evidence/command-0feb-0547-boundaries-20261024.json)。中间日志在`/tmp/rustdx-0feb-0547-checks-20261024`，是本机临时材料；该JSON保留摘要和本机路径，不承诺临时日志长期存在。

## 5. 尚未证明的业务范围

- **0feb非空实盘业务**：原九命令证据中，两个主站、股票/ETF、20261022/23/24共12次均返回空序列。本次非空构造、全240点及压缩回放证明代码边界和计算合同，不能证明主站非空业务、保存窗口、真实交易日或成交量单位。
- **0547交易时段主动推送**：原真实采样证明三证券完整基线及随后空刷新；周六短poll没有观察到主动推送。本地被动推送/前置推送/重建测试证明实现路径，不能证明盘中连续推送周期、长期稳定性或交易所数据完整性。
- **更多品种与服务端尾部**：支持目录精度及原始字段保留不表示未知尾部、其他市场或所有参数的业务语义已核实。

代码实现与本次边界修复已完成，上述业务证据缺口继续保留。原始实盘证据见[九命令证据目录](../../testing/evidence/nine-commands-20261024)和[原实施机器记录](../../testing/evidence/nine-commands-implementation-20261024.json)。

## 6. 复现

使用项目Rust 1.89工具链及已安装离线依赖：

```sh
cargo +1.89.0 test --all-features --offline --locked \
  --test command_boundaries --test quote_subscription \
  --test protocol_7709_decoders --test extended_queries
cargo +1.89.0 fmt --all --check
cargo +1.89.0 clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --offline --locked
RUSTDOCFLAGS='-D warnings' cargo +1.89.0 doc --workspace --all-features --no-deps --offline --locked
```

本机工具链位于仓库`.cargo/bin`，检查时使用仓库`.cargo`作为CARGO_HOME、`.cargo/rustup`作为RUSTUP_HOME；具体环境记录在机器证据中。
