# 剩余验收第六阶段：数据库原生全备与空目录恢复

日期：2026-10-24。范围：FR-06、FR-18 的数据库服务端备份恢复工程验收。新增可重复运行的[演练脚本](../../../scripts/validation/verify_database_recovery.py)，不修改 Rust 实现；第五阶段冻结输入和结果保持原字节。

最终演练已通过。ClickHouse 原生 `BACKUP/RESTORE DATABASE` 和 MongoDB `mongodump/mongorestore` 在全新容器数据目录恢复两个 runner 发布版本，两库各 2,400 行。两个固定历史版本的文件、ClickHouse 和 MongoDB 导出，在备份前与恢复后完全一致。数据库全部行和元数据也一致，包括 MongoDB `_id`、验证器与完整唯一索引、ClickHouse Float64 位值与表约束。

## 1. 输入与环境

| 项目 | 实际范围 |
| --- | --- |
| Rust 二进制 | 复用第五阶段 Rust 1.89 debug 构建，CLI `c3785001955325b61c8c0b39d0c0a98ead6a37b3912fd4f9d4b99284427b9be2`；runner `62be36f0f800d9e84799dba7361ae6dc088bd7120aa770979036de4d160e903e` |
| 对应 Rust 源码 | 第五阶段最终输入 `d080d6c1ed153150d10637f6dabbff2674baa8091b581f19da30bc64986f68cf`；本轮脚本和部署输入另外冻结 |
| 主机 | Ubuntu 24.04.5、Linux 6.8.0-138、x86_64；独立 Docker 27.5.1 daemon |
| ClickHouse | 25.8.33.6 Alpine，digest `87e0a5b72f5465b18eacca7c76850e7ff551c9795c50e451f5646299e5e24146`；2 CPU、2 GiB，原有有界线程/查询配置 |
| MongoDB | 8.0.16，digest `4b58ebcb1dc7a7b4e84cd8ce9098d48764ae4478876898ff9551acf2ac4a6a6d`；2 CPU、1 GiB，WiredTiger 缓存 0.25 GiB |
| Mongo 工具 | Database Tools 100.12.2、mongosh 2.5.10；工具实际版本、镜像 inspect 与容器 inspect 均留存 |
| 数据 | 明确合成的 600 只 sh600000 至 sh600599，2026-07-06/07 两日；两次 source_revision，close 从 10 到 10.5，amount 与 volume 随证券变化 |
| 写入方式 | DBA 预建独立随机命名空间，真实 runner 发布两个 release，每个版本 1,200 行，跨 512 行有界批次 |
| 网络与身份 | 数据库仅监听随机 loopback 端口；独立 daemon 禁用 bridge/iptables，临时容器与随机库由脚本独占；以root读取停止的容器UID数据文件库存，未测试生产账户或 ACL |

二进制的上一阶段来源见[原环境记录](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/environment.json)，本轮对应检查见[二进制来源核对](../../testing/evidence/acceptance-stage6-20261024/binary-provenance.json)。新脚本的确切字节保存在[最终输入快照](../../testing/evidence/acceptance-stage6-20261024/linux/evidence/attempt2/source/manifest.json)。

## 2. 实际恢复流程

1. 构造并独立解码两份 `.day` 输入、合成日历/证券历史/点时名单，运行两次真实 runner；固定 v1、v2 分别导出文件和两库数据，独立比较全部键与六个数值字段。
2. runner 退出后保持写入静止，在 catalog v2 边界备份控制账本/文件，再执行同步 ClickHouse 原生 ZIP 全备及 MongoDB BSON/metadata 全备。所有原生备份另建文件清单和 SHA-256。
3. 审计并停止、移除原容器，保留其原数据目录；新容器使用从未写入的另一组空目录和相同 loopback 端口。恢复前确认 ClickHouse 数据库不存在、MongoDB 没有集合，不执行原数据目录挂载或业务重新 INSERT。
4. 执行原生恢复和控制层隔离恢复。原请求证据仍指向相同数据库身份，从恢复的账本固定消费 v1、v2；检查文件/数据库 CSV 字节、全库行、ClickHouse 浮点位值、MongoDB `_id` 与索引/验证器。
5. 对恢复的库执行约束与重复恢复拒绝检查，再次核对全库快照。原生备份和已经停止的旧数据目录摘要保持不变。

这是停止写入后的完整数据库备份。MongoDB 使用 BSON 逻辑备份，没有使用 `--oplog`，没有演练 WiredTiger 物理卷、journal/oplog 回放、持续写入的一致性或任意时点恢复。控制层备份依旧不内置数据库服务端备份。

## 3. 验收结果与时间

