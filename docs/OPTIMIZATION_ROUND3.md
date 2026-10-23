# 第三轮优化与交叉验证

日期：2026-10-23；基线：`5aed794`。本轮承接最新架构评审的六项建议，不重复上一轮已经完成的工作。保留同步I/O、有界线程、Rust2024/MSRV1.89；不推送、不发布。

按依赖顺序实施：

1. 覆盖管理：日历、规则核实日期、证券历史增加可自动执行的到期报告和更新入口；未知数据明确标记。验证临界日期、缺失、过期及更新拒绝。
2. 来源可重放：东财逐页保存有上限的原始响应、请求参数、采集时间和解析器版本，校验摘要后离线重放；CSV兼容性回归。
3. 回读效率：先测现有开销，再加入有界可配批量和长驻MongoDB回读进程；不减少导出键或字段的精确核对。验证丢键、重复、篡改、超量、异常退出和取消。
4. 协议接口：扩展checked不可变请求和独立解码到快照、财务、逐笔；新K线适配明确周期及量单位。旧入口兼容，独立参考解码及恶意输入回归。
5. 阶段观测：记录下载、解析、排序、写入、回读耗时及临时文件峰值、错误和数据新鲜度，明确并发耗时的统计口径。
6. 部署与故障验收：稳定数据库版本配置/CI、可复现大规模导入和重复、断网、强制终止及恢复验收；完整本地MSRV/fmt/Clippy/测试与真实数据交叉核验。

环境边界：本机无Docker/Podman/Colima；已保存的ClickHouse为macOS实验构建26.11.1.548，MongoDB为8.0.16。生产配置和主机未提供。稳定Linux配置、实际本机实验构建及真实生产验收必须分别报告。2027官方完整交易日历和全市场证券状态不能猜测；跨主机写入协调需要实际部署拓扑，当前只承诺同机互斥。

实现和测量结果随每阶段完成补充，最终验收记录单独保留原始指标与验证范围。

## 覆盖管理与来源重放

新增`data-status`，日历窗口、规则核实日期及证券状态缺口可输出JSON，`--strict`保留报告并返回失败；schema4采集清单包含同口径告警。边界、空数据集、缺失证券状态、命令退出码及原子报告回归通过。周巡检工作流已写入，尚未推送运行；当前日历到期窗口和规则核实截止的真实告警未被掩盖。详见[维护说明](DATA_MAINTENANCE.md)。

东财原始正文在解析前有界落盘，留存请求参数、采集时点、市场/源时间戳和版本，并提供逐页摘要校验重放。2026-10-23 UTC+8早间实测total5563中的2股，源日期2026-10-22；在线CSV与无网络重放逐字节相同，SHA-256=`0318b6b583e38a5f16aefb624f6029fe517baa9805847b26a7fa31d87d0cf10c`，与前轮独立保留CSV一致。真实原始正文保存为[回归夹具](../rustdx-cmd/tests/fixtures/east-raw-20261023.json)，详见[重放文档](EAST_REPLAY.md)。该验证只涵盖两只股票，不能代表全市场。

第一批Rust1.89 CLI all-targets测试、Clippy all-targets/all-features拒绝警告通过；离线重放恶意参数/摘要/页序回归通过。最终新增真实夹具及解析失败留证回归、Clippy和fmt复查通过。日志：`/tmp/rustdx-round3-stage1.log`、`/tmp/rustdx-round3-stage1-final.log`、`/tmp/rustdx-round3-stage1-fixtures.log`。

## 回读优化前基准

先保存基线release二进制（SHA与前轮最终验收相同），用[测量脚本](../scripts/measure_database_readback.py)执行实际隔离数据库导入。128个合成A股day文件，共65,536行，4线程，无gbbq/previous；每目标第一次写入、第二次重复写入，逐键全字段回读，各128批。未启用进程计时包装，以免Python启动开销影响两版耗时比较。

ClickHouse26.11.1.548本机实验构建整命令耗时11.214/9.216秒，MongoDB8.0.16为26.189/25.323秒。所有输出CSV摘要与回读字段摘要一致。该耗时包括输入解析、CSV、导入和回读，不单独代表数据库查询时延，也不代表真实生产吞吐。原始指标见[基准JSON](fixtures/round3-readback-baseline.json)；4次均成功，基准私有数据库已删除，输入与日志临时保留用于复测。

## 回读优化与实测

批量默认4096，环境变量可配1–16384；非法值在day发布与数据库访问前失败。ClickHouse继续逐批`FINAL`精确核对，查询解析上限设为1MiB以容纳最大键集。MongoDB改为每次导入一个长驻mongosh，容量1通道连接一个管道工作线程，按批请求/响应；主线程检查绝对deadline与取消，异常路径终止并回收进程/线程，最终EOF与退出码也必须通过。响应16MiB上限和完整字段校验保留。默认给长驻mongosh设置128MiB V8老年代上限（已有NODE_OPTIONS优先），不是总RSS限制。

