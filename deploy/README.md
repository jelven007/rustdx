# 部署与隔离验收

<!-- lifecycle:registry -->
读者：部署/DBA；责任：部署维护者。源码核验2026-10-25，每30日及配置/入口/操作变化复核，现场另验；见[文档治理](../docs/development/DOCUMENTATION_GOVERNANCE.md)。
<!-- /lifecycle -->

rustdx以Rust库、CLI及单机runner交付，任务定时触发、日志/告警接入和持久存储由部署环境管理。本目录提供数据库配置与runner部署模板，用户目标生产仍须独立验收；固定Linux数据库及静止恢复已经取得限定结果。

| 目录 | 用途 | 状态 |
| --- | --- | --- |
| production/clickhouse/ | ClickHouse25.8.33.6 Alpine固定digest、回环端口、账号及持久数据/日志卷 | 部署模板；生产容量与权限须在目标环境核实 |
| production/clickhouse/sql/factor.sql | 按FINAL语义读取最新前因子 | 查询参考，表名须核实；不会被Compose自动执行 |
| acceptance/ | ClickHouse25.8.33.6/MongoDB8.0.16固定digest和资源限制 | 隔离临时存储、18123/27018；固定Linux两库与静止恢复已验证；目标生产另验 |
| [production/runner/](production/runner/README.md) | 独立版本隔离CH表/Mongo集合、systemd Job/metrics timer、告警与备份恢复步骤 | 单机组件已本机回归；Linux启动、权限、硬配额及长期/全量待验收 |

```sh
cp deploy/production/clickhouse/.env.example deploy/production/clickhouse/.env
# 编辑本地.env，设置CLICKHOUSE_PASSWORD
docker compose --env-file deploy/production/clickhouse/.env \
  -f deploy/production/clickhouse/compose.yml up -d
```

CLI的CLICKHOUSE_URL/USER/PASSWORD须另通过进程环境导出；Compose的端口/凭据文件不会自动设置CLI。MongoDB导入还需要PATH中的mongoimport/mongosh及MONGODB_URI，见[配置说明](../config/README.md)。

acceptance仅用于隔离验证，脚本创建随机私有数据库，结果保存JSON与原始日志。数据库CI已配置，固定Linux ClickHouse25.8.33.6/MongoDB8.0.16已完成限定子集与静止全备空目录恢复；macOS实验及固定Linux结果不能替代用户目标全量/长期验收。启动、清理和故障范围见[验收说明](../docs/testing/DATABASE_ACCEPTANCE.md)。

上线前完成目标版本/配置、小表和实际规模重复写入、全键精确回读及恢复演练。备份与源码回退分别管理；容器重建不等于数据恢复，生产卷不能按验收容器清理。操作步骤见[运维手册](../docs/runbooks/RUNBOOK.md)，表迁移见[迁移说明](../docs/release/MIGRATION.md)。

runner使用[独立Job配置](../config/runner/job.example.json)，DBA先创建新命名空间，publisher运行期只查询/插入。完整覆盖、全部目标回执/全字段及总范围核验完成后才提交目录；消费固定版本，unknown不重新INSERT。其在线备份覆盖SQLite WAL和文件/请求证据，不是数据库服务端数据卷备份。具体操作与边界见[runner部署说明](production/runner/README.md)。

限定结果见[当前数据库验收](../docs/testing/DATABASE_ACCEPTANCE.md)和[第六阶段原始报告](../docs/archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)。生产与验收资源的清理规则分别管理。
