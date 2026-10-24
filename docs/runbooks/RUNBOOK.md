# 运维手册

更新日期：2026-10-24。业务名单、行情日和可用版本由[运营流程](../operations/OPERATIONS.md)审核；本手册负责部署、资源、故障及恢复。CLI状态以[导入约定](../operations/IMPORTS.md)为准，runner详细步骤见[部署恢复](../../deploy/runner/README.md)。

## 1. 部署前置与安装

| 项目 | 必须准备与验证 |
| --- | --- |
| 源码与二进制 | 固定提交、锁文件、版本及SHA-256；构建Rust1.89及以上 |
| 平台与存储 | runner使用本地Unix持久盘，支持文件锁、SQLite WAL、原子rename及目录fsync |
| 数据与事实 | 显式源、日历、规则核实截止、点时名单、证券历史及公告原字节；Unknown拒绝完整发布 |
| 数据库 | 固定版本/工具；兼容表和runner新命名空间分别准备，账号权限与完整字段/键验证 |
| 预算 | 同机许可目录及一致上限、任务deadline、源/工作空间/行数、硬配额与磁盘余量 |
| 调度与监控 | 专用用户、Job原子更新、服务进程组、日志/textfile、基于业务日历的告警 |
| 恢复 | 控制账本/文件与数据库服务端各自备份；隔离恢复及实际RPO/RTO |

从仓库根构建：

```sh
cargo +1.89.0 build --workspace --release --locked
cargo install --path apps/rustdx-cli --locked
rustdx --no-tcp-startup show -v
```

构建机运行[工程门禁](../testing/VALIDATION.md)，目标机完成[上线验收](../testing/PRODUCTION_ACCEPTANCE.md)。Linux/systemd模板当前仍须实际验证，不能由模板语法通过推导运行通过。

CLI读取进程环境和命令行，runner显式读取严格Job JSON；二者不会自动加载runtime.env。配置入口见[config](../../config/README.md)，ClickHouse/Mongo及systemd见[deploy](../../deploy/README.md)。所有全局参数放子命令前。

## 2. 在线与离线启动

本地文件和运维命令默认不启动行情TCP；--tcp-warmup显式测速完整初始IP池并预热，预热失败阻止本次操作。TCP采集自行核验冻结节点。RUSTDX_TCP_INITIAL_IPS可覆盖通用初始节点，进程许可默认8，同目录多进程使用一致容量。

旧--no-tcp-startup保留并与--tcp-warmup互斥；数据库命令按实际需要连接所选数据库。测试skip变量不控制业务。

```sh
rustdx --no-tcp-startup day /srv/source/lday -o /srv/output/stocks.csv \
  --report /srv/reports/day.json --workers 4
rustdx-runner --no-tcp-startup status --root /var/lib/rustdx
rustdx-runner --no-tcp-startup metrics --root /var/lib/rustdx \
  --output /var/lib/node_exporter/textfile_collector/rustdx.prom
```

runner本地任务可在严格Job中设no_tcp_startup=true，并传给worker。定时器只执行已维护Job，不自动生成下一日事实。调度恢复前先核实Job/source_revision与当前目录。

## 3. 监控与现场保存

同时观察退出码、CLI清单status/stage/artifact/database_status、runner Job/Attempt、目录版本、错误与覆盖，以及数据库完整回读结果。行数/日期边界只用于辅助，不能代替全键全字段验证。

资源观察包含许可数、任务期限、磁盘余量、进程树RSS、阶段累计/活跃时间与样本数；[观测口径](../operations/OBSERVABILITY.md)定义其范围。natural-day freshness不等于交易日覆盖，未结束Attempt及unknown需要单独告警。

排障前保存原始run_id归档、账本事件、请求/响应、query_id、仍存在CSV及日志，核实活跃进程/组和在途数据库请求。不要修改清单状态、删除锁文件或覆盖冻结摘要来解除故障。

## 4. CLI按状态恢复

| 状态 | 核查及动作 |
| --- | --- |
| 没有清单 | 核实参数、路径/锁、来源和启动失败；这些可能早于采集 |
| running/parsing | 核实进程是否活动；未发布中断保留证据，修复后独立重跑 |
| publication_pending | 比较目标/暂存文件SHA-256，确认rename是否完成 |
| published + not_started | 保留CSV，核实表/索引准备与写入前失败；不能据此推断目标不存在 |
| unknown | 核实query_id、服务端在途请求、本次键和值及较新数据；不自动重写 |
| acknowledged | 完整成功回执已有，保留CSV，继续核对全部导出键及字段 |
| verified + cleanup_pending | 核实文件摘要，完成保留/清理，不再写库 |
| complete/failed | 结合具体阶段确认动作；failed不表示数据库回滚 |

