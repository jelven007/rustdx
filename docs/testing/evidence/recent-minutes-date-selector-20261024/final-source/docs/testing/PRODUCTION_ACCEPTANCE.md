# 当前验收矩阵

更新日期：2026-10-24。本文区分当前能力、已取得的验证和未关闭的生产门禁。实际命令与环境见所引用证据；当前版本号不能作为源码身份。

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
| 26同码命令 | eltdx3.3.0注册表独立核对，四条现代入口、两节点98帧离线重放 | 全部增强业务契约、市场2全量和完整资源校验 |
| 受管响应身份 | 旧/新收帧、握手/心跳/探测共用校验；9项专项、归档拒绝及448项工作区回归 | 本次没有新增实盘采样，身份回显核验限已保留材料 |
| 0010财务 | 固定平安银行20260815更新样本的10字段独立证据，证券/日期/raw_bits范围匹配及资格回归 | 其余14个命名槽候选、7槽Unknown；其他证券/更新版本及报告口径 |
| 行情/目录/052d/052b | 独立报价与K线样本；第五阶段两节点052d选定12条日线的原帧至CSV、OHLC/金额与官网day精确对照 | TCP跨源成交股数仍差−94至+1股；全品种/周期源量、代码分类权威性、未知状态/尾槽、全市场历史 |
| 0fc5/0fc6事件/金额 | 原始状态/事件/阶段/方向资格、完整枚举与源额/估算分离回归 | ConfirmedByProfile的独立业务核验、全品种价格单位、逐笔完整性及独立盘后源额 |
| F10/板块/资金流/辅助指标 | F10字节范围与GBK、板块下载及分档/指标布局算术已有实现和所列样本 | 文本指标结构化、全文件hash校验、官方分类/资金身份和未知selector/指标窗口 |
| 0feb/0547 | 0feb日期schema2修复；两节点六证券六交易日17,280分钟点与同日0fb4、72次日线开末价对照一致；日期门禁及订阅恢复回归 | 0feb通用日期资格、完整保存窗口与量单位，require_trading_date仍Unsupported；实际盘中主动推送窗口 |
| 日线CLI | 原子发布、schema7、流式RowBinary保住Float64；最终源码在固定Linux两库各两次精确核对779,200历史行，CSV确定性不变 | 当前全包/目标生产配置与全量 |
| runner | 单机覆盖、唯一release、目录CAS、固定版本消费与恢复；Linux31项全执行，真实两库600只模拟证券/1,200行跨512批次 | 真实全市场事实、生产Linux部署、权限/硬配额、长期负载 |
| TCP日线runner | 原始帧、短页/空页、缺键拒绝、源修订及恢复回归；两真实节点各12条选定日线，CSV字节相同 | 最多32只沪深A股/已结束日期；跨源股数精确性、全量、当天及五分钟；此次真实样本未到空页 |
| 控制层备份 | SQLite WAL在线备份、文件/数据库请求证据隔离恢复，Linux真实两库案例也执行 | 目标生产恢复编排、留存与批准的RPO/RTO |
| 数据库原生备份恢复 | 第六阶段固定Linux/两库原生全备恢复至空目录，两个runner版本、每库2,400合成行，全字段/元数据及固定导出一致；错误写入/重复恢复拒绝 | MongoDB物理卷与日志/PITR、持续写入、生产用户/ACL、规模和批准的RPO/RTO |
| 工程目录迁移 | 第五阶段固定源码在macOS相关回归及Linux工作区、fmt/Clippy/文档检查通过；原字节证据随隔离源码输入保留 | GitHub Actions配置未推送/未运行；本轮没有重跑fuzz |

## 3. 证据入口

