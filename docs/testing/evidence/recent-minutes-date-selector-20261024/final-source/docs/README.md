# rustdx 文档入口

更新日期：2026-10-24。本文档集对应当前工作区源码；当前 Cargo 版本仍为核心1.12.2、CLI1.8.2，不能据版本号把已发布包视为本轮源码。实现、离线回归、真实样本验证和目标生产验收分别记录。

剩余验收最新进展：[0feb日期编码修复与两节点同日对照](archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)；此前的[第六阶段/两库原生全备与空目录恢复](archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)及[第五阶段/RowBinary精度修复、固定数据库与真实TCP对照](archive/development/REMAINING_ACCEPTANCE_STAGE5_20261024.md)继续保留。
旧0feb“8月请求取回6月数据”已定位为ordinal编码错误；修复样本、合成数据恢复与固定数据库环境分别留证，通用日期资格、真实市场完整性和目标生产门禁继续单独判定。

## 按工作职责阅读

| 工作 | 主入口 | 专项参考 |
| --- | --- | --- |
| 产品需求与范围 | [需求与验收条件](requirements/REQUIREMENTS.md) | [日期规则及事实边界](requirements/LIMIT_RULES.md) |
| 系统与工程设计 | [当前架构](architecture/PROJECT_ANALYSIS.md)、[生产技术方案](architecture/PRODUCTION_FUSION_PLAN.md) | [目录职责](architecture/PROJECT_STRUCTURE.md)、[网络预算](architecture/NETWORK.md)、[当日IP池](architecture/DAILY_IP_POOL.md) |
| 应用接入与接口 | [外部应用调用约定](development/APPLICATION_INTEGRATION.md) | [26命令完整参考](reference/COMMAND_API.md)、[返回字段解析状态](reference/RESPONSE_FIELD_SEMANTICS.md)、[checked协议边界](architecture/PROTOCOL_REQUESTS.md) |
| 开发和变更 | [开发说明](development/DEVELOPMENT.md) | [脚本入口](../scripts/README.md)、[配置入口](../config/README.md) |
| 测试和验收 | [验证方法](testing/VALIDATION.md)、[当前验收矩阵](testing/PRODUCTION_ACCEPTANCE.md) | [数据库验收](testing/DATABASE_ACCEPTANCE.md)、[证据说明](testing/EVIDENCE.md) |
| 数据运营 | [运营流程与成功判定](operations/OPERATIONS.md) | [数据维护](operations/DATA_MAINTENANCE.md)、[导入状态](operations/IMPORTS.md)、[观测口径](operations/OBSERVABILITY.md) |
| 部署和故障恢复 | [运维手册](runbooks/RUNBOOK.md) | [部署入口](../deploy/README.md)、[runner部署与恢复](../deploy/runner/README.md)、[运行数据约定](../data/README.md) |
| 发布和升级 | [源码与包发布](release/RELEASES.md)、[行为迁移](release/MIGRATION.md) | [本次目录迁移与删除](release/PROJECT_RESTRUCTURE.md)、[CHANGELOG](../CHANGELOG.md) |
| 历史分析 | [归档索引](archive/README.md) | 原始采样、摘要和日志保存在 [testing/evidence](testing/evidence) |

## 文档维护规则

需求编号 FR-01 至 FR-18 作为追踪锚点。需求描述可观察行为，方案解释组件和取舍，开发说明定位实现，验证文档记录实际检查，运营与运维分别定义业务审核和现场操作。

跨接口消费规则只在应用调用约定中维护；逐命令签名和完整字段类型在接口手册中维护，字段解析/候选/未知状态与验证范围在字段解析状态中维护；清单字段和数据库状态只在导入说明中维护。其他文档引用这些定义，避免复制出多个版本。

当前文档使用稳定文件名。带日期的专项分析和历次结果归入 archive；历史证据的旧路径、绝对采样路径与摘要按[迁移映射](testing/evidence/project-restructure-20261024/migration.json)追溯。历史结果不能替代新源码或新环境验收。

修改行为时同步需求追踪、受影响方案、集成约定、测试及运维说明。新增“已通过”结论须提供范围、命令、退出码、环境、输入来源和跳过项；未取得的市场事实、部署结果与长期指标保持待验收状态。
