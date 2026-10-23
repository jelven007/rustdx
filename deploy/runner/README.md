# 单机 runner 部署与恢复

模板用于 Linux/systemd 的本地 `.day` 采集与版本发布。实际代码仍是同步 I/O、有界线程和子进程；没有跨机协调、跨库事务或内置 Web 服务。模板未在目标 Linux 验收，资源数值是起始配置，须按实际负载调整。

## 1. 准备与配置

从同一提交使用 Rust1.89 或更新兼容版本构建：

```sh
cargo +1.89.0 build --release -p jelven-rustdx-cli -p rustdx-runner
```

部署人员安装两个二进制至 `/opt/rustdx/bin/`，记录 Git 提交和二进制 SHA-256，创建专用 `rustdx` 用户及其持久根目录 `/var/lib/rustdx`。根目录必须使用支持文件锁、SQLite WAL、原子 rename 和目录 fsync 的本地持久 Unix 文件系统；不要放在共享 NFS 上。

复制 [Job 示例](../../config/runner/job.example.json) 到 `/etc/rustdx/job.json`。替换路径、日期、资源预算、来源修订及日历原始证据摘要；示例中的全零摘要必须替换为实际值。日历、证券历史、点时名单及其公告原字节由数据维护流程提供，项目不会生成真实全市场状态。`facts/` 中原字节以小写 SHA-256 命名。路径和字节摘要都须与维护数据一致。

每个 Job 明确 `planned_date`、`source_as_of`、日期范围、`policy_revision` 和 `source_revision`。修订已发布日期时，`source_revision` 必须增加，`source_as_of` 不能降低。扩展历史所用日历须覆盖完整目录。`database_targets: []` 表示只发布文件目录；配置多个目标时，全部验收通过才推进唯一目录指针。

定时器执行已经维护的 Job 文件，不会按墙钟自动制造新的证券事实或来源修订。部署方须在输入维护完成后原子更新 Job；反复执行相同 Job 会去重。假期、来源未更新或事实覆盖不足须由维护流程处理。

## 2. 数据库与凭据

先由 DBA 在隔离命名空间执行 [ClickHouse SQL](clickhouse.sql) 和 [MongoDB 索引/验证器](mongodb.js)，不要复用原平面 `rustdx.bars`。MongoDB 示例通过独立 DBA 凭据环境运行：

```sh
mongosh --nodb --quiet --norc --eval \
  'db = connect(process.env.RUSTDX_DBA_MONGO_URI); load("deploy/runner/mongodb.js");'
```

ClickHouse 表使用普通 MergeTree，无 TTL、异步插入或替换合并；MongoDB 使用 simple collation 和全部契约维度的完整唯一索引。运行期不执行 DDL，不使用 upsert、FINAL 或删除旧数据。每行包含 dataset、release、契约及日期/市场/代码/品种；契约明确日线、原价、人民币元及股。

发布账号仅拥有目标表/集合的 SELECT/find 和 INSERT/insert；消费者使用独立只读账号。DDL、索引、更新、删除和归档清理属于受控 DBA 流程，不向普通发布账号授予。不要与非受管写入者共享发布凭据；实现通过受管发布流程保持版本隔离，数据库的 INSERT 权限本身仍能被持有者用于向任意 release 追加数据。

`/etc/rustdx/publisher.env` 由部署密钥管理注入，按 [变量示例](../../config/runner/publisher.env.example) 命名，仅 root/服务管理器可读。JSON、账本请求和 mongosh argv 不包含凭据值；ClickHouse 使用认证头，MongoDB URI通过私有进程环境传递，mongosh stderr不进入公开日志。worker通过 `env_clear` 只取得固定 PATH 和私有网络目录。

当前子进程仍与控制层使用同一 OS 用户；清空环境防止继承凭据，但不构成对恶意同 UID 程序的权限隔离。更严格的 worker/publisher 身份、存储 ACL 和网络隔离须在目标部署编排并单独验收，不能仅凭这些 systemd 模板宣称已经实现。

## 3. systemd 与指标

安装本目录 `rustdx-runner.service/.timer` 和 `rustdx-metrics.service/.timer`，审核用户、路径、时限、内存、CPU与文件句柄后启用。runner启动前执行 `recover`；unknown 写入会阻断后续任务，避免服务重启变成重写。systemd `KillMode=control-group` 覆盖控制层和采集/数据库子进程；runner自身也管理 worker 的独立进程组。