中间文件可能仍名为clickhouse或mongodb，-k成功后才改为<output>.csv；以artifact.path和现场为准。shasum -a 256可核实字节。

CH query_log只在具备权限、日志启用时辅助核查，日志缺失不证明未写。CH兼容表用FINAL核对逻辑键；Mongo检查完整(date,code)唯一索引和simple collation。重跑兼容路径前确认源不会覆盖较新数据。

## 5. runner按状态恢复

| 现场 | 操作 |
| --- | --- |
| 活动或不明确worker/进程组 | 由服务管理器停止并确认退出，再recover；不自动杀不明确PID |
| 未登记封存release的中断 | recover隔离并保留输入，不自动重采 |
| 已封存且无不确定DB写入 | recover核验原字节、必要回读及目录提交；不使用变化后的外部来源 |
| DB unknown或某目标未开始 | 保持待核查；recover不重新INSERT |
| 全部目标acknowledged/verified但目录未提交 | 继续精确核验，保留原verified凭据，恢复目录CAS |
| 覆盖失败、旧修订或候选需弃置 | quarantine未发布且无活动worker的Attempt，记录原因；用新release重做 |
| 旧版本数据额外键/字段变化 | 数据库导出拒绝并保留旧输出；固定版本文件仍可核查 |

recover和quarantine不撤改已发布Attempt或正式指针。晚到旧请求只能影响已弃置的旧release；全部目标验证完成前不发布新目录。

## 6. 常见故障定位

| 现象 | 处置 |
| --- | --- |
| AlreadyExists网络配置 | 将一次NetworkPolicy配置移到首次policy/stats/连接之前 |
| 许可等待或超时 | 核实主Client、订阅、批次、池及共享目录；按任务预算释放闲置连接 |
| 无有效IP/全部冷却 | 保存节点统计，核实网络/源服务，降低并发；不无限重试绕过deadline |
| 日历越界或Unknown | 由维护流程补有公告支持的完整事实，不由行情缺失猜状态 |
| 输入日期区间重叠 | 整理为独立可核实输入后重跑 |
| HTTP200末尾异常、子进程确认丢失 | 按unknown现场核查，保存CSV和响应 |
| CH回读长等待 | 查system.processes及线程池；有界查询关闭并行格式化仍需核实服务端容量 |
| 清单保存/清理失败 | 可能已有数据，依据最后检查点和现场恢复，不直接重复写库 |

## 7. 备份、清理和回退

```sh
rustdx-runner --no-tcp-startup backup --root /var/lib/rustdx \
  --destination /srv/backups/rustdx-001
rustdx-runner --no-tcp-startup restore --root /srv/backups/rustdx-001 \
  --destination /srv/restore/rustdx-001
```

backup.json是完整备份标记；恢复要求全新隔离目录及restore-validated.json，通过后再按业务批准流程切换。备份包括已提交WAL、文件、目录及请求证据，数据库服务端数据卷/日志另行备份或新命名空间重建并验收。

数据库全备与控制备份须在停止发布、确认无在途写入的同一固定版本边界取得；数据库原生恢复到空目录后，分别核对所需历史版本的文件与数据库导出、全部键/字段、约束和索引，再恢复调度。不能直接改归档端点/收据或开启覆盖非空表来迁就恢复结果。第六阶段已验证固定Linux两库的静止原生全备空目录恢复；Mongo物理卷/journal/oplog/PITR及生产RPO/RTO仍待验。具体编排见[runner恢复说明](../../deploy/runner/README.md#5-备份与隔离恢复)、[固定版本消费约定](../development/APPLICATION_INTEGRATION.md#10-恢复后的固定版本消费)和[演练证据](../archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)。

清理前确认无活跃任务、未知写入、待发布或待清理产物；锁文件存在不代表锁活动，也不能删除锁文件解除正在持有的锁。缓存、归档和备份按明确保留期清理。

代码/二进制回退、旧catalog版本消费和数据库恢复分别执行。源码回退不会回滚CSV、清单或数据库；使用旧结构源码时按该提交构建，升级路径见[迁移说明](../release/PROJECT_RESTRUCTURE.md)。
