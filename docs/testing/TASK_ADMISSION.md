# 任务准入与目标部署验收

<!-- lifecycle:registry -->
读者：测试/产品；责任：验收维护者。源码核验：2026-10-28；每30日及准入合同或任务范围变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

更新日期：2026-10-28。对应A12、FR-03、FR-04、FR-15至FR-18。消费规则统一以[应用接入](../development/APPLICATION_INTEGRATION.md)为准，已有真实结论以[验收矩阵](PRODUCTION_ACCEPTANCE.md)为准。

## 1. 分开判定任务与环境

[机器矩阵](../../config/acceptance/task-matrix.json)区分`runner_day`、`minute_bar`、`snapshot`、`subscription_0547`。runner只实现已结束沪深A股原价日线。其他三种任务尚无runner执行和发布入口，工具始终报告该缺口；不能通过填入历史日线证据启用它们。

矩阵的历史引用只说明已有范围，不授予新的证券、日期、字段、量单位或交易窗口资格。独立0feb日期参照绑定具体结果，不证明量/均价或合成分钟标签。0547工程恢复通过不替代实际盘中被动推送；最终消费还须使用全部键同步getter。

## 2. 准入请求

复制[部署模板](../../config/acceptance/deployment.example.json)到新的审核目录，填入明确市场、证券、日期/窗口、必需字段，以及应有键集合的总数和SHA。规则包括时间来源、最终性、源修订、断流、溢出和版本发布；不适用的行为写明理由，不能填`unknown`。

身份同时绑定实际源码提交、依赖锁、配置、CLI/runner二进制、目标ID、平台、数据库/工具和事实版本。填入业务批准的进程树RSS、FD、线程、磁盘、阶段延迟、RPO/RTO和持续负载时间；示例没有预设生产SLO。

`runner_day.scope.required_fields`必须非空、无重复且仅包含当前规范CSV的
date/market/code/instrument_class/open/high/low/close/amount_cny/volume_shares；
分钟标签、快照或订阅字段不能通过日线请求取得资格。

```sh
python3 scripts/validation/audit_task_admission.py \
  --request /review/deployment.json --evidence-root /review \
  --output /review/new-admission.json
```

默认模板退出1；它是待填合同，没有部署通过结论。工具不运行采集、数据库写入、部署或通知。

## 3. 目标证据封套

每个请求的`evidence`列出唯一`kind/path/sha256`。九类必需报告分别为目标smoke、事实覆盖、字段语义、全量回读、故障恢复、资源、权限、备份恢复和持续负载。报告是维护者审核封套，绑定真实原日志/命令/输入；现有工具的不同schema结果作为`artifacts`引用，不能把旧报告改写成目标环境结果。

每份封套至少包含：

```json
{
  "schema_version": 1,
  "kind": "target_smoke",
  "task_type": "runner_day",
  "scope": {"与请求scope逐项相同": "实际范围"},
  "identity": {"与请求identity逐项相同": "实际目标身份"},
  "status": "passed",
  "synthetic": false,
  "owner_role": "实际验收负责人",
  "reviewer_role": "独立复核负责人",
  "recorded_at": "实际含时区的获取时间",
  "artifacts": [{"path": "原始日志路径", "sha256": "原始日志摘要"}]
}
```

`field_semantics.qualifications`对矩阵所需每项保存`independently_verified_for_scope`及原始依据；Unknown、结构解码或其他范围资格拒绝。`fact_coverage/full_readback`须有与请求相同的`expected_key_set`、`missing_keys=0/extra_keys=0`，回读另有`all_required_fields_equal=true`。

`fault_recovery.passed_cases`包括停止/重启、确认丢失、worker中断、封存后目录失败、固定旧版本读取。`resources`保存`process_tree_measured/hard_disk_quota_verified=true`及矩阵资源项的`maximums`；超过批准上限拒绝。`access_controls`逐项记录`production_accounts/acl_isolation/allowed_directories/scheduler/alerts`。

`backup_recovery`分别验证`control_ledger_and_files/database_native/fixed_versions_equal`，并保存`measured.rpo_seconds/rto_seconds`，不是用控制账本恢复替代数据库原生恢复。`sustained_load`记录`duration_seconds/completed_jobs/failed_jobs`，达到批准时间、完成任务且没有未关闭失败。

`resources.quota_cases`另逐项证明workspace ENOSPC保指针、consumer-audit ENOSPC保旧输出、
database-evidence ENOSPC拒绝发布。权限报告另证明发布者不能DDL/update/delete、消费者不能写、
worker不能读发布密钥，以及真实通知送达。恢复报告另证明consumer_audit、
shared_quiescent_boundary、account_roles_restored、isolated_target及backup_corruption_rejected。
具体字段见矩阵；缺失或false拒绝，不能以单机合成回归填成真实目标通过。

## 4. 每任务执行门禁

[日调度器](../../scripts/ops/daily.py)先`prepare --settings /etc/rustdx/daily.json`，
输出按实际JSON字节SHA命名的完整日期Job及只读plan；审核方为每个Job在admissions目录提供
`<job-sha256>.json`请求。请求另含实际矩阵`matrix_sha256`和有时区、未来的`valid_until`，
identity包含实际calendar/security_history/universe的`facts_sha256`。
维护者通过[安装身份模板](../../config/runtime/installed-identity.example.json)登记目标、
源码与工具/数据库/事实版本，由部署权限保护；哈希只核字节，版本来源仍需人工复核。

`production_gate.check`在recover前重验九类封套、原日志摘要、Job/两二进制/锁/三事实/平台/
安装版本/矩阵身份，以及准确dataset、来源类型、完整证券/市场/日期与plan全部键摘要。
缺请求、过期或任一变化均保留积压并拒绝recover/run，日调度run返回非零；
recover之后再次核对来源有效期、获取日/修订/完整截止、事实、plan和准入，再执行run。
新日期或输入变化须重新审核，历史样本不批准未来Job。直接runner命令仍由授权操作者按合同执行。

## 5. 结果边界

`admission_denied`及非零退出表示还有缺口。`evidence_contract_complete`只表示所声明范围的审核封套、原始字节、身份和合同一致；摘要及角色字符串不认证来源真实性，也不替代人工复核或运行时qualification getter。工具不据此自动发布数据。

实际部署操作继续按[运维手册](../runbooks/RUNBOOK.md)执行；扩展任务先实现独立入口和消费合同，再取得目标证据。新结果写入新支持包，不扩大或改写冻结历史结论。