为文件系统设置实际磁盘配额。采集工作目录和数据库证据各受配置预算监测；工作目录监测允许临时文件在扫描期间消失，最终封存会重计静止目录。周期监测不等于硬配额。示例首次运行预留空间须至少覆盖 workspace 加 `min_free_bytes`，还须为长期 release、账本、数据库证据和备份留容量。

metrics命令可以原子写出 Prometheus textfile：

```sh
rustdx-runner metrics --root /var/lib/rustdx \
  --output /var/lib/node_exporter/textfile_collector/rustdx.prom
```

部署方预先创建 textfile 目录并赋予 `rustdx` 写权限，node_exporter取得读取权限。指标包含 Job 状态、未结束 Attempt、事件数、当前发布时点、刷新时点和剩余磁盘。首次初始化账本后再启用指标定时器。`alerts.yml` 提供待核查、指标失效和预留空间示例；多实例场景须按 job/instance 限定缺失判断，空间阈值须与 Job 配置同步。发布新鲜度 SLO应结合维护日历和应有 Job，不能用周末/假期的自然小时差证明缺失。

## 4. 故障与恢复

| 现场 | 操作 |
| --- | --- |
| worker活动或进程组存在 | 通过服务管理器停止并确认结束，再 `recover`；不对不明确 PID 自动发杀信号 |
| 未登记持久 release 的中断 | `recover` 隔离并保留输入，登记新 Job/Attempt，不自动重采 |
| 文件已封存、目录事务未提交 | `recover` 校验原字节后提交；不读取已变化的外部来源 |
| 数据库全部 acknowledged/verified、目录未提交 | 重新精确回读；已 verified 的凭据保持不变，使原 orphan catalog 可复用 |
| 数据库 unknown、响应确认丢失或某目标尚未开始 | 查账本及 `database/<release>/<target>/` 请求/响应，核查服务器在途请求；恢复不重新 INSERT |
| coverage失败、旧修订、unknown需弃置 | `quarantine --attempt ... --reason ...` 保留唯一候选；登记明确新修订并创建全新 release |
| 数据库多余键或字段被外部修改 | 数据库导出失败并保留旧输出；指针不变，固定版本文件导出仍可核查归档 |

每个写请求在传输/spawn前先 fsync 请求字节并事务登记；成功完整响应持久后才确认。ClickHouse 保存 query_id、状态、服务端返回的 query_id及完整限额内正文；MongoDB保存逻辑请求和完整JSON响应。超过限额、失败或确认丢失均不推进指针。缺失响应可以是进程中断，不能当作目标未写入。

unknown不会仅凭某一时刻行数正确自动转为 verified。运营核查服务器、停止在途写入后，可明确弃置候选并用新 release 重做；晚到的旧请求仍只作用于弃置的唯一 release。正式版本与已发布 Attempt 不可通过 quarantine 撤改。

## 5. 备份与隔离恢复

```sh
rustdx-runner backup --root /var/lib/rustdx --destination /srv/backups/rustdx-001
rustdx-runner restore --root /srv/backups/rustdx-001 --destination /srv/restore/rustdx-001
rustdx-runner export --root /srv/restore/rustdx-001 --dataset a-share-day --version 1 \
  --from 2026-10-22 --through 2026-10-22 --output /srv/exports/restored.csv
```

备份持有控制锁并拒绝活动/不明确 worker，SQLite在线备份包含已提交WAL，同时保留 attempts、releases、catalogs和database证据。最后出现完整 `backup.json` 才是完成标记。恢复要求全新隔离目录，核验全部库存字节、SQLite完整性/外键、所有release及历史目录和数据库证据，再写 `restore-validated.json`。

这备份包含可重建数据的规范文件及控制账本，不是 ClickHouse/MongoDB 服务端数据卷或日志备份。恢复校验不连接数据库、不切换生产目录；数据库灾难恢复由数据库备份流程或明确的新命名空间/Job重建执行，再做精确验收。RTO/RPO、保留期、配额、Linux实际服务启动、数据库权限与长期负载必须在部署环境证明。

MongoDB适配当前每个有界逻辑请求启动一次 mongosh，依次写入/回读512行；内存与请求大小有界，但进程启动和连接成本较高。1,200行模拟数据的本机服务验证不能代替全量吞吐量、稳定版本Linux或长期SLO验收。实施证据与尚未通过的门禁见[实施记录](../../docs/development/PRODUCTION_IMPLEMENTATION.md)。
