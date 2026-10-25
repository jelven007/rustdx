# 工程文档治理

<!-- lifecycle:registry -->
读者为需求、开发、测试、运营和运维维护者，责任为文档维护者。源码核验2026-10-25，每90日或生命周期规则变化复核。<!-- /lifecycle -->

此页是任务与维护规则参考，机器权威登记为config/documentation/registry.json。

**每项合同只保留一个权威定义位置。** 需求行为在REQUIREMENTS，跨接口消费在APPLICATION_INTEGRATION，逐命令签名在COMMAND_API，字段资格在RESPONSE_FIELD_SEMANTICS，CLI清单状态在IMPORTS，指标在OBSERVABILITY，现场恢复在RUNBOOK，源码版本事件在VERSION_POLICY。其他材料引用这些位置，历史重复材料明确非当前权威。

| Doc | Diátaxis quadrant (tutorial / how-to / reference / explanation) | Responsibility path | Source of truth | Last verified | Verification cadence | Staleness signal |
| --- | --- | --- | --- | --- | --- | --- |
| [REQUIREMENTS.md](../requirements/REQUIREMENTS.md) | reference | 产品维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/requirements/TRACEABILITY.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [TRACEABILITY.md](../requirements/TRACEABILITY.md) | reference | 产品及测试维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/requirements/REQUIREMENTS.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [PROJECT_STRUCTURE.md](../architecture/PROJECT_STRUCTURE.md) | explanation | 架构维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/architecture/decisions/0001-production-boundaries.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [PROJECT_ANALYSIS.md](../architecture/PROJECT_ANALYSIS.md) | explanation | 架构维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/architecture/PROJECT_STRUCTURE.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [PRODUCTION_FUSION_PLAN.md](../architecture/PRODUCTION_FUSION_PLAN.md) | explanation | 架构维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/requirements/REQUIREMENTS.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [0001-production-boundaries.md](../architecture/decisions/0001-production-boundaries.md) | explanation | 架构维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/architecture/PROJECT_STRUCTURE.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [DEVELOPMENT.md](DEVELOPMENT.md) | how-to | 开发维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/architecture/PROJECT_STRUCTURE.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [APPLICATION_INTEGRATION.md](APPLICATION_INTEGRATION.md) | reference | API维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/reference/COMMAND_API.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [DOCUMENTATION_GOVERNANCE.md](DOCUMENTATION_GOVERNANCE.md) | reference | 文档维护者 / python3 scripts/validation/check_documentation_lifecycle.py | config/documentation/registry.json | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [COMMAND_API.md](../reference/COMMAND_API.md) | reference | API维护者 / python3 scripts/validation/check_documentation_lifecycle.py | crates/rustdx-core/src/tcp | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [RESPONSE_FIELD_SEMANTICS.md](../reference/RESPONSE_FIELD_SEMANTICS.md) | reference | 协议维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/reference/COMMAND_API.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [LIMIT_RULES.md](../requirements/LIMIT_RULES.md) | reference | 规则维护者 / python3 scripts/validation/check_documentation_lifecycle.py | crates/rustdx-core/src/limit | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [VALIDATION.md](../testing/VALIDATION.md) | how-to | 测试维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/requirements/TRACEABILITY.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [PRODUCTION_ACCEPTANCE.md](../testing/PRODUCTION_ACCEPTANCE.md) | reference | 验收维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/testing/EVIDENCE.md | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [DATABASE_ACCEPTANCE.md](../testing/DATABASE_ACCEPTANCE.md) | how-to | 数据库测试维护者 / python3 scripts/validation/check_documentation_lifecycle.py | deploy/acceptance/compose.yml | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [EVIDENCE.md](../testing/EVIDENCE.md) | reference | 测试维护者 / python3 scripts/validation/check_documentation_lifecycle.py | scripts/release/evidence_manifest.py | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [TASK_ADMISSION.md](../testing/TASK_ADMISSION.md) | how-to | 验收维护者 / python3 scripts/validation/check_documentation_lifecycle.py | config/acceptance/task-matrix.json | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [OPERATIONS.md](../operations/OPERATIONS.md) | how-to | 数据运营维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/operations/IMPORTS.md | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [IMPORTS.md](../operations/IMPORTS.md) | reference | 数据导入维护者 / python3 scripts/validation/check_documentation_lifecycle.py | apps/rustdx-cli/src | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [DATA_MAINTENANCE.md](../operations/DATA_MAINTENANCE.md) | how-to | 事实维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/requirements/LIMIT_RULES.md | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [OBSERVABILITY.md](../operations/OBSERVABILITY.md) | reference | 运维维护者 / python3 scripts/validation/check_documentation_lifecycle.py | apps/rustdx-runner/src | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [RUNBOOK.md](../runbooks/RUNBOOK.md) | how-to | 运维维护者 / python3 scripts/validation/check_documentation_lifecycle.py | deploy/production/runner/README.md | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [RELEASES.md](../release/RELEASES.md) | how-to | 发布维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/release/VERSION_POLICY.md | 2026-10-25源码 | 每30日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [VERSION_POLICY.md](../release/VERSION_POLICY.md) | reference | 发布维护者 / python3 scripts/validation/check_documentation_lifecycle.py | scripts/release/versioning.py | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [MIGRATION.md](../release/MIGRATION.md) | how-to | API及发布维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/development/APPLICATION_INTEGRATION.md | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |
| [STRUCTURE_MIGRATION.md](../release/STRUCTURE_MIGRATION.md) | how-to | 发布维护者 / python3 scripts/validation/check_documentation_lifecycle.py | docs/testing/evidence/production-lifecycle-20261025/migration.json | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |

