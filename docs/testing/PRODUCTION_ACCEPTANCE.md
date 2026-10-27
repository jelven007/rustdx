# 当前验收矩阵

<!-- lifecycle:registry -->
读者：产品/运维；责任：验收维护者。源码核验：2026-10-27；每30日及新验收结果或未关闭范围变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

更新日期：2026-10-27。本文区分当前能力、已取得的验证和未关闭的生产门禁。实际命令与环境见所引用证据；当前版本号不能作为源码身份。

## 1. 判定层级

| 层级 | 通过的含义 | 证据要求 |
| --- | --- | --- |
| 实现 | 可调用入口和契约存在 | 源码及公开接口 |
| 工程回归 | 指定源码在指定环境通过行为/故障检查 | 提交、命令、退出码、样本、跳过项 |
| 真实样本 | 实际来源在所列时点/范围与独立依据一致 | 原请求/响应、节点、时间、参数和字段对照 |
| 目标生产 | 指定部署、权限、真实范围、容量和恢复满足业务目标 | 目标环境记录、长期观测与恢复演练 |

本机测试、非空结果、主站枚举到空页和同码覆盖均不能直接推导目标生产通过。

字段判定另外区分结构解码、单位换算、业务派生、候选/未知与范围内独立核验，详见[返回字段状态](../reference/RESPONSE_FIELD_SEMANTICS.md#parsing-status)。财务Verified、成交ConfirmedByProfile、日期资格、分页complete和订阅Synchronized分别表达不同条件，不能互相升级。

## 2. 当前范围

| 能力 | 已取得证据 | 尚未验收 |
| --- | --- | --- |
| 26同码命令 | eltdx3.3.0注册表独立核对，四条现代入口、两节点98帧离线重放；板块两节点80帧身份/MD5；通用300字节资源新增两节点114帧/ZIP及全部CFG槽核验 | 全部增强业务契约、市场2全量、其他资源格式与统计业务单位/日期 |
| 受管响应身份 | 旧/新收帧、握手/心跳/探测共用校验；9项专项、归档拒绝及448项工作区回归 | 本次没有新增实盘采样，身份回显核验限已保留材料 |
| 0010财务 | 固定平安银行H1更新20260815的10字段；Q3更新20261024、报告期2026-09-30的17字段逐位公告证据；85.15亿元为三活动合计、汇率影响−26.43亿元后为58.72亿元，固定口径诊断和拒绝回归 | 原CashFlowNetChange不同口径仍拒绝标准/候选换算；Q3共7命名候选、7槽Unknown；其他证券/更新版本及口径 |
| 行情/目录/052d/052b | 历史052b两节点3只在市A股24条日线8字段与官网day精确一致；10月27日四证券27,237条核对，显式day参考入口并列原TCP/参考及44条源差异，缺日/篡改拒绝 | 1条052b股数少9股及43条金额差1ULP、旧052d差−94至+1股保留；全品种/周期源量、分类权威性、未知状态/尾槽、全市场历史 |
| 0fc5/0fc6事件/金额 | 原始状态/事件/阶段/方向资格、完整枚举与源额/估算分离回归 | ConfirmedByProfile的独立业务核验、全品种价格单位、逐笔完整性及独立盘后源额 |
| F10/板块/资金流/辅助指标 | F10字节范围与GBK；板块同源/MD5/严格解析及两节点105,826条成分；通用资源预算/尾部/完成状态、统计ZIP/CRC/GBK/唯一键及两节点32,404条CFG记录全槽一致；分档/指标已有样本 | F10文本指标结构化、其他资源格式、统计单位/日期资格、官方分类/资金身份和未知selector/指标窗口 |
| 0feb/0547 | 历史日期schema2两节点六证券六日17,280点与同日0fb4、72次日线开末价一致；新209点跨源边界诊断；两节点各33个日期/证券请求非空，000001的22组开末价对官网day；收盘前两节点各4次poll_update明确RequestedRefresh、行情/游标变化及完整状态独立回放 | 0feb默认日期仍Unsupported；11:30跨源价量差、通用日期资格/完整保存期限/量单位未验收；未观察到被动推送，主动刷新不关闭该门禁 |
| 0feb扩大采样/0547半帧边界 | 两节点共456次分钟请求、43,200点独立重解；采样非空下界000001为20000609、159915为20111209、510300为20120528；发送前已收到半帧不会满足后发同id刷新请求，模拟红绿反转回归 | 33组历史样本跨节点原值不同；106组000001官网day开末价对照有35组差异，当日参考缺失；连续完整历史、准确全点、服务器保存承诺及真实被动推送仍未验收 |
| 日线CLI | 原子发布、schema7、流式RowBinary保住Float64；最终源码在固定Linux两库各两次精确核对779,200历史行，CSV确定性不变 | 当前全包/目标生产配置与全量 |
| runner | 单机覆盖、唯一release、目录CAS、固定版本消费与恢复；Linux31项全执行，真实两库600只模拟证券/1,200行跨512批次 | 真实全市场事实、生产Linux部署、权限/硬配额、长期负载 |
| TCP日线runner | 新采集用052b来源v2；新有总预算计划最多10000证券/8线程；40只模拟证券80行顺序原码重放，缺40键拒绝目录推进；两节点各12条实际选定日线字段精确；旧052d及无schedule原合同重放通过 | 已结束沪深A股日线；真实全市场吞吐/事实、当天及分钟/快照/订阅；已有真实样本越过下界，未到空页 |
| 长期留存预览 | 控制锁、含提交WAL的独立只读快照、全部历史版本/数据库回执/显式备份引用核对；逻辑/分配容量及双采样趋势；只生成候选 | 不执行删除/账本裁剪；历史退役承诺及生产长期容量仍须独立设计验收 |
| 控制层备份 | SQLite WAL在线备份、文件/数据库请求证据隔离恢复，Linux真实两库案例也执行 | 目标生产恢复编排、留存与批准的RPO/RTO |
| 数据库原生备份恢复 | 第六阶段固定Linux/两库原生全备恢复至空目录，两个runner版本、每库2,400合成行，全字段/元数据及固定导出一致；错误写入/重复恢复拒绝 | MongoDB物理卷与日志/PITR、持续写入、生产用户/ACL、规模和批准的RPO/RTO |
| 工程目录迁移 | 第五阶段固定源码在macOS相关回归及Linux工作区、fmt/Clippy/文档检查通过；原字节证据随隔离源码输入保留 | GitHub Actions配置未推送/未运行；本轮没有重跑fuzz |
| 17项优化与维护 | Rust1.89/macOS全工程522通过/10忽略、文档57通过/18忽略、实际CLI控制流程20通过/1排除；Python10项、fmt/Clippy/严格文档；共同指标10组同机交替对照0.43–1.23倍、输出一致；CI调用方/解包消费与职责拆分 | 真实数据库外部用例本轮不执行；配置不等于Actions已运行；合成性能不证明生产规模 |

## 3. 证据入口

| 范围 | 报告/机器证据 |
| --- | --- |
| 返回字段语义 | [26命令字段状态及证据索引](../reference/RESPONSE_FIELD_SEMANTICS.md)、[H1独立期望](../../crates/rustdx-core/tests/fixtures/protocol/finance.semantic.expected.json)、[Q3独立期望](../../crates/rustdx-core/tests/fixtures/finance-q3/expected.json) |
| 10月27日实时遗留修复 | [修复报告](../archive/development/LIVE_MARKET_LEGACY_FIX_20261027.md)、[原帧/日志/摘要](evidence/live-market-legacy-fix-20261027)、[主动刷新与日期端点独立回放](evidence/live-market-legacy-fix-20261027/live-replay.json)；客户端修复与源数据/能力未关闭项分别记录 |
| 10月27日上游能力补证 | [补证修复报告](../archive/development/UPSTREAM_CAPABILITIES_FIX_20261027.md)、[固定原帧ZIP与重放脚本](evidence/upstream-capabilities-20261027)、[独立逐点/现金流结果](evidence/upstream-capabilities-20261027/independent-replay.json)；源差异照实保留，半帧模拟不计作被动实盘 |
| 26命令补齐 | [实施报告](../archive/validation/ELTDX_3_3_0_26_COMMANDS_IMPLEMENTATION_20261024.md)、[JSON](evidence/eltdx-3.3.0-26-commands-implementation-20261024.json) |
| P0响应正确性 | [修复说明](../archive/development/RESPONSE_IDENTITY_FIX_20261022.md)、[验证JSON](evidence/response-identity-fix-20261022.json) |
| 近期分时与推送 | [0feb边界](../archive/validation/RECENT_MINUTES_PUSH_BOUNDARIES_20261024.md)、[后续非空夹具与窗口报告](../archive/validation/NONEMPTY_FIXTURES_PUSH_WINDOW_20261024.md)、[推送报告](../archive/validation/QUOTE_PUSH_VALIDATION_20261024.md) |
| 0feb日期错位修复 | [修复报告](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)、[原始两节点支持包](evidence/recent-minutes-date-selector-20261024)、[独立日期对照](evidence/recent-minutes-date-selector-20261024/date-selector-verification.json)；旧错位证据及摘要不变 |
| TCP日线整数股数修复 | [修复报告](../archive/development/TCP_DAILY_INTEGER_SHARES_FIX_20261024.md)、[新原始支持包](evidence/tcp-day-volume-fix-20261024)、[24条全字段独立核对](evidence/tcp-day-volume-fix-20261024/independent.json)；旧052d差异及旧052b金额证据不改 |
| 板块文件完整性修复 | [修复报告](../archive/development/BLOCK_FILE_INTEGRITY_FIX_20261024.md)、[新原始支持包](evidence/block-integrity-fix-20261024)、[两节点全文件MD5及全部成分对照](evidence/block-integrity-fix-20261024/independent.json)；旧02c5元数据证据不改 |
| 通用资源与推送门禁 | [修复报告](../archive/development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md)、[新证据](evidence/resource-contract-push-fix-20261024/summary.json)；两节点114帧、907,312原始CFG槽及307,838候选映射独立一致；0547仅新增休市门禁，盘中仍待验 |
| 真实历史/故障 | [第三轮报告](../archive/history/OPTIMIZATION_ROUND3.md)、[历史JSON](evidence/round3-real-day-databases.json)、[故障JSON](evidence/round3-database-faults.json) |
| runner组件 | [实施报告](../archive/development/PRODUCTION_IMPLEMENTATION.md)、[JSON](evidence/runner-implementation-20261023.json) |
| TCP日线闭环与剩余优化 | [第四阶段报告](../archive/development/REMAINING_FIXES_STAGE4_20261024.md)、[JSON](evidence/remaining-fixes-stage4-20261024.json) |
| 最新源码、RowBinary与固定Linux | [第五阶段报告](../archive/development/REMAINING_ACCEPTANCE_STAGE5_20261024.md)、[支持包](evidence/acceptance-stage5-20261024)、[最终源码身份](evidence/acceptance-stage5-20261024/source-pacing-final-manifest.json) |
| 第五阶段实际数据库/环境 | [Linux命令](evidence/acceptance-stage5-20261024/linux/evidence/results.json)、[真实历史](evidence/acceptance-stage5-20261024/linux/evidence/real-history.json)、[环境/二进制](evidence/acceptance-stage5-20261024/linux/evidence/environment.json)、[清理审计](evidence/acceptance-stage5-20261024/linux/evidence/cleanup-audit.json) |
| 服务端原生备份恢复 | [第六阶段报告](../archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)、[最终演练](evidence/acceptance-stage6-20261024/linux/evidence/attempt2/recovery.json)、[输入与日志](evidence/acceptance-stage6-20261024/linux)、[清理审计](evidence/acceptance-stage6-20261024/linux/evidence/cleanup-audit.json) |
| 本次目录重组 | [迁移记录](../archive/release/PROJECT_RESTRUCTURE_20261024.md)、[验证JSON](evidence/project-restructure-20261024/verification.json) |
| 17项优化最终范围 | [收尾报告](../archive/development/OPTIMIZATION_FINAL_20261025.md)、[工程及性能支持包](evidence/optimization-final-20261025)、[执行进度](../archive/development/OPTIMIZATION_PROGRESS_20261025.md)；与此前实帧/固定Linux证据按各自源码身份区分 |
| 早期生产结果 | [历史验收报告](../archive/validation/PRODUCTION_ACCEPTANCE_THROUGH_20261024.md) |

10月10日约2981万行ClickHouse结果属于当时版本；10月23日779,200历史行与runner模拟事实是不同范围。第五阶段分别记录初始源码的macOS实验服务及最终源码的固定Linux镜像执行；按源码身份、命令及实际范围判定，不因相同版本号或旧服务通过推导生产通过。

## 4. 返回字段专项验收条件

1. 按应用所需命令、证券/市场、日期/报告期、品种和category列出必需字段，确认单位、日期来源、完整性和真实资格API；没有资格枚举的DTO不能自动视为Verified。
2. 为必需业务字段提供独立参考和原始响应对照，记录节点、参数、获取时点、源码/规则版本及允许误差。财务逐槽核验，报告口径单列；不能凭同证券后续数值接近继续套旧raw_bits证据。
3. 工程回归检查候选/未知不进入默认已核验值、已知财务冲突不通过candidate_value绕过、缺精度不补造价格、原始事件不被筛选丢失、源额不与末价估算混用；改变公开0feb日期或删除不匹配点不能绕过require_trading_date，显式day参考不改写原TCP值。
4. 日期验收比较实际行情日期，区分请求日、本地获取日、响应日期槽及合成标签；非空/240点/空页完成不单独作为证明。0feb编码冲突已在所列样本范围关闭，不能据此将任意请求的日期资格升级为已验收。
5. 检查不完整分页、逐项批量错误、缺失证券与订阅失效不会升级成完整有效结果；通用资源预算前缀不得当作全文件，统计候选字段须另验单位/日期。0547须另在精确交易窗口验证被动推送、状态变化及游标推进，并确认最终快照全部订阅键完整同步、连接/代次一致；本地模拟只证明工程行为。

执行路径和已有依据见[字段证据索引](../reference/RESPONSE_FIELD_SEMANTICS.md#evidence-boundaries)，消费步骤见[应用接入](../development/APPLICATION_INTEGRATION.md#21-字段消费步骤)。新增真实结论写入新证据，不修改冻结材料以满足新检查。

## 5. 上线前关闭条件

1. 固定源码、依赖、二进制及数据库/工具版本，执行目标平台smoke和本地回归。
2. 核实完整日历、规则截止、点时名单、证券状态及公告原字节；Unknown拒绝完整发布。
3. 在隔离目标执行小表、真实范围、重复写入、全字段/全键和总范围核对。
4. 演练确认丢失、worker中断、封存后目录失败、unknown隔离和固定版本读取。
5. 验收实际账户/ACL、许可目录、硬磁盘配额、进程树资源、调度和告警。
6. 完成控制账本/文件及数据库服务端各自备份恢复，测量批准的RPO/RTO及长期指标。

执行方法见[测试说明](VALIDATION.md)及[运维手册](../runbooks/RUNBOOK.md)。未取得的部署信息、长期SLO与未来事实保持待验收状态。

A12新增[任务准入](TASK_ADMISSION.md)和[机器矩阵](../../config/acceptance/task-matrix.json)，
分别约束已结束日线、分钟、快照与0547订阅。工具检查目标同配置、全键字段、资源、
权限、持续负载和批准的RPO/RTO证据封套；默认未知模板拒绝，其他任务没有runner入口。
工具及本机合成回归完成不把上表历史验证提升为当前源码的目标生产结果。
