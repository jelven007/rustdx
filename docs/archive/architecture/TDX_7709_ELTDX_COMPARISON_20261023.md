# 通达信7709与eltdx社区实现对照

资料截至2026-10-23；社区基线为摘要固定的eltdx 3.3.0发布包，项目对照基线为rustdx `408e30e`。[来源4](#cite-4) [来源12](#cite-12)

2026-10-24补充：[K线成交量专项验证](../validation/KLINE_VOLUME_UNITS_20261024.md)已完成指定股票/ETF的日线`9/1`、`4/1`及5分钟`0/1`真实交叉采样。样本单位随周期编号变化；同周期38/54字节响应正文一致。复权、其他周期/品种和指数分钟量仍待验证，原10月23日静态证据及来源摘要保持原样。[14](#cite-14)

**eltdx是通达信7709服务端协议的第三方客户端实现。官方协议决定服务器接受什么请求、返回什么字节；eltdx实现其中一部分请求编码、响应解码及业务封装。当前公开官方资料缺少完整7709逐字节规范，因此可以确认“eltdx如何实现”和“它与既有客户端有何差异”，无法确认“它与官方完整规范逐条一致”。**

本报告分别使用三种证据口径：**官方文档确认**用于官方公开API；**源码确认**用于具体版本的编码和解码；**待线上验证**用于服务器覆盖、字段业务口径及实际兼容。rustdx和pytdx均属于客户端实现证据，不被当作厂商正式规范。

[本轮对照证据](../../testing/evidence/tdx-7709-eltdx-comparison-20261023.json)记录源码摘要及静态核对；[前轮研究](TDX_PROTOCOL_RESEARCH_20261023.md)保存版本时间线、全部命令及原始资料。

## 1. 官方可确认信息与社区实现的范围

| 维度 | 通达信官方可确认信息 | eltdx 3.3.0可确认实现 | 比较结论 |
| --- | --- | --- | --- |
| 定位 | 厂商数据服务及公开高层API | 社区客户端，Python入口＋Rust协议/运行核心 | 协议提供者与协议使用者属于不同层次 |
| 7709完整字节规范 | 本次可访问公开材料中未取得 | 可阅读帧、命令、参数及解码源码 | 源码可核查，不等于官方认证 |
| 命令完整数量 | N/A，无法确认整个服务端命令集合 | 注册26项，包含握手和心跳 [4](#cite-4) | 26是该SDK的实现清单，不是官方协议总数 |
| 最新版本标签 | 官方TQ/tdxaidata 1.3.1；官网注明仅内测版支持 [1](#cite-1) | 社区3.3.0发布于10月21日 [4](#cite-4) | 两组版本不能互相对应成7709版本号 |
| 本地HTTP | 官方公开`127.0.0.1:17709`调用方式 [2](#cite-2) | eltdx另有自己的产品封装；7709核心为二进制TCP | 17709 HTTP不属于本报告的7709帧 |
| 后台认证 | 官方tdxaidata经原生库访问后台，需要数据服务Key [2](#cite-2) | 7709实现使用连接握手；本次未取得它与官方Key机制的对应关系 [5](#cite-5) | 不能据社区握手推断官方SDK内部认证或权限 |
| 单位与字段 | 官方`get_market_data`公开量、额、复权及填充规则 [3](#cite-3) | 具体命令有自己的源字段及转换 [6](#cite-6) [8](#cite-8) | 必须按接口核实，不能跨API直接继承单位 |
| 覆盖和稳定性 | 本次没有取得7709完整市场覆盖及兼容性承诺 | 取决服务器、品种和作者已实现的布局 | 不能仅凭命令数量计算市场覆盖率 |

官方1.3.1新增综合信息、扩展市场统计和公式能力的事实，并不能说明7709在同一天增加了对应opcode。类似地，eltdx补齐某个命令，只证明该SDK新接入了它，不能确定该命令在服务端的首次出现日期。

## 2. eltdx保留7709通信框架，并增加客户端处理

### 2.1 帧布局

eltdx重建的请求头为12字节、响应头为16字节；本库当前请求及响应读取亦使用这两个长度。以下位置从0计数，数值为小端，属于客户端源码证据。[5](#cite-5) [12](#cite-12)

| 帧 | 位置 | eltdx中的解释 |
| --- | --- | --- |
| 请求 | 0 | 前缀`0x0C` |
| 请求 | 1–4、5 | `msg_id: u32`、`control: u8` |
| 请求 | 6–7、8–9 | 两份相同u16长度；值为请求data长度＋2 |
| 请求 | 10–11、12以后 | 命令码u16、请求data |
| 响应 | 0–3 | 前缀`B1 CB 74 00` |
| 响应 | 4、5–8、9 | control、msg_id、保留字节 |
| 响应 | 10–11 | 命令码u16 |
| 响应 | 12–13、14–15 | 压缩正文长度、解压正文长度 |
| 响应 | 16以后 | 正文 |

eltdx的帧解码显式检查前缀、重复长度和完整帧长度，运行层再将响应与消息号、命令码及连接代次配对，并管理未配对推送。本库通用TCP读框以单次请求/响应为主，读满头后按长度读取正文，checked业务解码器再检查身份及业务数据。[5](#cite-5) [11](#cite-11) [12](#cite-12)

这些是客户端校验与调度设计差异。动态消息号、推送队列和连接代次不会自动构成一种新的服务端协议。

### 2.2 握手与保活

两边使用`0x000D`、请求data为`01`的初始握手。本库用固定`PACK1`头，eltdx通过通用帧构造消息号；eltdx还解出服务端日期、交易时段、名称等信息，并保留未解释字节。本库旧`PACK2/PACK3`不进入当前正常连接路径。[5](#cite-5) [12](#cite-12)

保活方式存在具体差异：eltdx登记`0x0004`空data心跳，本库heartbeat复用`0x044E`证券数量请求。这说明两个客户端选用了不同操作，不能据此断言官方只允许其中一种。[5](#cite-5) [12](#cite-12)

## 3. 与现有7709客户端的具体报文差异

本节左列明确是**rustdx当前实现**，用于提供可复核的旧报文基线；它不是“官方7709规范”。

| 业务 | rustdx当前实现 | eltdx 3.3.0 | 已确认差异及影响 |
| --- | --- | --- | --- |
| 证券目录 | `0x0450`；请求16字节，start为u16；记录29字节 | `0x044D`；请求26字节，start为u32；记录37字节 [7](#cite-7) [12](#cite-12) | 命令和布局都不同，不能仅替换opcode |
| 目录名称/昨收 | 名称8字节GBK；昨收用compact-float解码 | 名称16字节GBK；昨收直接f32；登记价格小数位 [7](#cite-7) [12](#cite-12) | 需要新目录类型及不同解码器 |
| K线请求 | `0x052D`；完整38字节；公开category，第二周期参数固定1 | 同码；完整54字节，即12字节头＋42字节data [6](#cite-6) [12](#cite-12) | 同码请求长度和参数表达不同 |
| K线周期 | 标准日线9、1分钟8；checked category为0–11 | 日线`4/1`、1分钟`7/1`；还能表达N分钟`8/N`、N日`9/N`、N秒`13/N` [6](#cite-6) [12](#cite-12) | 周期数字不能直接复用；支持全部证券仍待验证 |
| K线复权 | 请求原价，另取除权事件在本地复权 | 请求传0–4复权模式及锚定日期，作者说明主站返回对应价格 [6](#cite-6) | 服务端返回复权与本地算法需要分开验证 |
| K线价格 | 差分累计后÷1000 | 差分累计后÷1000 [6](#cite-6) [12](#cite-12) | 该价格比例相同；不存在“本库K线按分”的差异 |
| K线成交量 | compact-float解码值直接写入vol | 保留wire解码值，另输出`volume_lots = wire / 100` [6](#cite-6) [12](#cite-12) | 10月24日样本确认：股票`9/1`源量为手，`4/1`及5分钟`0/1`为股；不是报文长度导致，见专项验证 [14](#cite-14) |
| 旧完整快照 | `0x053E`，固定头、代码列表；公开五档业务字段 | `0x053E`保留旧完整快照，增加状态、原始尾部及品种价格处理 [8](#cite-8) [12](#cite-12) | 请求正文结构相同，帧参数及结果类型仍需对照 |
| 新基础快照/刷新 | 当前quotes主路径为`0x053E` | `0x054C`基础快照；`0x0547`游标、五档及推送 [8](#cite-8) | 不同命令、布局和调用状态，解析器不可互换 |
| 当日分时 | `0x051D`，MinuteTimeData仅price/vol | `0x0537`，包含均价、时间及原始字段 [10](#cite-10) [12](#cite-12) | 需要新增请求及字段，不能只给旧结果补时间标签 |
| 历史成交 | `0x0FB5`；公开时分、价格、量、方向等 | `0x0FC6`增强记录；混合成交、竞价状态及盘后事件 [10](#cite-10) | 记录布局和事件口径不同 |
| 集合竞价/资金流/退市日线 | 当前无对应主动TCP入口 | `0x056A/0x0FFC/0x052B` [4](#cite-4) | 属于本库缺少的能力，不能当作官方最近才新增 |

### 3.1 旧快照的相同点与差异

两边`0x053E`请求data均为：8字节前缀`05 00 00 00 00 00 00 00`＋u16数量＋每只证券7字节市场/代码。完整请求长度均为`22 + 7 × 证券数`。本次用仓库既有的两证券真实请求核对了这一正文结构，原采样时间为2026-10-22 23:31:59 UTC；没有执行eltdx解码器或重新采样。[8](#cite-8) [9](#cite-9)

eltdx通用构包默认control为1，本库该快照固定头control为2；消息号处理亦不同。控制字段的完整厂商语义未取得，因此只能记录编码差异，不能自行解释为新版认证或新的会话权限。[5](#cite-5) [8](#cite-8) [12](#cite-12)

eltdx旧快照的价格比例为`100 × price_divisor(code)`；`price_divisor`优先用目录小数位，缺失时对部分ETF前缀使用回退。本库当前快照按100还原价格，且未保留目录驱动的这一处理。[8](#cite-8) [12](#cite-12)

这对ETF、债券等多小数品种的验证有实际意义，但不能据源码前缀规则声称已经准确覆盖全部品种。新快照和刷新命令使用各自解码流程，不应套用上述旧快照公式。

### 3.2 命令数量与市场覆盖

eltdx登记26项，本库14个业务命令＋1个活跃握手；按opcode比较，同码12项、eltdx另有14项本库未接入，本库另有`0x0450/0x051D/0x0FB5`三项未列入eltdx清单。[4](#cite-4) [12](#cite-12)

这只是实现集合差异。缺失某个opcode不代表对应旧码失效；登记一个opcode也不证明该命令覆盖沪深京全部证券、所有历史日期和服务器。eltdx市场枚举包括0深圳、1上海、2北京；本库普通TCP仅支持0/1。市场枚举支持和每个接口实际覆盖须分开看。[6](#cite-6) [7](#cite-7) [12](#cite-12)

## 4. 官方API字段与社区DTO的差异

### 4.1 单位和返回合同

| 字段/规则 | 官方`get_market_data`公开合同 | eltdx普通7709 K线 | 接入时需要处理 |
| --- | --- | --- | --- |
| OHLC | 元 | 差分还原后展示价格，源码为毫厘价÷1000 [3](#cite-3) [6](#cite-6) | 表面单位相同，仍需明确品种、周期与复权基准 |
| 成交量 | 最小单位，股或张 | 原始量、wire解码值及另算的手数 [3](#cite-3) [6](#cite-6) | 按具体品种确定单位，不能全市场统一按100换算 |
| 成交额 | 万元 | 社区说明及源码输出解码后的amount；常见股票样本按元解释 [3](#cite-3) [6](#cite-6) | 官方API转元需乘10000；不能把这条规则应用到7709原始字节 |
| 复权 | `dividend_type`与`ForwardFactor` | 复权请求参数或本地除权因子封装 | 记录复权方式、锚点及因子，价格序列不能混用 |
| 缺失数据 | `fill_data=True`默认可向后填充 [3](#cite-3) | 按命令返回及上层分页封装 | 原始数据与填充结果需要明确区分 |
| 数据组织 | 高层字段表、字符串及DataFrame等 | 协议模型、raw字节及Python业务模型 | 这是API/DTO差异，不能称为官方7709响应格式差异 |

官方高层API的单位是厂商明确说明；eltdx普通7709字段是社区解析和业务解释，两者证据层次不同。不能用高层API的成交额单位反推底层TCP字段编码。

### 4.2 线上字段、派生字段与保留字段

| eltdx字段 | 字段性质 | 业务含义的边界 |
| --- | --- | --- |
| `volume_raw/amount_raw` | 从响应取出的编码值 | 原始编码不等于已确定的业务单位 |
| `volume_lots` | wire量÷100的派生值 [6](#cite-6) | 换算比例是实现选择，应用范围需核实 |
| `event_kind/is_actual_trade` | 依据status和时间分类 | 字符串事件名并非服务器原生字段 |
| `status_raw=8/5` | 源状态码＋社区业务解释 [10](#cite-10) | 作者分别按竞价快照/盘后成交处理；缺少厂商完整枚举证明 |
| `SecurityCode.category/board` | 根据市场、代码前缀派生 | 不是服务端提供的正式品种分类 |
| `full_code` | 市场与六位代码组合 [7](#cite-7) | 有助保持证券身份，格式本身属于客户端 |
| `bar_time` | 时间标签选择 | 不是新增交易发生时间字段，也不保证新采样 |
| `unknown*_raw/tail_raw` | 未解释字段原样保留 | 保留字节便于研究，不应给未知字段编造业务含义 |

`status=8`按作者规则不进入实际成交集合，而`status=5`的盘后成交可进入；盲目对全部增强历史记录求和会混入竞价快照。社区规则可作为待验证假设，不能替代交易所或厂商明确的业务枚举。[10](#cite-10)

## 5. 实现与采用边界

eltdx把协议核心、Tokio运行层和Python/PyO3边界分开，提供连接池、消息配对、推送缓冲及原始字段保留。本库保持同步I/O及有界线程，checked请求侧重不可变参数、响应身份和数值/尾部检查。线程模型、连接池和批量API均是客户端设计，不是7709服务端差异；本次没有建立两者性能对比数据。[11](#cite-11) [12](#cite-12)

eltdx架构文档中仍有旧的22/23命令描述，当前注册代码及命令表为26。本报告以固定版本注册代码为计数依据，不将旧文档文字混入最新清单。[4](#cite-4) [11](#cite-11)

eltdx实际LICENSE是Non-Commercial Research License，禁止商业或生产服务使用。它适合作为本次个人协议研究资料；直接引入库或复制其实现进入生产代码，需要先解决这项许可限制。厂商数据服务的使用条件与客户端代码许可另行成立。[13](#cite-13)

对rustdx，优先验证三个具体问题：

1. **K线成交量与复权**：成交量部分已于10月24日完成限定范围验证：默认日线rustdx请求`9/1`直接输出手，eltdx请求`4/1`返回股后除100输出手；本库5分钟`0/1`样本为股，不能统一套用日线换算。指数分钟量、其他周期/品种/服务器及复权继续待验证。[14](#cite-14)
2. **目录及多小数品种报价**：独立验证`0x044D`记录、小数位和ETF/债券价格比例；新目录不能复用旧29字节解码器。[7](#cite-7)
3. **新增行情及事件接口**：分别验证基础快照、五档刷新、增强历史成交和竞价过程，记录推送丢失、混合事件、分页终止及实际服务器覆盖。

10月23日对照轮完成的是源码、既有真实请求及资料摘要对照，当时未新增7709线上探针。10月24日另行补充24组K线真实请求、官方`.day`对照及双方数值公式重放；没有执行完整第三方SDK或修改Rust源码。限定样本的单位验证不等于全协议线上兼容或生产验收。

## 6. 来源与复核入口

<a id="cite-1"></a> **[1] 官方版本说明，高可信，2026-10-23快照。** [官网](https://help.tdx.com.cn/quant/docs/markdown/mindoc-1cfsjkbf8f3is/TdxQuantVersion.html) · [固定原文](../../testing/evidence/tdx-protocol-20261023/official/official-versions.html)。

<a id="cite-2"></a> **[2] 官方HTTP及后台接口，高可信，高层API范围。** [HTTP官网](https://help.tdx.com.cn/quant/docs/markdown/mindoc-1hdhbmi50d038.html) · [后台官网](https://help.tdx.com.cn/quant/docs/markdown/mindoc-1hjbgqpdhv114.html) · [固定HTTP原文](../../testing/evidence/tdx-protocol-20261023/official/official-http17709.html)。

<a id="cite-3"></a> **[3] 官方K线单位及返回规则，高可信，高层API范围。** [官网](https://help.tdx.com.cn/quant/docs/markdown/mindoc-1ctuhthaq5qmg/mindoc-1h10g60jt68sc.html) · [固定原文](../../testing/evidence/tdx-protocol-20261023/official/official-market-data.html)。

<a id="cite-4"></a> **[4] eltdx固定发布包，源码事实可复核，服务端语义为社区来源。** [发布记录](https://github.com/electkismet/eltdx/releases/tag/v3.3.0) · [注册代码](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/request.rs) · [命令清单](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/COMMANDS_7709.md)。

<a id="cite-5"></a> **[5] eltdx帧及会话实现，静态源码证据。** [frame.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/frame.rs) · [session.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/session.rs)。

<a id="cite-6"></a> **[6] eltdx K线、周期及换算，源码事实/社区业务解释。** [klines.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/klines.rs) · [unit.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/unit.rs) · [K线说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-K线周期线.md)。

<a id="cite-7"></a> **[7] eltdx证券目录，静态源码证据。** [security.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/security.rs) · [代码表说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-代码表.md)。

<a id="cite-8"></a> **[8] eltdx快照、刷新及价格处理，静态源码证据。** [quotes.rs](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/quotes.rs) · [三个快照命令边界](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/COMMANDS_7709.md)。

<a id="cite-9"></a> **[9] 仓库既有真实请求，局部样本证据。** [请求hex](../../../crates/rustdx-core/tests/fixtures/protocol/quotes.request.hex) · [采样身份、时间及pytdx参考](../../../crates/rustdx-core/tests/fixtures/protocol/manifest.json)。只用于正文结构核对，未扩充本轮线上验收范围。

<a id="cite-10"></a> **[10] eltdx分时、成交及事件解释，社区来源，业务枚举待独立确认。** [分时说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-当日分时.md) · [历史成交说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-历史成交明细.md)。

<a id="cite-11"></a> **[11] eltdx运行架构，作者文档及本轮源码核对，非独立性能测量。** [架构文档](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/ARCHITECTURE.md)。

<a id="cite-12"></a> **[12] rustdx当前源码，项目事实，非官方协议规范。** [TCP读框](../../../crates/rustdx-core/src/tcp/mod.rs) · [握手/保活](../../../crates/rustdx-core/src/tcp/basic.rs) · [证券目录](../../../crates/rustdx-core/src/tcp/stock/security_list.rs) · [K线](../../../crates/rustdx-core/src/tcp/stock/kline_request.rs) · [快照](../../../crates/rustdx-core/src/tcp/stock/quotes.rs) · [checked请求](../../../crates/rustdx-core/src/tcp/stock/requests.rs) · [当前接口字典](../operations/UPSTREAM_ACTIVE_INTERFACES.md)。

<a id="cite-13"></a> **[13] eltdx实际代码许可，原始法律文本。** [LICENSE](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/LICENSE)。

<a id="cite-14"></a> **[14] 2026-10-24成交量专项，指定服务器/证券/周期的真实样本证据。** [报告](../validation/KLINE_VOLUME_UNITS_20261024.md) · [机器可读结论](../../testing/evidence/kline-volume-units-20261024.json) · [原始帧和重放入口](../../testing/evidence/kline-volume-20261024/README.md)。同供应商`.day`与7709的不同表示交叉验证，非交易所独立认证。
