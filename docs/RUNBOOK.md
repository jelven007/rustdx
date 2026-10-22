# rustdx 运维手册

更新日期：2026-10-22；适用仓库源码增强版。数据选择与质量判断见[运营说明](OPERATIONS.md)，清单字段和状态以[导入说明](IMPORTS.md)为准。

## 部署准备

部署机需要Rust1.89及以上；运行已构建CLI不需要编译器。ClickHouse使用HTTP，仓库Compose为25.8，仅绑定回环端口并持久化数据/日志卷。MongoDB导入需要系统mongoimport；当前CLI没有MongoDB URI/认证参数，使用工具默认本机连接，应先核实适配目标环境。

```sh
cargo build --workspace --release --locked
cargo install --path rustdx-cmd --locked
rustdx show -v
cp deploy/clickhouse/.env.example deploy/clickhouse/.env
# 修改.env中的CLICKHOUSE_PASSWORD，再启动
docker compose --env-file deploy/clickhouse/.env \
  -f deploy/clickhouse/compose.yml up -d
docker compose --env-file deploy/clickhouse/.env \
  -f deploy/clickhouse/compose.yml ps
```

`.env`不提交仓库。CLI的 `CLICKHOUSE_URL/USER/PASSWORD` 与Compose服务配置对应，URL默认本机HTTP。Compose端口变量不会自动配置CLI；使用非默认HTTP端口时显式修改CLICKHOUSE_URL。

```sh
export CLICKHOUSE_URL=http://127.0.0.1:8123
export CLICKHOUSE_USER=rustdx
# 从部署环境安全注入CLICKHOUSE_PASSWORD
cargo run --release --example production_smoke
rustdx day ./lday -l sz000001 -o clickhouse -t rustdx.bars_v2 -k \
  --report clickhouse-run.json
```

部署前完成离线门禁，再在隔离小表验证写入、重复导入、FINAL去重及日期/数值。`production_smoke`只验证行情接口，不能替代数据库验收。已有表不兼容时按[迁移说明](MIGRATION.md)备份、建新表、核对后切换，CLI不自动迁移旧表。

任务使用固定工作目录和独立输出/报告名；同一数据库目标的跨主机写入需调度层协调。受管TCP默认8条是单进程预算，不是跨进程/跨机器额度。下载单次最长2小时、数据库单次操作300秒；整条day命令包含下载、解析、DDL及查询，不承诺5分钟任务总上限。

## 监控与核对

| 观测 | 判断和动作 |
| --- | --- |
| 退出码、清单status/errors | 非零或failed进入排障；running先核实进程与归档时间 |
| stage、artifact.state、database_status | 确定发布、写入、清理中断在哪一步，按下表恢复 |
| 行数、代码覆盖、日期边界、日历版本 | 与本次名单及预期行情日比较；过期或异常下降不供下游使用 |
| network_stats | 当前/峰值连接、许可超时、帧失败、端点冷却；由宿主程序采集，CLI未内置监控服务 |
| ClickHouse健康、FINAL计数、query_id | 回执后独立核对逻辑数据；物理重复在合并前可能存在 |
| 文件、清单、临时目录与数据卷空间 | 保证输入/中间CSV/原子替换有余量；保留异常写入证据 |

告警阈值与运行归档期限由实际部署负载确定，当前源码没有内置SLA或告警阈值。清单的logical_keys是已验证输出键数，不能当作数据库查询结果。

## 按状态恢复

先保存最新清单、run_id归档、任务日志及仍存在的CSV，核实是否有活跃进程；不要修改清单来伪造成功。