| 范围 | 报告/机器证据 |
| --- | --- |
| 返回字段语义 | [26命令字段状态及证据索引](../reference/RESPONSE_FIELD_SEMANTICS.md)、[独立财务期望](../../crates/rustdx-core/tests/fixtures/protocol/finance.semantic.expected.json)；本次文档整理不计为新增实盘证据 |
| 26命令补齐 | [实施报告](../archive/validation/ELTDX_3_3_0_26_COMMANDS_IMPLEMENTATION_20261024.md)、[JSON](evidence/eltdx-3.3.0-26-commands-implementation-20261024.json) |
| P0响应正确性 | [修复说明](../archive/development/RESPONSE_IDENTITY_FIX_20261022.md)、[验证JSON](evidence/response-identity-fix-20261022.json) |
| 近期分时与推送 | [0feb边界](../archive/validation/RECENT_MINUTES_PUSH_BOUNDARIES_20261024.md)、[后续非空夹具与窗口报告](../archive/validation/NONEMPTY_FIXTURES_PUSH_WINDOW_20261024.md)、[推送报告](../archive/validation/QUOTE_PUSH_VALIDATION_20261024.md) |
| 0feb日期错位修复 | [修复报告](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)、[原始两节点支持包](evidence/recent-minutes-date-selector-20261024)、[独立日期对照](evidence/recent-minutes-date-selector-20261024/date-selector-verification.json)；旧错位证据及摘要不变 |
| 真实历史/故障 | [第三轮报告](../archive/history/OPTIMIZATION_ROUND3.md)、[历史JSON](evidence/round3-real-day-databases.json)、[故障JSON](evidence/round3-database-faults.json) |
| runner组件 | [实施报告](../archive/development/PRODUCTION_IMPLEMENTATION.md)、[JSON](evidence/runner-implementation-20261023.json) |
| TCP日线闭环与剩余优化 | [第四阶段报告](../archive/development/REMAINING_FIXES_STAGE4_20261024.md)、[JSON](evidence/remaining-fixes-stage4-20261024.json) |
| 最新源码、RowBinary与固定Linux | [第五阶段报告](../archive/development/REMAINING_ACCEPTANCE_STAGE5_20261024.md)、[支持包](evidence/acceptance-stage5-20261024)、[最终源码身份](evidence/acceptance-stage5-20261024/source-pacing-final-manifest.json) |
| 第五阶段实际数据库/环境 | [Linux命令](evidence/acceptance-stage5-20261024/linux/evidence/results.json)、[真实历史](evidence/acceptance-stage5-20261024/linux/evidence/real-history.json)、[环境/二进制](evidence/acceptance-stage5-20261024/linux/evidence/environment.json)、[清理审计](evidence/acceptance-stage5-20261024/linux/evidence/cleanup-audit.json) |
| 服务端原生备份恢复 | [第六阶段报告](../archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)、[最终演练](evidence/acceptance-stage6-20261024/linux/evidence/attempt2/recovery.json)、[输入与日志](evidence/acceptance-stage6-20261024/linux)、[清理审计](evidence/acceptance-stage6-20261024/linux/evidence/cleanup-audit.json) |
| 本次目录重组 | [迁移记录](../release/PROJECT_RESTRUCTURE.md)、[验证JSON](evidence/project-restructure-20261024/verification.json) |
| 早期生产结果 | [历史验收报告](../archive/validation/PRODUCTION_ACCEPTANCE_THROUGH_20261024.md) |

10月10日约2981万行ClickHouse结果属于当时版本；10月23日779,200历史行与runner模拟事实是不同范围。第五阶段分别记录初始源码的macOS实验服务及最终源码的固定Linux镜像执行；按源码身份、命令及实际范围判定，不因相同版本号或旧服务通过推导生产通过。

## 4. 返回字段专项验收条件

1. 按应用所需命令、证券/市场、日期/报告期、品种和category列出必需字段，确认单位、日期来源、完整性和真实资格API；没有资格枚举的DTO不能自动视为Verified。
2. 为必需业务字段提供独立参考和原始响应对照，记录节点、参数、获取时点、源码/规则版本及允许误差。财务逐槽核验，报告口径单列；不能凭同证券后续数值接近继续套旧raw_bits证据。
3. 工程回归检查候选/未知不进入默认已核验值、缺精度不补造价格、原始事件不被筛选丢失、源额不与末价估算混用；改变公开0feb日期不能绕过require_trading_date。
4. 日期验收比较实际行情日期，区分请求日、本地获取日、响应日期槽及合成标签；非空/240点/空页完成不单独作为证明。0feb编码冲突已在所列样本范围关闭，不能据此将任意请求的日期资格升级为已验收。
5. 检查不完整分页、逐项批量错误、缺失证券与订阅失效不会升级成完整有效结果。0547须另在严格交易窗口验证被动推送、状态变化及游标推进；本地模拟只证明工程行为。

执行路径和已有依据见[字段证据索引](../reference/RESPONSE_FIELD_SEMANTICS.md#evidence-boundaries)，消费步骤见[应用接入](../development/APPLICATION_INTEGRATION.md#21-字段消费步骤)。新增真实结论写入新证据，不修改冻结材料以满足新检查。

## 5. 上线前关闭条件

1. 固定源码、依赖、二进制及数据库/工具版本，执行目标平台smoke和本地回归。
2. 核实完整日历、规则截止、点时名单、证券状态及公告原字节；Unknown拒绝完整发布。
3. 在隔离目标执行小表、真实范围、重复写入、全字段/全键和总范围核对。
4. 演练确认丢失、worker中断、封存后目录失败、unknown隔离和固定版本读取。
5. 验收实际账户/ACL、许可目录、硬磁盘配额、进程树资源、调度和告警。
6. 完成控制账本/文件及数据库服务端各自备份恢复，测量批准的RPO/RTO及长期指标。

执行方法见[测试说明](VALIDATION.md)及[运维手册](../runbooks/RUNBOOK.md)。未取得的部署信息、长期SLO与未来事实保持待验收状态。
