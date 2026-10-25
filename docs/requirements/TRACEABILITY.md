# FR需求到生产责任追踪

<!-- lifecycle:registry -->
读者：开发/测试/运营；责任：产品及测试维护者。源码核验：2026-10-25；每90日及FR、源码或验证位置变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

读者为产品、开发、测试与运营维护者；责任为产品及测试维护者。源码核验2026-10-25，每90日及FR/实现/验证入口改变复核。需求行为以[REQUIREMENTS](REQUIREMENTS.md)为权威，此页只维护关系；测试名相对于所属crate/tests。

| 需求 | 实现责任 | 关键验证 | 运营/运维依据 |
| --- | --- | --- | --- |
| FR-01 | core/src/calendar | 日历模块单测、独立交易所逐日对照 | DATA_MAINTENANCE、LIMIT_RULES |
| FR-02 | core/src/limit | 日期生效边界/未知状态单测 | LIMIT_RULES |
| FR-03 | core/src/data、file、tcp/stock | quote_precision、finance_semantics、trade_models、fq | APPLICATION_INTEGRATION、OPERATIONS |
| FR-04 | core/src/operation.rs、pool.rs、tcp | network_budget、tcp_cancel、tcp_startup、reliability | NETWORK、RUNBOOK |
| FR-05 | CLI/src/cmd/day、io | cli、day_streaming、cli_deadline | IMPORTS、OPERATIONS |
| FR-06 | CLI/src/io/{sinks,verify,rowbinary,mongo_readback}、runner/src/database | database_acceptance、control_plane | IMPORTS、DATABASE_ACCEPTANCE、RUNBOOK |
| FR-07 | core/src/tcp/stock、file | protocol_decoders、response_identity、command_boundaries、resource_contract | COMMAND_API、RESPONSE_FIELD_SEMANTICS |
| FR-08 | core/src/tcp/stock | 官方名单分页/总量/重复回归 | APPLICATION_INTEGRATION |
| FR-09 | core/src/indicators.rs | 独立公式单测、properties、indicators bench | OPERATIONS |
| FR-10 | scripts/release、validation、CI | MSRV/三平台、Python、真实包/独立消费 | RELEASES、VALIDATION |
| FR-11 | config/documentation、docs、AGENTS | 文档导航/生命周期、FR锚点、目录与保全摘要 | DOCUMENTATION_GOVERNANCE、STRUCTURE_MIGRATION |
| FR-12 | core/src/cache.rs、tcp/stock/client | 缓存模块单测、shared_quote_metadata、metadata_hot_path示例 | APPLICATION_INTEGRATION、RUNBOOK |
| FR-13 | core/src/tcp/evidence.rs、CLI/src/tcp_evidence.rs | wire_archive、帧/截断/篡改重放 | EVIDENCE、RUNBOOK |
| FR-14 | CLI/src/manifest、runner/src/observations.rs、ops.rs | 阶段并集/持久事件/刷新/剩余空间回归 | OBSERVABILITY、OPERATIONS |
| FR-15 | runner/src/pipeline、controller | control_plane缺键/免行情/去重/期限 | OPERATIONS、TASK_ADMISSION |
| FR-16 | runner/src/{ledger,pipeline,consumer,storage}.rs | control_plane目录CAS/固定版本/原子导出 | APPLICATION_INTEGRATION、RUNBOOK |
| FR-17 | runner/src/database | unknown/额外键/重复/版本隔离回归 | RUNBOOK、DATABASE_ACCEPTANCE |
| FR-18 | runner/src/{ops,ledger,storage,observations}.rs | WAL/隔离恢复、固定Linux两库静止恢复 | RUNBOOK、OBSERVABILITY |

路径中的core和CLI分别指crates/rustdx-core与apps/rustdx-cli，runner指apps/rustdx-runner。实际模块拆分以源码为准，矩阵不复制API或状态合同。

[开发入口](../development/DEVELOPMENT.md)定位代码与命令，[验证方法](../testing/VALIDATION.md)规定检查范围，[当前验收](../testing/PRODUCTION_ACCEPTANCE.md)管理未关闭条件，[运营](../operations/OPERATIONS.md)和[运维](../runbooks/RUNBOOK.md)定义动作。相关指标见[OBSERVABILITY](../operations/OBSERVABILITY.md)，应用共同规则见[APPLICATION_INTEGRATION](../development/APPLICATION_INTEGRATION.md)。

变更时确认本表受影响FR、更新权威文档和有意义的验证，再记录源码/环境/输入与跳过项。既往FR结果不自动授予新源码、未来日期或用户目标生产资格。