| 状态 | 核查与恢复 |
| --- | --- |
| 未生成清单 | 核对参数、路径冲突/锁及下载来源解析；这些动作可能早于采集 |
| running / parsing | 核实进程是否还在；异常结束且未发布时，修复输入后重新启动独立任务 |
| artifact.publication_pending | 计算目标文件SHA-256，与清单比较，确认rename是否完成；旧输出可能仍在 |
| artifact.published + not_started | 文件已发布，数据库尚未发起INSERT；保留CSV，核实表准备等失败原因 |
| database_status.unknown | 按query_id及目标逻辑键核对是否部分写入，确认后安排恢复；不直接自动重跑 |
| acknowledged + cleanup_pending | 数据库已回执；核实中间文件和保留文件是否存在、摘要是否一致，处理清理，不再次写库 |
| retained / removed | 核实文件是否符合预期；清单和数据库核对结果继续归档 |
| complete / failed | 结合以上字段判断已经完成的动作；failed不等于数据库回滚 |

核对保留CSV的示例：

```sh
# macOS；Linux可用sha256sum
shasum -a 256 clickhouse.csv
```

数据库回执前失败时中间文件名可能仍为 `clickhouse`，`-k`成功后才更名为 `clickhouse.csv`。实际路径以artifact.path和现场文件为准。

ClickHouse启用query_log且具备权限时，可按清单query_id查服务端记录；日志未出现不能证明没有写入。下列查询中的run_id及表名替换为本次实际值，并限定本次日期/代码范围核对目标库：

```sql
SELECT event_time, type, exception_code, exception
FROM system.query_log
WHERE query_id = 'rustdx-<run_id>'
ORDER BY event_time;

SELECT count(), uniqExact(code), min(date), max(date)
FROM rustdx.bars_v2 FINAL;
```

仅整表count或边界一致仍不能证明逐行正确；进一步按本次CSV核对逻辑键和抽样数值。ClickHouse以最后导入版本替换旧行，重跑前先确认源质量及是否覆盖较新数据。MongoDB须核对 `(date,code)` 唯一索引、目标数据和进程日志，退出失败可能已写入部分行。

## 常见故障

| 现象 | 处置 |
| --- | --- |
| AlreadyExists配置错误 | 在首次Tcp/policy/stats调用前设置一次NetworkPolicy，重启宿主程序应用新策略 |
| WouldBlock/许可超时 | 检查主连接、长期空闲池及并发任务；释放闲置池，按网络预算调整容量；上限1不能支撑主连接加批次 |
| 全部端点冷却/正文超时 | 保存端点统计，降低采集并发，核实出口与服务器；不绕过时限无限重试 |
| 交易所名单鉴权、东财502/TLS断连 | 记录响应错误、低频后续任务或使用已核实本地名单/历史源补数 |
| 日历越界 | 提供完整且已核实外部日历，覆盖查询及前后导航；禁止仅扩大日期范围 |
| 同证券输入区间重叠 | 整理重复源为可核实输入后重跑，不叠加多个覆盖相同区间的目录 |
| HTTP200末尾异常、mongoimport超时 | 按unknown写入恢复流程处理，保留CSV和服务端记录 |
| 清单保存失败 | 即使数据库已回执也可能非零退出；根据旧检查点和现场核实，不能直接重复写库 |

## 备份、清理与回退

保存运行归档、源文件、gbbq/前因子/日历、保留CSV及其SHA-256；数据库使用自身一致性备份。Compose数据卷 `rustdx-clickhouse-data` 和日志卷独立于容器，容器重建不能代替备份。不要以删除数据卷方式处理导入错误。

输入ZIP/.part与解压缓存位于系统临时目录，按来源加锁；清理前确认没有任务使用。系统锁文件存在不代表仍被占用，不通过删除锁文件解除活跃任务锁。未知数据库写入或待发布/待清理状态的产物在核实前保留。

源码回退从[两个历史标签](RELEASES.md)检出后重新构建，部署前运行对应版本的检查。回退二进制不会回滚数据库、CSV或清单，也不会把schema2归档变为旧格式。已知良好二进制、部署配置及数据库备份应另行保存；版本切换后先用独立小任务核实，再恢复正式调度。
