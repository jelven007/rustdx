# rustdx 文档入口

<!-- lifecycle:registry -->
读者：所有维护者；责任：文档维护者。源码核验2026-10-27，每30日及配置/入口/操作变化复核，现场另验；见[文档治理](development/DOCUMENTATION_GOVERNANCE.md)。
<!-- /lifecycle -->

当前源码版本：<!-- version:project -->2.0.3<!-- /version -->。数字由主线 CI 同步；注册表历史发布事实另列。

更新日期：2026-10-27。本文档集对应当前源码。读者按工作任务进入下表；维护者按[文档治理](development/DOCUMENTATION_GOVERNANCE.md)和[FR追踪矩阵](requirements/TRACEABILITY.md)更新相关材料。

核心与CLI **2.0.0的历史注册表发布**见[发布记录](archive/release/PUBLICATION_20261025.md)。当前版本标记只描述源码，不证明新版本已上传。工程检查、真实源样本、固定数据库演练和用户目标生产分别留证。

当前实现是同步核心库、单次采集CLI和单机runner；完整市场事实由维护流程提供。目录、依赖方向及未实现的多机扩展见[项目结构](architecture/PROJECT_STRUCTURE.md)和[技术方案](architecture/PRODUCTION_FUSION_PLAN.md)。近期修复和历次结果从[归档索引](archive/README.md)追溯；当前待验边界以[生产验收矩阵](testing/PRODUCTION_ACCEPTANCE.md)为准。

2026-10-27实时遗留修复增加显式持续刷新、Q3财务资格与冲突拒绝、日线参考及分钟边界诊断；原始源差异、被动推送和完整分钟保存期限保持事实边界，见[修复报告](archive/development/LIVE_MARKET_LEGACY_FIX_20261027.md)。

同日[上游能力补证](archive/development/UPSTREAM_CAPABILITIES_FIX_20261027.md)解释固定Q3现金差额的汇率口径，修复发送前半帧归属，并独立回放456次分钟请求/43,200点。新增历史源差异及采样下界分别登记，真实被动推送和连续完整保存期限仍待验收。

本轮[官方事实范围复查](archive/development/FACT_SCOPE_REVIEW_20261027.md)取得沪深京原件并保留覆盖缺口；
[独立分钟档案决策](architecture/decisions/0002-minute-archive.md)提供逐轴资格、槽/量额闭合、
修订及保存输入的可执行候选校验。

## 按工作职责阅读

| 工作 | 主入口 | 专项参考 |
| --- | --- | --- |
| 产品需求与范围 | [需求与验收条件](requirements/REQUIREMENTS.md) | [日期规则及事实边界](requirements/LIMIT_RULES.md) |
| 系统与工程设计 | [当前架构](architecture/PROJECT_ANALYSIS.md)、[生产技术方案](architecture/PRODUCTION_FUSION_PLAN.md) | [目录职责](architecture/PROJECT_STRUCTURE.md)、[网络预算](architecture/NETWORK.md)、[当日IP池](architecture/DAILY_IP_POOL.md) |
| 应用接入与接口 | [外部应用调用约定](development/APPLICATION_INTEGRATION.md) | [26命令完整参考](reference/COMMAND_API.md)、[返回字段解析状态](reference/RESPONSE_FIELD_SEMANTICS.md)、[checked协议边界](architecture/PROTOCOL_REQUESTS.md) |
| 开发和变更 | [开发说明](development/DEVELOPMENT.md) | [脚本入口](../scripts/README.md)、[配置入口](../config/README.md) |
| 测试和验收 | [验证方法](testing/VALIDATION.md)、[当前验收矩阵](testing/PRODUCTION_ACCEPTANCE.md) | [数据库验收](testing/DATABASE_ACCEPTANCE.md)、[证据说明](testing/EVIDENCE.md)、[任务准入](testing/TASK_ADMISSION.md) |
| 数据运营 | [运营流程与成功判定](operations/OPERATIONS.md) | [数据维护](operations/DATA_MAINTENANCE.md)、[导入状态](operations/IMPORTS.md)、[观测口径](operations/OBSERVABILITY.md)、[证据留存](operations/EVIDENCE_RETENTION.md) |
| 部署和故障恢复 | [运维手册](runbooks/RUNBOOK.md) | [部署入口](../deploy/README.md)、[runner部署与恢复](../deploy/production/runner/README.md)、[运行数据约定](../data/README.md) |
| 发布和升级 | [源码与包发布](release/RELEASES.md)、[版本管理](release/VERSION_POLICY.md)、[行为迁移](release/MIGRATION.md) | [本次目录迁移](release/STRUCTURE_MIGRATION.md)、[2.0.0发布记录](archive/release/PUBLICATION_20261025.md)、[CHANGELOG](../CHANGELOG.md) |
| 历史分析 | [归档索引](archive/README.md) | 原始采样、摘要和日志保存在 [testing/evidence](testing/evidence) |

## 文档维护规则

需求编号 FR-01 至 FR-18 作为追踪锚点。需求描述可观察行为，方案解释组件和取舍，开发说明定位实现，验证文档记录实际检查，运营与运维分别定义业务审核和现场操作。

跨接口消费规则只在应用调用约定中维护；逐命令签名和完整字段类型在接口手册中维护，字段解析/候选/未知状态与验证范围在字段解析状态中维护；清单字段和数据库状态只在导入说明中维护。其他文档引用这些定义，避免复制出多个版本。

当前文档使用稳定文件名。带日期的专项分析和历次结果归入 archive；冻结证据和历史摘要保持原字节。2026-10-24与本次映射分别保存，见[迁移说明](release/STRUCTURE_MIGRATION.md)。历史结果不能替代新源码或新环境验收。

关键文档登记读者、责任角色、权威定义、最近源码核验日、周期和失效信号。每次PR检查导航、FR锚点、登记覆盖及期限；接口、恢复或部署改变而未同步文档时，按治理规则判为过期。具体操作者和目标环境验收日期由部署方另记，源码核验日不替代现场核验。

修改行为时同步需求追踪、受影响方案、集成约定、测试及运维说明。新增“已通过”结论须提供范围、命令、退出码、环境、输入来源和跳过项；未取得的市场事实、部署结果与长期指标保持待验收状态。
