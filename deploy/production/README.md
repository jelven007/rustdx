# 生产部署模板

[ClickHouse](clickhouse/compose.yml)为持久数据库模板，[runner](runner/README.md)提供DBA、systemd、指标和恢复操作。账号、容量、硬配额、实际业务范围和目标验收由部署方确认。

隔离验证使用[acceptance](../acceptance/compose.yml)，临时资源清理规则不适用于生产卷。[运维手册](../../docs/runbooks/RUNBOOK.md)
