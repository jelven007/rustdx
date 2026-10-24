# rustdx 当前项目优化评估

审查对象：`/Users/bytedance/Documents/rustdx` 的实际工作区。基准 HEAD 为 `408e30eb172704070e384fb35fb0e56549a1fbc5`，工作区包含尚未提交的功能增强和目录迁移；报告中的“当前”指磁盘源码。核心包为 `jelven-rustdx-complete 1.12.2`，CLI 为 `jelven-rustdx-cli 1.8.2`，runner 为不发布的 `0.1.0`。

本轮交付代码分析、边界验证和优化建议，没有修改生产源码。全部新编译产物与临时验证程序位于仓库外，必要的验证日志另存为报告附件。

## 1. 项目状态与验证范围

项目已经具备行情协议、受管网络、标准数据、本地文件、参考事实、指标计算、采集导出和单机版本发布等层次。当前源码覆盖26个7709命令；响应身份校验、绝对任务期限、取消、连接许可、分页完成状态、字段资格及原始来源已经有实现。CLI使用分块处理和原子发布；runner使用本地SQLite账本、封存release、数据库精确回读和目录版本CAS。这些机制应作为后续优化的基础。[架构](file:///Users/bytedance/Documents/rustdx/docs/architecture/PROJECT_ANALYSIS.md#L7-L54) · [调用规则](file:///Users/bytedance/Documents/rustdx/docs/development/APPLICATION_INTEGRATION.md)

| 生产模块 | Rust文件数 | 文件总行数，含文档及内联测试 |
|---|---:|---:|
| 核心库 `crates/rustdx-core/src` | 97 | 28,885 |
| CLI `apps/rustdx-cli/src` | 22 | 7,461 |
| runner `apps/rustdx-runner/src` | 12 | 4,836 |
| 合计 | 131 | 41,182 |

审查结合高风险调用链阅读、接口与HEAD比较、配置及文档检查、全工作区编译检查和独立行为验证；不是逐行人工检查全部文件。149个源码及关键配置文件的摘要已保存，未发现这些文件在本轮检查启动后发生修改。[验证记录与源码摘要](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/verification.json)

| 本次检查 | 结果 | 范围 |
|---|---|---|
| Rust 1.89 格式检查 | 通过 | 全workspace |
| Clippy，拒绝警告 | 通过 | workspace、全部targets、全部features、锁定依赖 |
| 工作区测试及文档测试 | 565通过、0失败、27忽略 | 64份测试汇总；包含文档测试 |
| Rust文档构建 | 通过，无警告 | workspace、全部features、不构建依赖文档 |
| Python / Shell语法 | 通过 | 26个Python脚本、3个Shell脚本 |
| 当前文档本地链接目标 | 通过 | 抽取检查27份Markdown，无缺失文件目标；未验证所有锚点 |
| 独立边界验证 | 5组行为已核对 | 连接池、缓存、复权、旧日历、离线status |

测试设置了 `RUSTDX_SKIP_INTEGRATION_TESTS=1`。部分外部接口测试通过检查环境变量直接返回，仍会被Rust计为“通过”，所以565不能解释为565项真实行情或生产验收。本轮没有新增实盘采样、真实数据库验收、Linux部署、性能计时或fuzz运行。现有实盘与数据库资料仅按其原范围引用。[外部测试的跳过入口](file:///Users/bytedance/Documents/rustdx/tests/integration/tests/stocklist.rs#L25-L58)

## 2. 优先级与建议清单

P1表示应在对外稳定交付或对应业务上线前处理的正确性、可靠性和适用范围问题；P2表示性能、维护或运行能力改进。优先级是本轮工程判断，不是已经发生生产事故的证明。

| 编号 | 优先级 | 需要优化的地方 | 判定性质 |
|---|---|---|---|
| F01 | P1 | 复权入口校验证券与事件身份 | 已复现 |
| F02 | P1 | 连接池等待及时响应取消和父任务期限 | 已复现取消延迟 |
| F03 | P1 | 缓存合并请求的锁等待纳入任务预算 | 已复现超时后成功返回 |
| F04 | P1 | 旧日历API禁止隐式推断覆盖范围外日期 | 已复现 |
| F05 | P1 | 本地运维命令的TCP启动策略 | 已复现网络故障阻断status |
| F06 | P1 | 破坏性API变更与版本、发布身份匹配 | 源码及现有迁移文档确认 |
| F07 | P1 | 0feb日期资格和0547盘中推送验收 | 已实现，业务验收仍有限定 |
| F08 | P1 | 财务、成交量与其他字段的单位和证据覆盖 | 已有资格机制，覆盖仍有限定 |
| F09 | P1 | 全市场点时事实和新年度日历 | 数据运营待办 |
| F10 | P2 | 聚合Vec批量查询的结果内存预算 | 源码设计边界 |
| F11 | P2 | 目录预热、共享及报价热路径分配 | 需性能测量后优化 |
| F12 | P2 | runner在线来源从小范围日线扩展 | 现有合同边界 |
| F13 | P2 | 归档、版本和账本长期留存预算 | 现有运行能力边界 |
| F14 | P1 | 目标环境资源、指标及恢复验收 | 已有模板及有限证据 |
| F15 | P2 | CI增加行为、API兼容和性能回退检查 | 配置检查 |
| F16 | P2 | 修正当前架构文档中的过期结论 | 已确认描述冲突 |
| F17 | P2 | 按职责继续拆分大型编排模块 | 维护性建议 |

## 3. 已复现的正确性与可靠性问题

### F01：复权函数接受其他证券的事件

`Fq::build`筛选了事件类别和日期，但没有核对事件的市场及代码，也没有确认输入日线始终属于同一证券。`file::day::fq::Day::from_bytes/concat_bytes`继续把传入事件交给该函数。因此，直接使用库的调用方若传入全市场或错误分组的GBBQ切片，会得到受其他证券事件影响的因子。[计算入口](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/file/gbbq/fq.rs#L32-L99) · [日线封装](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/file/day/fq.rs#L23-L35)

本轮使用仓库现有日线和GBBQ夹具：把`sz000002`在`19910601`的实际夹具事件传给`sz000001`的7,226条日线，函数成功返回，因子序列与未传入该事件时不同。该验证证明身份约束缺口，不是对证券真实收益的重新计算。[行为日志](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/probes.log)

建议在checked复权入口解析完整Symbol，验证每条日线和事件的身份；跨证券输入返回明确错误。增量Day封装已经校验Factor的代码及基准日期，应保留这项保护。空事件和同证券多事件仍应正常处理，保留停牌期间事件链与基期规则。[已有增量基准校验](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/file/day/fq.rs#L64-L80)

当前CLI已使用完整代码分组并取对应事件，本轮没有证明CLI正常分组路径算错；主要影响直接调用上述核心API的应用。[CLI分组保护](file:///Users/bytedance/Documents/rustdx/apps/rustdx-cli/src/io.rs#L275-L286)

验收：同代码不同市场、不同代码、混合日线、错误增量基准均拒绝；原有同证券全量与增量复权结果一致。

### F02：连接池满额等待不能及时响应取消

`ConnectionPool::acquire`在进入循环时检查任务预算，但满额时使用整段`wait_timeout(state, remaining)`等待。取消标记不会通知该Condvar；等待时长也由传入连接等待截止时间计算，而不是每次取父OperationContext剩余预算。持有者不归还连接时，取消可能要等到槽位释放或原等待时限才被发现。[等待实现](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/pool.rs#L99-L143)

本轮占满一个本地连接池，取消另一个等待任务：取消后150ms仍未返回，归还槽位后才返回`Interrupted`，实际取消响应耗时155ms。受管全局许可和BatchGate已用短周期等待检查预算，可以统一采用同类机制。[现有许可等待](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/governance.rs#L190-L229)

建议把Condvar等待分成短周期，使用父任务与连接等待的共同剩余期限，并在每次唤醒后检查取消。进一步用RAII预留槽位对象管理`active`，统一锁中毒错误处理。

验收：持有者始终不归还时，取消和短父期限仍在规定响应上限内退出；没有槽位泄漏，关闭池能唤醒全部等待者。

### F03：缓存锁等待可以越过期限并成功返回

`Cache::get_or_fetch`以64个Mutex合并并发获取，使用阻塞`lock()`等待，取得锁后也没有再次检查OperationContext。相同key，以及散列到同一条带的不同key，可能被慢fetch阻塞。FileCache自己的跨进程锁已有期限与取消检查，但上层合并请求的等待没有同样保障。[合并请求入口](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/cache.rs#L802-L827) · [文件锁对照](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/cache.rs#L399-L423)

本轮给第二个请求75ms任务预算，并让第一个请求持有该锁。第二个请求等待204ms后返回`Ok`，此时其OperationContext已为`TimedOut`。[行为日志](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/probes.log)

建议增加可表达超时/取消与fetch错误的checked入口，锁等待、二次缓存读取和返回前共用预算。对无法强制中断的fetch闭包明确协作规则，并防止同条带重入死锁。保留旧入口兼容性时，清楚标注其等待契约。

验收：慢fetch、条带碰撞、父期限和取消均有行为测试；期限耗尽后不继续以成功结果交付。

### F04：旧日历入口对未知年份给出交易日判断

严格`try_*`入口通过BuiltinCalendar检查2024–2026覆盖范围。旧`is_trading_day`直接调用依赖的工作日判断，依赖只排除这三年的休市表；其他年份的周一至周五会返回true。旧前后交易日查询还保留搜索失败后的日期回退。[覆盖与旧入口](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/calendar.rs#L43-L117) · [旧导航回退](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/calendar.rs#L172-L218)

本轮查询`2027-01-01`：旧入口返回true，严格入口返回`OutOfRange`。这说明旧API把未知与已核实交易日混在同一个bool中；本轮没有获取2027完整官方日历。[行为日志](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/calendar-probe.log)

建议应用与runner统一使用CalendarProvider或`try_*`；给旧入口添加弃用说明，在破坏性版本中收敛其语义。发布覆盖检查应从实际日历元信息取范围，不继续维护另一份截止常量。

验收：覆盖前后、跨年导航、极端日期及休市区间都不会返回未经确认的“有效交易日”。

### F05：本地status默认依赖行情节点可用

CLI在命令分派前初始化TCP；runner的`status/metrics/catalog/backup/restore/recover`等管理操作默认也经过TCP初始化，只有显式跳过才能离线执行。初始化是进程内状态，不因共用许可目录而跨进程共享日池快照。[CLI启动](file:///Users/bytedance/Documents/rustdx/apps/rustdx-cli/src/main.rs#L16-L39) · [runner管理入口](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/src/main.rs#L151-L160)

本轮把候选节点限定为一个不可用的本地端口：`runner status`退出1，错误为“当日有效IP池为空”；相同本地账本加`--no-tcp-startup`退出0并返回状态。测试没有访问外部行情服务。[完整对照](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/offline-status-probe.json)

建议按命令实际网络需求选择默认启动方式，让本地查询和故障恢复能独立运行；需要主动测速的命令保留显式预热选项。在线入口继续执行有效池与预算检查。现有systemd指标和恢复模板已经显式跳过TCP，可把这项经验固化到命令默认策略。[指标模板](file:///Users/bytedance/Documents/rustdx/deploy/runner/rustdx-metrics.service#L5-L16)

验收：断网时本地状态、指标、文件处理和恢复仍可用；在线采集正确失败；help及无效参数保持不启动网络。

## 4. 接口与业务适用范围

### F06：破坏性变更尚未形成新的发布版本

HEAD中的`finance()`返回`FinanceInfoData`，当前返回`FinanceSnapshot`；`heartbeat()`从`u16`改为`HeartbeatAck`，但核心包仍为1.12.2。现有迁移文档已明确记录这是破坏性变化、当前仅源码交付，因此此项属于已知发布待办。[当前财务入口](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/client.rs#L681-L715) · [迁移约定](file:///Users/bytedance/Documents/rustdx/docs/release/MIGRATION.md#L99-L143)

建议确定正式兼容策略：保持旧公开方法并使用新方法名，或者进行破坏性版本升级；同步CLI依赖、锁文件、示例和迁移说明。发布前使用旧调用方小工程及API兼容检查，随后执行现有独立解包验证。固定源码提交和包摘要，避免使用同版本号识别不同内容。

验收：旧调用方的编译结果符合声明；新版本可以从独立发布包构建，CLI依赖匹配；发布清单包含准确提交、锁文件和包身份。现有preflight值得保留，本轮没有发布或查询注册表同版本的实际字节。[发布验证](file:///Users/bytedance/Documents/rustdx/scripts/release/preflight.py#L105-L143)

### F07：0feb和0547应继续完成业务资格验收

0feb请求日期编码已修正，仍没有响应真实交易日的通用资格；`require_trading_date()`当前明确返回Unsupported。0547已有同步状态、游标、队列预算和重建机制，但真实盘中主动推送窗口仍列为未验收。这些是已实现能力的验收边界。[日期门禁](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/recent_minutes.rs#L75-L103) · [订阅状态](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/quote_subscription.rs#L49-L175) · [当前验收范围](file:///Users/bytedance/Documents/rustdx/docs/testing/PRODUCTION_ACCEPTANCE.md#L22-L34)

建议为0feb建立绑定证券、日期、节点、请求原码和独立同日参照的资格机制；在精确交易窗口采集0547被动推送、游标推进、全部订阅键、断连重建和溢出恢复。结果落在新证据中，维持现有拒绝未经确认日期发布的行为。

验收：请求日期、响应日期资格和分钟标签可区分；订阅只有在所有必需证券恢复完整基线后才可消费；实盘与模拟会话证据分别统计。

### F08：字段资格和单位需要扩大到实际消费范围

财务已用逐字段资格保护默认业务值，但现有文档只列固定样本的10字段独立核验，其他字段仍有候选及未知。052d不同请求的成交量存在量化差异；新052b整数股数链已在所列范围核对，不能自动扩大到所有品种和周期。当前CanonicalBar也明确记录源单位是调用方指定还是参考依据。[财务资格](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/finance_semantics.rs#L124-L218) · [成交量来源](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/data/model.rs#L107-L163)

建议按应用真正使用的字段建立“命令、品种、周期、源单位、日期资格、证据版本”矩阵，逐步补足财务报告期、逐笔金额/事件、统计资源单位与未知槽。标准适配层应保留qualification，不把候选、估算和缺失转成默认已核验数值。

验收：更换证券、更新财报原值、改变周期或来源时，旧资格不会被继承；输入单位转换只发生一次；估算成交额和源报告金额仍分别存储。

### F09：全市场点时事实与2027日历需要运营闭环

核心已提供SecurityHistory、外部HolidayCalendar和覆盖拒绝机制，但模型存在不等于完整事实已经取得。当前配置日历截至2026年底，全市场证券状态、名单区间和新年度日历仍在验收矩阵中列为待办。[状态区间](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/limit/history.rs#L39-L59) · [外部日历](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/calendar/provider.rs#L172-L227)

建议建立固定维护流程，保存公告原字节和摘要、来源截止、上市/退市、ST变化、停复牌及点时名单。先明确2027公告的接入与交叉核验路径，资料可得后再扩展覆盖。代码前缀分类继续作为提示，不能替代权威证券身份与历史事实。

验收：应有证券和应有日期均可追溯；历史筛选不会使用当前名单代替过去名单；Unknown仍阻止完整发布。

## 5. 性能、规模与运行能力

### F10：Vec批量结果仍可聚合大量内存

旧批量接口限制输入数量并使用共享deadline，但`command_batch`把所有成功结果保留在Vec中，没有聚合结果的总行/字节预算。并发数有界并不限制最终累积结果大小。现有stream接口已提供单项和总交付预算，适合大范围消费。[聚合结果](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/client/extended_queries.rs#L290-L370) · [流式预算](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/client/stream.rs#L18-L46)

建议大批量应用优先接现有`*_batch_stream`；为Vec入口增加明确总预算或公开适用上限。测量峰值应涵盖结果、解码器、原始正文、队列、序列化缓冲和持有的共享数据，JSON字节数不能替代RSS。

验收：固定总预算下扩大证券数，进程树峰值仍符合目标；中途错误和预算耗尽保留任务未完成状态。

### F11：目录共享与热路径优化应有独立测量

默认Client各自拥有QuoteMetadataStore；冷报价查询需要在请求预算中加载完整目录，应用已经可以显式共享Arc和预热。精度查找还会为`code.into()`临时分配String。[Client目录入口](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/client.rs#L45-L119) · [精度查询](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/stock/quote_metadata.rs#L81-L94)

建议在应用入口统一持有明确生命周期的目录Store并按日预热，记录启动、冷目录加载和热报价三类耗时。若分配剖析确认该查找明显，再调整键组织以使用借用字符串查询。按真实负载评估进程间重复测速成本，保持有效节点检查。

验收：共享不会混合失效目录或市场代次；缺精度仍报错；优化前后价格与非价格字段完全一致，并有同环境性能记录。

### F12：runner在线来源仍是小范围、串行日线合同

当前TCP来源最多32只排序唯一沪深A股、已结束日线，配置不接受当天、五分钟或复权来源，采集按证券同步执行。这是明确合同，不应把核心26个接口等同于runner已经持续采集26种数据。[请求边界](file:///Users/bytedance/Documents/rustdx/apps/rustdx-cli/src/tcp_day.rs#L99-L129) · [应用合同](file:///Users/bytedance/Documents/rustdx/docs/development/APPLICATION_INTEGRATION.md#L207-L221)

建议按目标范围拆分预算明确的采集任务，给全市场、分钟线、快照和订阅分别定义应有键、来源留证、完成状态和版本发布合同。全市场批量扩展先接有界流式交付和任务编排，再扩大输入范围。

验收：缺证券、重复键、不完整页和源修订均不会推进完整目录；新任务类型仍能独立重放和精确回读。

### F13：长期归档和账本需要留存策略

任务有工作空间预算和开始前的剩余磁盘检查，但`attempts/releases/catalogs/database`及append-only事件会随运行积累。每次任务预算不等于整个运行目录的长期容量上限，备份也会复制多个历史版本。[任务空间检查](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/src/pipeline.rs#L955-L973) · [账本追加事件](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/src/ledger.rs#L60-L74) · [备份范围](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/src/ops.rs#L93-L107)

建议建立容量预测、分层归档和留存计划；提供只生成候选清单的清理预览，计算catalog、release、数据库回执和备份引用后再处理过期产物。历史可消费版本和原始证据需要明确的保留承诺。

验收：长期运行样本有容量趋势；清理后所承诺版本仍可消费和恢复；本轮不执行任何数据清理。

### F14：目标环境的资源、指标及恢复验收

现有systemd模板已设置MemoryMax、CPUQuota、TasksMax和进程组；runner也有取消宽限及目录空间监测。当前机器证据与有限Linux/数据库演练不能直接确定用户目标部署的资源上限、长期SLO和RPO/RTO。现有数据库原生备份恢复资料已经取得限定结果，后续重点应是目标环境及持续写入等剩余范围。[服务配置](file:///Users/bytedance/Documents/rustdx/deploy/runner/rustdx-runner.service#L6-L28) · [验收范围](file:///Users/bytedance/Documents/rustdx/docs/testing/PRODUCTION_ACCEPTANCE.md#L29-L34)

建议在目标配置下测量完整进程树RSS、FD、线程/进程数、磁盘峰值、阶段耗时及任务覆盖；将任务内网络和阶段统计持久化后导出指标。独立metrics进程直接读取进程内NetworkStats不能代表采集worker的统计。恢复演练应固定旧catalog版本并核对全部键/字段，确认业务要求的持续写入、权限与恢复时限。

验收：取消、磁盘不足、服务重启和数据库确认丢失有目标环境结果；告警基于业务覆盖和实际预算，恢复后的所需版本可精确消费。

## 6. 工程维护与检查流程

### F15：CI应覆盖已发现的行为和回退风险

当前CI有三个操作系统、MSRV、Clippy、格式、数据库工作流和两个fuzz目标。指标性能任务只生成并上传观察，已有`compare_indicators.py`没有接入该任务；API兼容与独立包消费验证也未成为该CI的直接步骤。[CI](file:///Users/bytedance/Documents/rustdx/.github/workflows/ci.yml) · [性能比较工具](file:///Users/bytedance/Documents/rustdx/scripts/bench/compare_indicators.py#L23-L44)

建议修复F01–F04时把对应行为回归加入所属crate；添加旧调用方/API兼容检查和必要的独立包消费检查。性能比较固定硬件、工具链、参数和输入范围后再执行，允许合理噪声，不能仅以同OS/架构视为同一环境。

验收：缺陷重新出现时检查明确失败；性能回退有可比基线及保存的输入，远端CI结果绑定实际提交。

### F16：当前架构文档有过期的能力描述

同一份架构文档在第3节已描述TCP日线来源，第5节仍写“runner采集来源仅本地A股原价day”；0feb表述也没有清楚区分已修的编码错位与仍未验收的通用日期资格。用户据此判断功能范围时会得到不同结论。[两种来源](file:///Users/bytedance/Documents/rustdx/docs/architecture/PROJECT_ANALYSIS.md#L41-L44) · [过期表格](file:///Users/bytedance/Documents/rustdx/docs/architecture/PROJECT_ANALYSIS.md#L58-L65)

建议当前架构、接口参考和验收矩阵共用能力编号，明确区分“已实现、已回归、限定实帧、目标生产”。历史资料保留原结论，当前入口按源码和最新限定证据更新。现有文档本地文件链接检查通过，本项针对内容一致性。

验收：同一能力在README、架构、接口、调用约定和验收矩阵的范围一致，历史报告不被当作当前完整验收。

### F17：大型编排模块按职责拆分

`pipeline.rs`含冻结、事实覆盖、归一化、封存、目录发布和恢复；`database.rs`含容量证明、会话、写入及回读；CLI `io.rs`同时组织解析、导出、兼容导入和因子输入。目录分层已完成，下一步可减少这些模块内部的职责跨度。[runner编排](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/src/pipeline.rs) · [数据库会话](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/src/database.rs) · [CLI I/O](file:///Users/bytedance/Documents/rustdx/apps/rustdx-cli/src/io.rs)

建议按上述职责拆内部模块，保持公开入口、状态机、证据格式及同步有界设计。优先提取可独立审查的纯校验和容量计算，保留事务、锁与提交顺序的集中表达；文件行数本身不是缺陷，也不应成为拆分的唯一依据。

验收：模块依赖清楚，故障路径仍能在一个调用链中追踪；固定CSV、协议和版本发布结果不变。

## 7. 建议执行顺序

1. **先关闭可复现问题：** F01身份约束、F02连接池等待、F03缓存期限和F04日历旧入口；加入能证明失败行为的回归。
2. **再稳定应用入口与交付：** 决定F05启动默认策略，落实F06版本兼容方案，更新F16当前能力描述。
3. **按实际业务补证据：** 处理F07–F09及F14；先限定证券、日期、字段与部署范围，再扩展到全市场。
4. **根据测量做规模优化：** F10–F13、F15、F17；先建立内存、阶段与热路径基线，再调整预算和模块组织。

各项工作继续沿用Rust 2024、MSRV 1.89、同步I/O和有界线程设计。性能优化与业务验收分别记录；成功解码、同码覆盖、模拟故障回归和目标生产可用性保持不同判定。

## 8. 可复核附件

- [机器验证记录、149个源码/配置摘要与原始行为输出](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/verification.json)
- [Clippy原始输出](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/clippy.log)
- [565通过、27忽略的完整测试输出](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/tests.log)
- [Rust文档构建输出](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/rustdoc.log)
- [连接池、缓存、复权边界输出](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/probes.log)
- [旧日历边界输出](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/calendar-probe.log)
- [离线status对照](computer:///Users/bytedance/Documents/rustdx/project-review-20261022/evidence/offline-status-probe.json)

源码摘要清单SHA-256：`0fb7d722036c805f593be7944794bb0791c8d9430bf4153ea76ff0e73a26702a`。附件保存实际输出；独立验证使用本地模拟TCP以及仓库既有数据夹具，没有把构造输入当作实盘采样。