schema5记录批量、回读耗时及`sha256_order=csv_row_order`，字段摘要不随批量大小改变；schema3/4旧的批内排序摘要可能不同。此次基准每证券512行，旧新摘要恰好一致，不能泛化为所有旧清单兼容。CSV摘要始终保持原格式。

| 同一65,536行，4线程，每库首次/重复导入 | 优化前秒 | 优化后秒 | 两次平均耗时下降 |
| --- | --- | --- | --- |
| ClickHouse本机实验构建 | 11.214 / 9.216 | 8.708 / 8.335 | 16.6% |
| MongoDB8.0.16 | 26.189 / 25.323 | 11.746 / 12.276 | 53.4% |

以上是整命令墙钟耗时，样本只有两次，不是生产吞吐承诺。批数128→16；优化后ClickHouse获取/解码约0.68–0.72秒、Mongo约2.48–2.69秒。所有导出键与6个数值字段精确通过，CSV SHA及字段SHA均与优化前相同。最大16384批量也在两库通过65,536行验收（4批）；Mongo最大批量在128MiB老年代限制下通过。数据与详细口径见[优化指标](fixtures/round3-readback-optimized.json)，完整本机清单/日志路径也在其中。

资源取舍：`/usr/bin/time`记录的ClickHouse命令最大RSS从约11.5–11.8MiB增为14.3–14.4MiB；Mongo从约255–256MiB增为321–325MiB。未限制堆的初版长驻Mongo达到约428–431MiB，128MiB老年代上限显著收敛峰值，但仍高于旧方案；它减少进程启动与查询次数，不保证内存下降。该RSS不是整棵进程树同一时刻的内存总和。

Rust1.89 CLI all-targets离线测试、Clippy all-targets/all-features拒绝警告、fmt通过。新测试覆盖批量边界、逆序响应、缺键、错值、重复、摘要跨批量一致、响应序号、截断/超量/多余响应、非零退出、阻塞管道的deadline/取消及回收。隔离两库的1,040键真实验收通过：重复/覆盖、CSV保留/清理、6/8字段与复权、错误表键、Mongo已有重复数据；之后只调整了Mongo堆限制及单次JSON解析，并复测相关单元测试、Clippy和真实65,536行。私有验收库均已删除。日志：`/tmp/rustdx-round3-readback-tests.log`、`/tmp/rustdx-round3-readback-acceptance.log`、`/tmp/rustdx-round3-readback-final.log`。稳定Linux与真实生产配置留待第6步，不将本机实验构建替代该验收。

## 协议安全与标准适配

快照/财务/当日逐笔/历史逐笔新增不可变checked请求和独立解码，Client相应入口迁移。拒绝请求参数非法、返回超量、截断、尾随字节与证券身份错配；财务增加有限值与日期检查，逐笔增加正价格/非负量检查。旧对象try_parse复用新路径，parse仍保留无Result签名及清空失败数据。财务换算改为先f64再乘10000，与pytdx一致，修正旧f32额外舍入。KlineRequest新增显式Symbol/Period入口，标准适配从请求推导周期，必须传源量单位。

真实协议来源为2026-10-23 UTC+8早间115.238.56.198:7709；两证券快照、一条财务、当日/历史逐笔各三条，全部公开字段及请求字节与固定pytdx提交逐一一致。原有股票/指数/分钟九条真实K线夹具继续一致。独立参考源码执行前验证摘要；[证据清单](../tests/fixtures/protocol/manifest.json)及[重解脚本](../scripts/verify_protocol_pytdx.py)可复现。恶意输入、旧状态清空、Client实际发包和标准单位/周期回归通过；详情见[协议接口](PROTOCOL_REQUESTS.md)。该小样本不代表全市场。

Rust1.89核心库all-targets/all-features离线回归通过：160单测、6项新增协议回归、4项K线回归及其余属性/本地服务/示例目标；文档55项及2项compile_fail通过，18项网络示例保持ignored。Clippy同范围拒绝警告、fmt/diff检查通过。真实响应作为初始语料，新增路径ASan/覆盖率fuzz执行526,218次、31秒，无崩溃；有界短测不代表穷尽。日志为`/tmp/rustdx-round3-protocol-final.log`、`/tmp/rustdx-round3-protocol-fuzz.log`；均退出0。整workspace及数据库最终检查留待第6步。

## 阶段观测

schema6增加可跨有界工作线程共享的每任务RAII阶段计数、累计耗时、阶段活跃墙钟时间并集，以及受跟踪临时路径逻辑字节峰值。day来源准备发生在最终报告创建之前，仍承接同一任务时钟；东财原始请求、解析与导出分别记录。数据库准备、写入、独立回读和Mongo退出确认都纳入观测。正常/错误/取消/panic展开释放计时和临时字节，不以累计耗时冒充CPU或相加为总耗时。

源快照、排序run/归并spool、待发布CSV/JSON计入临时空间；ZIP/解压缓存、清单证据及子进程scratch不在此口径。新鲜度使用无质量错误的非空来源末日和自然日年龄，错误/告警总数包含省略详情；不推测证券状态。完整定义见[OBSERVABILITY.md](OBSERVABILITY.md)。