| [NETWORK.md](../architecture/NETWORK.md) | explanation | 网络及协议维护者 / python3 scripts/validation/check_documentation_lifecycle.py | crates/rustdx-core/src/operation.rs | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |

| [DAILY_IP_POOL.md](../architecture/DAILY_IP_POOL.md) | explanation | 网络及协议维护者 / python3 scripts/validation/check_documentation_lifecycle.py | crates/rustdx-core/src/tcp | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |

| [PROTOCOL_REQUESTS.md](../architecture/PROTOCOL_REQUESTS.md) | explanation | 网络及协议维护者 / python3 scripts/validation/check_documentation_lifecycle.py | crates/rustdx-core/src/tcp/stock | 2026-10-25源码 | 每90日及触发事件 | 超过核验周期，或触发变更后尚未同步文档 |

| [config/README.md](../../config/README.md) | how-to | 配置维护者 / python3 scripts/validation/check_documentation_lifecycle.py | config/runner/job.example.json | 2026-10-25源码 | 每30日及配置/入口/操作变化 | 超过核验周期，或触发变更后尚未同步文档 |

| [deploy/README.md](../../deploy/README.md) | how-to | 部署维护者 / python3 scripts/validation/check_documentation_lifecycle.py | deploy/production/README.md | 2026-10-25源码 | 每30日及配置/入口/操作变化 | 超过核验周期，或触发变更后尚未同步文档 |

| [deploy/production/runner/README.md](../../deploy/production/runner/README.md) | how-to | 运维维护者 / python3 scripts/validation/check_documentation_lifecycle.py | apps/rustdx-runner/src/ops.rs | 2026-10-25源码 | 每30日及配置/入口/操作变化 | 超过核验周期，或触发变更后尚未同步文档 |

| [apps/rustdx-runner/README.md](../../apps/rustdx-runner/README.md) | how-to | runner维护者 / python3 scripts/validation/check_documentation_lifecycle.py | apps/rustdx-runner/src/config.rs | 2026-10-25源码 | 每30日及配置/入口/操作变化 | 超过核验周期，或触发变更后尚未同步文档 |

| [docs/README.md](../README.md) | how-to | 文档维护者 / python3 scripts/validation/check_documentation_lifecycle.py | config/documentation/registry.json | 2026-10-25源码 | 每30日及配置/入口/操作变化 | 超过核验周期，或触发变更后尚未同步文档 |

RUNBOOK为operational操作材料；ADR-0001为architectural决策材料。核验日期表示当前源码、配置与检查入口已对照，不代表用户目标部署已经上线。固定Linux数据库结果见验收矩阵，拓扑、依赖、路由、恢复及计划扩展分别在架构/运维材料标明当前与待验状态。

每次PR先确定受影响FR和责任文档，更新权威合同、追踪、实现/测试及操作说明，再执行导航和生命周期检查。新增关键文档必须登记全部字段；超过周期或对应代码、告警、配置、事故触发而未同步时标记stale并停止把它用作当前依据。日期只能随实际对照核验更新。

工具检查周期、字段、路径、重复登记、关键目录遗漏和lifecycle标记；链接/FR由check_documentation.py检查。变更触发语义仍由PR维护者核实，不能声称自动静态检查证明业务准确。登记的责任是工程角色；具体人员和现场核验日由部署交接记录补充。

上线需要当前需求/方案、应用消费、目标验收、运营、指标与恢复；升级还需行为/结构迁移和发布政策。运维RUNBOOK/OBSERVABILITY每30日及事故、阈值/资源/权限变化复核；运营/事实每30日及补数/公告复核。未完成验收不从历史样本升级为生产承诺。

完成阶段报告归archive，原始输入/日志归testing/evidence并保持字节；归档索引及迁移映射保持可追溯。新结果用新证据目录，不更新冻结来源/摘要。干净检出可从README→docs/README→职责入口→配置/检查命令完成导航，CI每个PR执行两项文档检查。
