# rustdx-complete 当前发布版评估

评估日期：2026-10-23。对象：[crates.io/rustdx-complete](https://crates.io/crates/rustdx-complete) 的当前非撤回版本 **1.11.0**，以及单独注明的配套 `rustdx-cli 1.7.2`。对照仓库：`jelven-rustdx-complete 1.12.2` / `jelven-rustdx-cli 1.8.2`，源码基线 `5d3f961e78a711014f19b2625a603a909f69bcc7`。

## 1. 评估结论

`rustdx-complete 1.11.0` 提供了较完整的通达信行情、文件解析和计算功能，适合作为协议实现与功能复用基础。默认特性在 Rust 1.89 上可以编译；同步 I/O、模块化结构和较小的直接依赖范围，也便于嵌入批量数据程序。[cite:1](#cite-1)[cite:2](#cite-2)

当前发布版仍有影响生产正确性的缺陷，包括截断数据被当成有效结果、连续性检查遗漏缺口和重复、日期规则过时、缓存键冲突，以及连接池和完整报文重试的实现风险。配套 CLI 另有吞掉文件错误、提前覆盖输出、数据库完成判定不足的问题。未经加固和目标环境验收，不宜直接承担生产数据发布。[cite:4](#cite-4)[cite:6](#cite-6)

本仓库已经包含该发布版的共同历史，并完成多轮强化。融合应以本仓库现有实现为主，保留上游协议能力，继续补齐生产调度、发布治理和运维验收；重复引入原包会增加两套类型、规则与错误语义的维护成本。具体设计见[生产融合方案](PRODUCTION_FUSION_PLAN.md)。

## 2. 版本身份与研究范围

### 2.1 版本核实

| 项目 | 核实结果 | 依据 |
| --- | --- | --- |
| 原核心包 | `rustdx-complete 1.11.0`，2026-09-10 04:19:42 UTC 发布，未撤回 | [cite:1](#cite-1) |
| 原核心源码 | 发布包的38份源码与 Git `48f4f112734c96ac2c021a69fbe54110f06dc108` 一致 | 发布包、`.cargo_vcs_info.json`、逐文件比较 |
| 核心包身份摘要 | SHA-256：`c5abe90b81bd1f07e832de8144b63ff6e20e89928a1050fcb88dcdaf2f4d61b7` | 实际下载包与注册表校验和一致，[cite:2](#cite-2) |
| 原配套 CLI | `rustdx-cli 1.7.2`，2026-09-11 00:25:05 UTC 发布 | [cite:5](#cite-5) |
| CLI源码 | 发布包与 Git `a6deec180800c766c4d58d4cd2968da4f36c17cb` 一致 | [cite:6](#cite-6) |
| 编译与许可证声明 | Rust 2024、MIT；原包未声明 `rust-version`，核心有7项直接常规依赖 | 两份 Cargo manifest，[cite:2](#cite-2) |
| 当前上游主分支 | 核实时最新提交为上述 CLI 提交；核心源码自1.11.0发布提交后未变 | 固定源码及主分支历史，[cite:7](#cite-7) |
| 本仓库关系 | 上述两个提交均为本仓库基线的祖先；原38份源码中35份已经修改、3份相同 | 本地 Git 祖先关系与逐文件比较 |

抓取 crates.io HTML 时曾返回带1.0.0的旧页面；实时 API、固定版本 docs.rs 和实际发布包均指向1.11.0。因此版本判断采用 API 与不可变发布包，旧 HTML 不作为当前版本证据。三个载体用于相互核对身份，不能算三项独立质量证据。

`jelven-rustdx-complete` 是不同注册表包名。本仓库 Cargo 版本仍为1.12.2/1.8.2，但后续强化尚未发布新 crate；不能通过安装这两个旧发布版本取得全部当前源码行为。部署须同时记录 Git 提交、依赖锁文件和二进制摘要，见[版本交付](../release/RELEASES.md)。

### 2.2 证据等级与方法

| 等级 | 含义 | 本次用途 |
| --- | --- | --- |
| 已确认 `[CONFIRMED]` | 实际执行得到的结果，或包/API/Git相互核实的身份事实 | 默认编译、all-features失败、10项差异探针、共同历史 |
| 源码单一证据 `[SINGLE-SOURCE]` | 固定发布源码直接显示的机制，未执行相应故障 | 池槽位错位、正文重试范围、复权事件顺序、CLI导入 |
| 外部规则核实 | 原始规则与生效公告核对；转载不重复计为独立来源 | 2026-07-06起上交所主板风险警示股票涨跌幅 |
| 数据不足 `[INSUFFICIENT DATA]` | 本次没有对应测量或运行证据 | 原包生产吞吐、长期稳定性、全市场一致性、生产SLA |

研究包括：注册表与源码包身份核对；核心 TCP、连接池、解析器、文件、日期规则、缓存、指标审查；配套 CLI 独立审查；Rust1.89离线构建；在临时审计工程同时依赖原包和本仓库，执行固定输入差异探针。持久化摘要见[研究证据JSON](../validation/evidence/upstream-review-20261023.json)。

## 3. 功能与优势

| 优势 | 已有能力 | 对生产设计的价值与边界 |
| --- | --- | --- |
| 行情协议覆盖较广 | 证券名单、快照、股票/指数K线、分时、财务、逐笔、公司信息等模块 | 可以复用协议知识；“完全对标pytdx”仍是项目描述，不代表所有接口已独立验收 |
| 历史文件能力齐备 | day、分钟文件、gbbq及收益/复权相关计算 | 支持离线历史补数与重算，减少仅依赖实时公共服务器的风险 |
| 调用层较方便 | Client、KlineBuilder、区间查询、批次结果、前后复权 | 能形成清晰采集入口；分页、参数编码和失败语义需要加固 |
| 同步架构容易集成 | 标准 TCP、线程、连接池；没有强制异步运行时 | 适合有界批处理，部署依赖较少；同步并不自动等于资源有界 |
| 分析能力集中 | SMA、EMA、MACD、RSI、布林带、KDJ，日历及涨跌停计算 | 可复用计算模块；指标预热、缺口和规则生效日期须形成数据契约 |
| 可选基础缓存 | 内存与文件缓存抽象 | 提供缓存扩展入口；没有自动为所有 Client 查询接入缓存 |
| 代码与授权可维护 | Rust、模块公开、MIT声明、7项直接常规依赖 | 有利于审查、定制和单一分支维护；发布附件与构建门禁仍需完善 |

上述功能以发布包模块与固定源码为依据。[cite:2](#cite-2)[cite:4](#cite-4) 原核心 crate 没有可执行二进制，ZIP获取、导出与写库属于配套 CLI，不能将它们表述为核心包内置能力。

## 4. 主要劣势与生产风险

本节 P0 表示对应数据或发布路径启用时必须处理的正确性风险；P1 表示稳定性、语义和性能风险；P2 表示维护与交付风险。优先级用于本次选型，不替代本仓库需求编号。

### 4.1 P0：输入与输出可信度

| 编号 | 问题及触发条件 | 实际影响 | 证据与建议 |
| --- | --- | --- | --- |
| R01 | K线声明数量大于实际完整记录；day文件包含残余尾部 | 原包仍能返回部分记录，调用方可能把损坏输入当成功 | P01/P05直接确认；使用checked解码、记录数及尾部完整性检查 |
| R02 | gbbq公开入口接受短切片或非法代码字节 | `get_unchecked`、`from_utf8_unchecked`依赖未验证前提；短文件头还会索引越界 | [gbbq L63–73、L160–197](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/file/gbbq/mod.rs#L63-L73)；源码证据，未执行未定义行为；改安全解析 |
| R03 | 连续性校验面对缺失交易日、重复日期 | 使用360天年/30天月近似与间隔启发式，遗漏真实缺口和重复 | P02确认；按有覆盖范围的真实日历与证券状态校验 |
| R04 | 日期相关涨跌幅使用固定主板ST 5%；涨停价处于半分边界 | 不能同时正确处理历史与2026新规，且可出现1分钱差异 | [limit L35–46、L91–110](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/limit.rs#L35-L46)、P10与原始规则；采用日期上下文及整数分 |
| R05 | 已借出连接仍保存Vec下标，清理先前闲槽后归还 | 下标不再代表原槽，可错误归还或丢失连接，破坏池状态 | [pool L194–250、L378–387](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/pool.rs#L194-L250)；源码证据，未运行竞态复现；改为借出对象持有连接、独立idle队列 |
| R06 | 配套CLI部分文件解析失败，或处理过程中失败 | worker的 `if let Ok` 忽略错误；`File::create`先截断旧输出，无原子发布隔离 | [CLI io L62–120](https://github.com/jackluo2012/rustdx/blob/a6deec180800c766c4d58d4cd2968da4f36c17cb/rustdx-cmd/src/io.rs#L62-L120)；源码证据；汇总质量报告，临时产物校验后替换 |
| R07 | 配套CLI写库成功头返回、正文迟到异常或部分导入 | 没有完整响应与全部逻辑键/数值独立回读，成功信号不足以证明数据正确 | [CLI io L238–308](https://github.com/jackluo2012/rustdx/blob/a6deec180800c766c4d58d4cd2968da4f36c17cb/rustdx-cmd/src/io.rs#L238-L308)；源码证据；区分unknown/acknowledged/verified |

R01–R05属于核心包，R06–R07只属于原配套CLI。[cite:4](#cite-4)[cite:6](#cite-6) P04进一步确认，一字节心跳会无错误地返回0，不能由此判断证券数量确实为0。

上交所2026规则公告明确从2026-07-06施行，主板风险警示股票涨跌幅从5%调整为10%；暂缓条款涉及其他项目，未暂缓该调整。此处核实的是上交所规则，不能仅凭这一来源推导所有市场的全部制度。原包没有日期参数，即使修改单个常量，也不能兼容历史计算。[cite:8](#cite-8)[cite:9](#cite-9)[cite:11](#cite-11)

上海证监局转载的7月6日报道与公告日期一致；该转载仅作交叉核对，不作为另一项独立监管规则。[cite:10](#cite-10)

### 4.2 P1：恢复、资源与行情语义

| 编号 | 缺陷或限制 | 生产影响 | 证据 |
| --- | --- | --- | --- |
| R08 | 重试仅覆盖发送与16字节响应头，正文读取在重试外；`auto_reconnect=1`仅执行首试；解压使用unwrap | 正文中断不能按完整请求恢复，异常压缩流可panic，重试预算语义容易误用 | [tcp L203–238、L303–330](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/tcp/mod.rs#L203-L238)，源码证据 |
| R09 | 建连时持池互斥锁，池满直接失败，批次池使用默认Tcp配置；close只清空槽位 | 慢建连阻塞其他调用，配置不一致，关闭行为不具永久关闭语义 | [pool L194–238、L298–301](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/pool.rs#L194-L238)，源码证据 |
| R10 | 日线分页使用 `saturating_add` 后判断 `start==0`，没有普遍无进展/总时限约束 | 在偏移饱和且端点仍返回满页时，可能重复拉取；日线路径未统一去重 | [client L793–846](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/tcp/stock/client.rs#L793-L846)，源码证据 |
| R11 | 没有整任务deadline、进程总网络预算和共享背压；CLI通道无界，文件缓存/内存缓存缺少容量约束 | 多任务组合时，单池上限不能约束总出口连接，结果积压扩大内存 | TCP/pool/cache及CLI固定源码；过期条目滞留由P07直接确认 |
| R12 | 文件缓存用字符替换生成文件名，直接写最终文件且I/O错误隐藏 | `a/b`与`a_b`串值，异常退出或写入错误难以诊断 | P06与[cache L263、L360附近](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/cache.rs#L263) |
| R13 | 缺少统一证券、周期、源单位、价格口径模型；请求字段公开且能与发送字节不一致 | sh/sz同码、股/手、股票/指数和收益因子容易被错误拼接 | [kline L153–235](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/tcp/stock/kline.rs#L153-L235)，源码证据 |
| R14 | 多个停牌期间除权事件沿用同一昨收，首条行情之前事件也进入累乘 | 复权锚点和连续事件组合可能错误，非法结果以默认数值掩盖 | [client L892–941](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/tcp/stock/client.rs#L892-L941)，本次只作源码审查 |
| R15 | 午休计入交易时间，无日历来源/有效区间元数据；CLI东财日期取本机Local | 采集时点、预期交易日和真实源时点可能错位 | P03、calendar源码和配套eastmoney源码 |
| R16 | 多项窗口指标反复扫描，EMA缺口不恢复，KDJ未预热仍返回0 | 大窗口计算成本增长，NaN污染后续，未成熟结果被当有效信号 | P08/P09；[indicators](https://github.com/jackluo2012/rustdx/blob/48f4f112734c96ac2c021a69fbe54110f06dc108/src/indicators.rs)，正常窗口路径O(n×period) |
| R17 | 原CLI东财使用HTTP与固定JSONP偏移；total一致性在聚合后检查，循环缺少空页/重复页/总时限保护，无原始正文留证 | 接口变化与无进展分页难以恢复，当前快照不能证明历史连续 | [CLI eastmoney L7–84、L121–123](https://github.com/jackluo2012/rustdx/blob/a6deec180800c766c4d58d4cd2968da4f36c17cb/rustdx-cmd/src/eastmoney/mod.rs#L7-L84)，源码证据 |

上述未标探针的行为均为固定源码证据。[cite:4](#cite-4)[cite:6](#cite-6) 不能把理论故障路径写成已发生的线上事故。源单位问题是注释与原始口径缺少统一契约，不代表可以将全部品种统一乘100。

### 4.3 P2：交付与可维护性

| 问题 | 观察与影响 | 建议 |
| --- | --- | --- |
| all-features与文档构建失败 | `docsrs`特性启用nightly属性；stable1.89失败E0554。公开docs.rs构建另因已移除 `doc_auto_cfg` 失败E0557 | 将文档cfg与普通特性分离；独立验收stable全特性与docs.rs环境 |
| 缺少完整构建矩阵 | 固定CI主要为Ubuntu/stable/default与外部skip，缺少明确MSRV、跨平台、fuzz及全特性门禁 | 完整源码检出和实际发布包分别验证 |
| 发布附件缺失 | 核心包排除了LICENSE，配套CLI实际包未包含LICENSE-MIT；metadata仍声明MIT | 补齐许可证与版权来源；此事实本身不等于无法依法使用 |
| 静态分类可信度不足 | 概念映射注明示例；行业映射没有可追溯字典版本与核实日期 | 保留原编码，使用有版本的权威映射；没有独立字典时返回未知 |
| 宣传与证据口径不同 | README“完全对标”、测试数量和性能描述属于项目声明 | 用固定请求/响应、独立期望和实际环境记录验收，避免据此计算质量分 |

构建错误见实际命令与docs.rs公开日志。[cite:3](#cite-3) 其他项依据发布包和固定CI/源码。[cite:2](#cite-2)[cite:4](#cite-4)[cite:6](#cite-6) 本次没有进行完整依赖漏洞或所有平台供应链审计。

## 5. 直接执行的10项差异探针

执行环境为macOS/arm64、Rust1.89.0。K线来自仓库实际TDX响应夹具，原始3条日期为2026-10-20/21/22；day来自真实文件夹具。截断、删除、重复和NaN是显式边界/故障输入，缓存和舍入使用最小边界输入；这些不是虚构的业务指标或市场表现。

| 探针 | 输入/触发 | 原1.11.0观察 | 本仓库对照 |
| --- | --- | --- | --- |
| P01 | 声明3条的真实K线正文截为22字节 | 无解析错误地留下1条记录 | checked解码返回Err |
| P02 | 3个连续交易日删除中间日，或重复首日 | 两种输入均被连续性校验判有效 | 两者均判无效 |
| P03 | `2026-10-22 12:30` | `is_trading_time=true` | false |
| P04 | 一字节心跳 | 返回0，没有panic或解析错误 | `try_parse=Err` |
| P05 | day真实文件截为63字节 | 返回1条，丢弃31字节残尾 | 返回Err |
| P06 | 文件缓存依次写键`a/b`、`a_b` | 查询前者取得后者值 | 两键独立 |
| P07 | 内存缓存TTL为0 | get为None，len仍为1 | len为0 |
| P08 | 真实收盘序列中间值改为NaN | EMA下一有效位置仍为NaN | 新有效分段恢复有限值 |
| P09 | 真实3条K线、KDJ窗口9 | 预热位置为Some(0) | 预热位置为None |
| P10 | 主板普通10%，昨收1.15元 | 涨停价1.26元 | 1.27元 |

临时审计工程最终10项测试退出0，表示**成功确认这些差异**，不能写成“原包10项生产测试通过”。生产代码没有因此增加原包依赖。探针源码摘要、夹具摘要、输入与观测、未执行范围保存在[证据JSON](../validation/evidence/upstream-review-20261023.json)。

两个重要反证已纳入结果：一字节心跳实际是静默0，不是panic；`Gbbqs::to_vec`已经用parsed标志避免再次解密，不能将其列为重复解密缺陷。RSI平盘返回100属于原实现口径，不能在没有约定时声称所有系统都必须返回50。

## 6. 构建、生产证据与限制

| 对象 | 本次/既有证据 | 可得结论 | 不可得结论 |
| --- | --- | --- | --- |
| 原核心1.11.0默认特性 | `cargo +1.89.0 check --lib --offline --locked`退出0，manifest指向实际发布包 | 该发布包在本机依赖缓存与工具链下可编译 | 所有功能/平台/外部服务通过 |
| 原核心1.11.0全特性 | 同环境`--all-features`退出101，E0554 | stable全特性存在确定构建问题 | 默认调用也无法编译 |
| 原包docs.rs | build4399035，nightly1.100.0，E0557 | 当前公开文档构建失败 | 与stable E0554为同一个错误码 |
| 原包差异探针 | 10项最终退出0 | 特定不利行为及本仓库对应修复可复现 | 原包完整单测、线上、数据库验收通过 |
| 本仓库既有离线验收 | MSRV/all-features/Clippy/fmt、独立协议夹具、属性测试和短时fuzz | 指定源码轮次通过相应回归 | 穷尽所有恶意输入、生产SLA |
| 本仓库既有真实验收 | 128个day文件、779,200行独立重解，两库首次/重复精确回读，4项故障恢复 | 指定真实历史子集及指定故障通过 | 原包同样通过、最新约2971万行全量通过、目标Linux生产通过 |

本仓库证据以[验证方法](../validation/VALIDATION.md)、[数据库验收](../validation/DATABASE_ACCEPTANCE.md)和[第三轮记录](../history/OPTIMIZATION_ROUND3.md)为准。实际数据库环境为macOS实验ClickHouse26.11.1.548、MongoDB8.0.16；Linux ClickHouse25.8.33.6/MongoDB8.0.16验收配置已固定，但尚未执行该平台验收。

本次没有新增在线行情采样、原包全套网络/数据库测试、吞吐基准或fuzz；没有实际执行池槽位竞态、非法UTF-8未定义行为。上游性能倍数、长期可用率、服务承诺和全市场准确率均为 `[INSUFFICIENT DATA]`，不据此给出评分或SLA。

## 7. 融合选择与改进顺序

| 路线 | 获益 | 成本/条件 | 建议 |
| --- | --- | --- | --- |
| 直接使用原1.11.0 | 接入快、功能广 | 需要重新处理已确认正确性、恢复及构建问题 | 用于隔离研究或功能验证 |
| 基于当前强化分支 | 复用共同历史与现有checked解析、资源预算、清单、回读 | 必须提升新版本、验收真实发布包和目标环境；仍有运营与协议迁移缺口 | **生产方案首选** |
| 原包与强化包同时进入生产 | 可并行比较 | 类型/规则分裂，依赖与错误语义复杂，不能天然提高准确率 | 只在隔离差异审计中使用 |
| 重写采集平台与异步核心 | 可以重新设计调度和接口 | 重复协议成本大，引入额外运行时及回归范围 | 当前批处理需求没有足够依据 |

优先处理仍在兼容入口中的危险解析/可变请求，维护2027完整日历与逐证券历史事实，再完成固定Linux与全量/故障/备份恢复验收。其后在现有CLI外增加任务账本、不可变产物和发布决策，最后按实际并发需求决定跨主机能力。当前原有`(date,code)`写库设计和同机锁，不能直接宣称多机写入安全。

原包的协议能力、本仓库的正确性与资源治理、拟议的生产发布层应分别承担职责。所有新建设项与验收门禁见[生产融合方案](PRODUCTION_FUSION_PLAN.md)，当前产品行为继续以[需求主文档](../requirements/REQUIREMENTS.md)为准。

## 8. Sources与复核入口

编号对应本次实际取得的来源；固定代码行链接见风险表。注册表/API/包/Git为同一项目的不同核对载体，数据库文档描述其自身语义，交易所公告描述对应规则，不把它们混算为独立质量背书。

<a id="cite-1"></a>
[cite:1] crates.io核心包元数据：<https://crates.io/api/v1/crates/rustdx-complete>。

<a id="cite-2"></a>
[cite:2] 不可变核心发布包：<https://static.crates.io/crates/rustdx-complete/rustdx-complete-1.11.0.crate>。

<a id="cite-3"></a>
[cite:3] docs.rs 1.11.0公开构建日志：<https://docs.rs/crate/rustdx-complete/1.11.0/builds/4399035>。

<a id="cite-4"></a>
[cite:4] 核心发布固定源码：<https://github.com/jackluo2012/rustdx/tree/48f4f112734c96ac2c021a69fbe54110f06dc108>。

<a id="cite-5"></a>
[cite:5] crates.io配套CLI元数据：<https://crates.io/api/v1/crates/rustdx-cli>。

<a id="cite-6"></a>
[cite:6] 不可变CLI发布包：<https://static.crates.io/crates/rustdx-cli/rustdx-cli-1.7.2.crate>；其固定Git提交为`a6deec180800c766c4d58d4cd2968da4f36c17cb`。

<a id="cite-7"></a>
[cite:7] 上游主分支提交API，核实日期2026-10-23：<https://api.github.com/repos/jackluo2012/rustdx/commits?per_page=2>。

<a id="cite-8"></a>
[cite:8] 上交所2026交易规则与生效公告：<https://www.sse.com.cn/lawandrules/sselawsrules2025/stocks/exchange/c/c_20260424_10816482.shtml>。

<a id="cite-9"></a>
[cite:9] 上交所说明主板风险警示股票5%调整为10%：<https://www.sse.com.cn/aboutus/mediacenter/hotandd/c/c_20260424_10816474.shtml>。

<a id="cite-10"></a>
[cite:10] 上海证监局转载7月6日报道，仅作交叉核对，不作为独立监管规则：<https://www.csrc.gov.cn/shanghai/c105566/c7643909/content.shtml>。

<a id="cite-11"></a>
[cite:11] 上交所暂缓条款附件，已核对暂缓范围：<https://www.sse.com.cn/lawandrules/sselawsrules2025/stocks/exchange/c/10816482/files/c4510656662e48dc9d8d0deb8cc864b3.docx>。

本地复核材料：[研究证据](../validation/evidence/upstream-review-20261023.json)、[架构现状](PROJECT_ANALYSIS.md)、[运行清单与导入](../operations/IMPORTS.md)、[既有验证](../validation/VALIDATION.md)。原始/tmp构建日志与临时审计工程不是长期归档，持久证据中明确记录其范围和摘要。