并发/嵌套时间并集、独立任务隔离、失败展开、临时文件跨线程持有/发布释放，以及1/4/8线程CSV字节一致、跨块质量错误、协作取消和ZIP解压观测承接回归通过。Rust1.89整workspace all-targets/all-features离线测试、文档测试、Clippy拒绝警告、fmt及release构建通过；日志`/tmp/rustdx-round3-final.log`，退出0。真实大规模观测及故障验收在第6步单独保存，避免把离线测试当实际数据库结果。

## 固定配置与真实历史验收

新增固定digest的Linux ClickHouse25.8.33.6 Alpine/MongoDB8.0.16验收配置与Ubuntu24.04/Rust1.89数据库CI，包含小表约束、65,536行重复写入、TCP中断/SIGKILL/恢复，以及可选官方真实历史子集。Docker Hub已核对tag/digest，YAML/XML语法与本地文档链接通过；本机没有Docker，配置尚未推送或在Linux执行，不能声称生产验收通过。操作与精确限制见[DATABASE_ACCEPTANCE.md](DATABASE_ACCEPTANCE.md)。

官方ZIP于2026-10-23早间取得，HTTP大小549,908,719字节、Last-Modified=2026-10-22 07:58:56 GMT，完整包SHA-256=`07aab2f1166a794cebfa380cda058417499c4f74049a5fe36fe029f5c803d400`。12,218条目共950,692,224解压字节；固定选取前128个sh600历史文件，779,200行。Python struct独立逐行核对日期/代码/OHLC/量额，CSV 1/4线程逐字节一致；两库各首次及重复写入全部键/6个数值字段精确回读通过，最终逻辑行数不增加，ClickHouse合并后物理行数也一致。全部CSV SHA-256=`80f1b74671bcc1d8d3aa5bf7693e5dbacdcf04c60f40300255f23694fffa46e5`，输入成员/摘要及原始观测见[真实历史JSON](fixtures/round3-real-day-databases.json)。

| 本机真实历史子集 | 首次秒 | 重复秒 |
| --- | --- | --- |
| ClickHouse26.11.1.548实验构建 | 16.207 | 17.677 |
| MongoDB8.0.16 | 49.236 | 49.912 |

这是完整命令墙钟时间，包含解析、导出、写入及回读，只有两次样本。受跟踪临时逻辑峰值约45.6–46.0MiB，最终均为0；不是全盘临时占用或RSS。150条质量告警如实保留，来源最新末日2026-10-22、最旧末日2006-02-21；不将历史停止更新的数据当当前完整市场。仅验证该779,200行子集，未重跑全包约2971万行。

较大真实数据揭示本机原128槽位全局线程池耗尽：GlobalThread/Active=128、Scheduled=131，查询等待调度；采样首次显示在并行格式化finalize等待，关闭格式化后仍出现查询执行池等待。保留两次失败JSON：[格式化等待](fixtures/round3-real-day-formatting-failure.json)、[全局池耗尽](fixtures/round3-real-day-thread-pool-failure.json)。两次都已完整写入779,200行，但回读未完成；协作取消保存failed/acknowledged与CSV，未被误记verified。自有服务器重启并将全局池调整为512/空闲64后通过上述重复验收；CLI有界查询保留关闭并行输出格式化，减少附加线程需求。失败私有库和最后成功私有库均已删除。

四项真实服务故障/恢复均通过：[故障JSON](fixtures/round3-database-faults.json)。ClickHouse代理中断与SIGKILL时均已写8,192行；Mongo代理中断后已有1,000行，SIGKILL时已写8,192行。网络失败保留failed/unknown；SIGKILL保留最后检查点running/unknown；全部保留已发布CSV。每项在同一表/输出路径重跑后8,192键全字段精确通过、行数不增长、CSV摘要不变，且保留两份run_id归档，证明同机锁随进程死亡释放。Mongo强制终止用真实mongoimport成功后的暂停包装进程阻断退出码确认，随后终止自己的包装进程；不把该方式冒充数据库服务故障。四项私有库均已删除。

最后真实1,040键的小表验收再次通过：6/8字段、复权、更新覆盖、去重、错误ClickHouse键、Mongo重复数据与CSV留存/清理。最后在线东财两股CSV与固定夹具摘要仍相同，快照/财务/逐笔固定pytdx独立重解通过；最终ASan/覆盖率fuzz512,262次、31秒无崩溃。整workspace MSRV测试/文档、Clippy、fmt通过，查询配置修复后CLI全目标/Clippy/fmt/release复查通过。最终二进制SHA-256=`50e7593dd9e9f25207bb1cf8c0f312532a65499c8f0dcb6d6f4267a82cdaad5d`；日志`/tmp/rustdx-round3-databases-512.log`、`/tmp/rustdx-round3-final-external.log`及[验证说明](VALIDATION.md)。六项源码优化和本机可执行验收已完成；稳定Linux CI、用户生产配置/全量、2027完整日历/全市场状态仍依赖相应环境与真实数据。