| 检查 | 最终结果 |
| --- | --- |
| 两个发布版本 | 每个版本 1,200 个唯一键；两库物理行数各 2,400，两个 release 完整 |
| v1 六份导出 | 文件/CH/Mongo，备份前后全部 SHA-256 `77c979f92364564363ceedd7b6c10b4056d070edbf7808c3b39ec9a762f76224` |
| v2 六份导出 | 文件/CH/Mongo，备份前后全部 SHA-256 `bf59f743e51e3ae8a0f5c5eb2754eb3456a45a3520e6ab540dd27d7a9629cd4c` |
| ClickHouse 元数据 | MergeTree、完整排序键、contract/identity/share_count 约束及所有数值位值相同 |
| MongoDB 元数据 | 全部 `_id`、业务字段、simple collation、validator 和索引相同 |
| 约束拒绝 | ClickHouse 错契约/品种/非整数股数 HTTP 500；MongoDB 错契约 code 121、重复完整键 code 11000 |
| 备份篡改 | 副本 ZIP 修改后库存核验失败，未启动该副本的恢复；本地另验证多余文件和缺失文件拒绝 |
| 重复恢复 | ClickHouse code 608/CANNOT_RESTORE_TABLE；MongoDB `--stopOnError` 退出 1；复查全部行/元数据没有改变 |
| 原目录保全 | 停止的原服务器数据目录恢复前后 SHA-256 清单相同 |
| 恢复计时 | 从启动空目录新服务至控制恢复、两版本六份导出与全库元数据精确核验完成：5.809584 秒 |
| ClickHouse 全备 | 同步全备命令约 0.004857 秒；属于小合成库 |
| 命令结果 | 最终记录 71 条工具命令/SQL 请求，含 5 条预期拒绝；全部满足各自预期，脚本结果 passed |

机器结果见[最终 recovery.json](../../testing/evidence/acceptance-stage6-20261024/linux/evidence/attempt2/recovery.json)。时间没有包含故障发现、人员响应、备份传输、生产规模与切换审批，也没有包含后续拒绝测试和清理；不能作为生产 RTO。写入静止期间恢复边界内的两版本没有丢失行，未据此声明业务批准的 RPO 为零。

首次演练的数据和元数据恢复也一致，但最后把 ClickHouse 错误匹配为字面 `non-empty`；25.8.33.6 实际返回 `Code: 608` 和 `already contains some data`，导致脚本失败。修正为核对该错误码和语义后完整重演，追加 MongoDB 重复恢复检查。首次[失败记录](../../testing/evidence/acceptance-stage6-20261024/linux/evidence/attempt1/recovery.json)、输入脚本和错误正文全部保留，没有修改旧结果使它通过。

## 4. 清理与证据

两个演练分别清理自建容器，没有剩余容器或匿名卷；最终独立 daemon 已停止，socket 已移除，没有该运行目录的 containerd 进程。审计见[cleanup-audit.json](../../testing/evidence/acceptance-stage6-20261024/linux/evidence/cleanup-audit.json)。

[Linux 支持包](../../testing/evidence/acceptance-stage6-20261024/linux)包含两次输入脚本、原生 ZIP/BSON/metadata 备份、控制层完整备份、恢复后控制文件、全部导出/快照、逐命令正文、镜像/容器信息及清理记录。[传输清单](../../testing/evidence/acceptance-stage6-20261024/support-manifest.json)标明包含文件的大小/摘要，以及未回传的停止后数据库卷文件；其完整摘要库存已回传。工具临时连接配置在运行结束后删除，未回传凭据文件。

本轮修改 Python、CI配置和文档，验证脚本语法、库存失败保护及真实容器演练；没有改 Rust，也没有重跑第五阶段已经完成的 Cargo 回归。[数据库workflow](../../../.github/workflows/database-acceptance.yml)已增加原生恢复步骤，使用sudo读取自建服务器的停止后文件库存，显式指定工具路径；上传前开放合成证据读取并排除原始服务器卷目录。该配置未推送，GitHub Actions尚未运行。新支持包、二进制来源、原生库存、CI语法与文档引用的本地验证见[verification.json](../../testing/evidence/acceptance-stage6-20261024/verification.json)。

## 5. 仍待验收

本轮关闭的是所列版本、合成范围、静止写入条件下的服务端原生全备空目录恢复。生产主机/账号/ACL、硬磁盘配额、长期负载/SLO、真实全市场事实、生产规模和批准的 RPO/RTO 仍待验；物理 MongoDB 卷与日志/PITR 也未覆盖。

返回字段剩余项继续按[当前验收矩阵](../../testing/PRODUCTION_ACCEPTANCE.md)处理：TCP 跨源成交股数差异、0547 实际盘中推送、0feb 响应交易日冲突、2027 官方日历及其他命令/证券范围的独立业务核验。日期或窗口不足不能靠合成数据升级为真实验收。
