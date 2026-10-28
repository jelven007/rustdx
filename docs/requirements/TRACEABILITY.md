# FR需求到生产责任追踪

<!-- lifecycle:registry -->
读者：开发/测试/运营；责任：产品及测试维护者。源码核验：2026-10-28；每90日及FR、源码或验证位置变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

读者为产品、开发、测试与运营维护者；责任为产品及测试维护者。源码核验2026-10-28，每90日及FR/实现/验证入口改变复核。需求行为以[REQUIREMENTS](REQUIREMENTS.md)为权威，此页只维护关系；测试名相对于所属crate/tests。

| 需求 | 实现责任 | 关键验证 | 运营/运维依据 |
| --- | --- | --- | --- |
| FR-01 | core/src/calendar | 日历模块单测、独立交易所逐日对照 | DATA_MAINTENANCE、LIMIT_RULES |
| FR-02 | core/src/limit、CLI/src/cmd/data_check.rs | 日期生效边界/未知状态、截止外状态可用但规则不授予、非法阶段/重叠及公告字节 | LIMIT_RULES、DATA_MAINTENANCE |
| FR-03 | core/src/data、file、tcp/stock | quote_precision、finance_semantics、finance_q3、finance_cash_basis、daily_reference、minute_reference、trade_models、fq | APPLICATION_INTEGRATION、OPERATIONS |
| FR-04 | core/src/operation.rs、pool.rs、tcp | network_budget、tcp_cancel、tcp_startup、reliability、quote_subscription的poll_update取消/撤销/许可释放及发送前半帧同id归属 | NETWORK、RUNBOOK |
| FR-05 | CLI/src/cmd/day、io | cli、day_streaming、cli_deadline | IMPORTS、OPERATIONS |
| FR-06 | CLI/src/io/{sinks,verify,rowbinary,mongo_readback}、runner/src/database | database_acceptance、control_plane | IMPORTS、DATABASE_ACCEPTANCE、RUNBOOK |
| FR-07 | core/src/tcp/stock、file | protocol_decoders、response_identity、command_boundaries、resource_contract、daily_reference、minute_reference、finance_cash_basis | COMMAND_API、RESPONSE_FIELD_SEMANTICS |
| FR-08 | core/src/tcp/stock | 官方名单分页/总量/重复回归 | APPLICATION_INTEGRATION |
| FR-09 | core/src/indicators.rs | 独立公式单测、properties、indicator_error_aliases、indicators bench | OPERATIONS、APPLICATION_INTEGRATION |
| FR-10 | scripts/release、validation、CI | MSRV/三平台、Python、真实包准确版本选择/独立签名及checked-owned消费、真实帧SHA及边界种子/长时fuzz、19个外部用例显式ignored及run_online_tests严格结果 | RELEASES、VALIDATION |
| FR-11 | config/documentation、docs、AGENTS | 文档导航/生命周期、schema源码锚点、FR锚点、目录与保全摘要 | DOCUMENTATION_GOVERNANCE、STRUCTURE_MIGRATION |
| FR-12 | core/src/cache.rs、tcp/stock/client | 缓存模块单测、file_cache固定输入基准、shared_quote_metadata、metadata_hot_path示例 | APPLICATION_INTEGRATION、RUNBOOK |
| FR-13 | core/src/tcp/evidence.rs、CLI/src/tcp_evidence.rs、scripts/release/evidence_manifest.py、validation/audit_minute_archive.py | wire_archive、帧/截断/篡改重放、固定归档安全restore、10月27日poll_update及扩大456次分钟请求原帧独立回放/跨节点差异、独立分钟槽/量额/修订/保存输入回归 | EVIDENCE、RUNBOOK、APPLICATION_INTEGRATION、ADR-0002 |
| FR-14 | CLI/src/manifest、runner/src/observations.rs、ops.rs、pipeline/business.rs、scripts/ops/daily.py | 阶段并集/持久事件/刷新/剩余空间、规则独立剩余期、日历积压/批准逾期/批次保留/独立monitor、逐实例告警夹具 | OBSERVABILITY、OPERATIONS |
| FR-15 | runner/src/pipeline、pipeline/plan.rs、controller、scripts/ops/{daily,production_gate}.py | control_plane缺键/免行情/去重/期限、行数与全部目标预算只拆完整日期、事实SHA绑定、实际runner日调度plan、缺准入禁止recover/run | OPERATIONS、TASK_ADMISSION |
| FR-16 | runner/src/{ledger,pipeline,consumer,storage}.rs | control_plane目录CAS/固定版本/原子导出、九release交错/许可1与8/双消费者共享1、额外总数或错query_id保旧输出 | APPLICATION_INTEGRATION、RUNBOOK |
| FR-17 | runner/src/database、database/capacity.rs | unknown/额外键/重复/版本隔离回归、证明/操作/全部目标证据预算前置拒绝 | RUNBOOK、DATABASE_ACCEPTANCE |
| FR-18 | runner/src/{ops,ledger,storage,observations}.rs | WAL/隔离恢复、consumer-audit共享/排他静止边界与失败审计原字节/损坏拒绝、verify_runner_local分窗恢复、独立留存输入缺口、固定Linux两库静止恢复脚本 | RUNBOOK、OBSERVABILITY、EVIDENCE_RETENTION |

路径中的core和CLI分别指crates/rustdx-core与apps/rustdx-cli，runner指apps/rustdx-runner。实际模块拆分以源码为准，矩阵不复制API或状态合同。

[开发入口](../development/DEVELOPMENT.md)定位代码与命令，[验证方法](../testing/VALIDATION.md)规定检查范围，[当前验收](../testing/PRODUCTION_ACCEPTANCE.md)管理未关闭条件，[运营](../operations/OPERATIONS.md)和[运维](../runbooks/RUNBOOK.md)定义动作。相关指标见[OBSERVABILITY](../operations/OBSERVABILITY.md)，应用共同规则见[APPLICATION_INTEGRATION](../development/APPLICATION_INTEGRATION.md)。

变更时确认本表受影响FR、更新权威文档和有意义的验证，再记录源码/环境/输入与跳过项。既往FR结果不自动授予新源码、未来日期或用户目标生产资格。

2026-10-27的FR-03/04/07/13修复、原始源差异和未关闭边界见[本轮报告](../archive/development/LIVE_MARKET_LEGACY_FIX_20261027.md)；源字段保留、被动推送及完整分钟保存期限不因客户端回归通过而升级。

同日[后续补证](../archive/development/UPSTREAM_CAPABILITIES_FIX_20261027.md)严格解释固定Q3现金差额，修复发送前半帧归属并冻结扩大分钟窗口/跨节点差异；四项FR的当前合同同步核验于2026-10-27，现场能力仍按验收矩阵判断。

2026-10-28的七项生产改进、回归和仍待目标验收的范围见[本轮实施](../archive/development/PRODUCTION_FIXES_20261028.md)；
源码门禁与调度不把缺失的市场资料、生产权限、通知或长期负载填成已通过。
