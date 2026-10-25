# 固定数据库配置与故障验收

<!-- lifecycle:registry -->
读者：测试/DBA；责任：数据库测试维护者。源码核验：2026-10-25；每30日及数据库/工具或恢复配置变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

验收配置位于[deploy/acceptance](../../deploy/acceptance/compose.yml)：Linux ClickHouse 25.8.33.6 Alpine、MongoDB 8.0.16，固定镜像索引SHA-256；2026-10-23已向Docker Hub核对版本tag与digest。ClickHouse的25.8大版本tag现在也固定到该digest，避免下次启动静默改变版本。

验收数据库只绑定本机18123/27018，使用临时容器存储。ClickHouse限制2 CPU、容器2GiB、单查询512MiB/2线程，全局线程池512槽位/最多64个空闲线程；MongoDB限制2 CPU、容器1GiB、WiredTiger缓存0.25GiB。本机实验构建的大批量实测发现128槽位被后台/嵌套线程池占满并阻塞查询，因此提高到仍有上限的512；CLI有界查询额外关闭并行输出格式化，减少附加线程池需求。这是可复现验收配置；生产ClickHouse继续使用[原部署配置](../../deploy/production/clickhouse/compose.yml)的凭据与持久卷，生产容量须按实际负载测量。

需要Docker Compose及PATH中的mongoimport 100.12.2、mongosh 2.5.10。Rust最低1.89：

```sh
docker compose -f deploy/acceptance/compose.yml up --wait --wait-timeout 180
cargo build -p jelven-rustdx-cli --release --locked
export CLICKHOUSE_URL=http://127.0.0.1:18123
export MONGODB_URI=mongodb://127.0.0.1:27018
RUSTDX_DB_ACCEPTANCE=1 cargo test -p jelven-rustdx-cli --test database_acceptance --locked
python3 scripts/bench/measure_database_readback.py --binary target/release/rustdx --output acceptance/bulk.json
python3 scripts/validation/verify_database_faults.py --binary target/release/rustdx --output acceptance/faults.json
docker compose -f deploy/acceptance/compose.yml down --volumes
```

小表验收核对重复/覆盖、6/8个数值字段、复权、CSV留存/删除、ClickHouse错误键及Mongo已有重复记录。65,536行脚本重复写入隔离表，回读全部导出键/字段并保存过程资源。所有脚本用随机私有数据库名，不接收生产表名；结束删除自己的数据库。输入、运行清单归档和日志保存在输出JSON注明的私有临时目录，不自动删除证据。

故障脚本通过自己的TCP代理转发到真实本机服务：

- 网络中断：在较大写请求后的服务端响应处断开连接并拒绝重连。可产生部分写入或完整写入但确认丢失；要求已有真实行、清单failed/unknown、已发布CSV保留。
- 强制终止：ClickHouse在写后响应处保持阻塞；Mongo先让真实mongoimport成功，再暂停包装进程，阻断它向rustdx传递退出码。对rustdx发SIGKILL，要求running/unknown、CSV保留；清理自己的暂停包装进程。
- 恢复：直接连原服务，使用同一输出/表重跑，证明进程死亡释放同机锁；全部8,192键及数值精确回读、逻辑行数不增加、CSV摘要不变，并保留前后两份run_id归档。

这些故障仅作用于测试客户端，不修改系统网络或强制停止共享数据库。SIGKILL记录的是最后一个检查点，不能期待RAII清理或最终新鲜度报告。客户端故障验收不覆盖真实服务器重启、磁盘损坏、跨主机互斥、备份恢复或外部写入并发；服务端原生全备恢复另见下列第六阶段记录。

真实历史验收：

```sh
curl --fail --location --retry 2 --max-time 600 \
  https://data.tdx.com.cn/vipdoc/hsjday.zip -o /tmp/hsjday.zip
python3 scripts/validation/verify_real_day_database.py --binary target/release/rustdx \
  --zip /tmp/hsjday.zip --output acceptance/real-history.json
```

ZIP由Python zipfile检查所选成员CRC；按名字固定选择前128个sh600历史文件，记录ZIP和每个输入SHA-256，不筛掉质量失败的文件。Python struct独立解码日期/OHLC/金额/量，逐行精确比较CSV，1/4线程字节一致，再在两库各写入两次、独立全键回读并核对最终行数。此子集含历史停牌/退市证券，日历越界与缺口如实告警；不等于全市场证券状态已核实。

[数据库CI](../../.github/workflows/database-acceptance.yml)配置在Ubuntu24.04/Rust1.89运行固定配置、小表、大批量和故障恢复；手动workflow_dispatch勾选real_zip可增加官方真实历史子集验收，上传全部JSON/日志/归档。当前配置未推送，GitHub Actions尚未运行。第三轮的实际服务为macOS实验ClickHouse26.11.1.548和MongoDB8.0.16，见[第三轮记录](../archive/history/OPTIMIZATION_ROUND3.md)；第五阶段另外在隔离Linux执行固定镜像，不能把两种环境混作一次验收。

2026-10-24增加runner独立版本化目标的DBA预建步骤，并显式执行 `cargo test -p rustdx-runner --test control_plane -- --include-ignored --nocapture --test-threads=1`。该步骤设置全部 `RUSTDX_TEST_*`，不存在缺环境提前跳过；同时配置 `tests/database_mongo_protocol.cjs` 的离线JavaScript检查。第五阶段的隔离Linux已实际执行完整31项runner回归，其中19项control_plane，包含真实两库600只合成证券/1,200行跨512批次、两release、篡改拒绝及控制层恢复；证券状态输入仍是模拟事实。

CLI 的mongosh认证URI通过环境变量读取；mongoimport使用0600临时JSON/YAML配置文件，在子进程结束后清理。URI不出现在argv，验收固定工具版本需要支持 `mongoimport --config`。

第五阶段修复三个脚本的`--no-tcp-startup`参数，使任务预算用于指定数据库动作。稳定Linux ClickHouse的CSVWithNames把low=6.94改变一位Float64；`precise_float_parsing=1`也未修正。CLI改为有界流式RowBinary，保留显式列、表内默认version、query_id及精确回读。坏日期/字段/证券/数值或超过1024字节的输入行拒绝；CSV原字节与原失败报告保留，未用容差掩盖偏差。

最终输入身份为`d080d6c1ed153150d10637f6dabbff2674baa8091b581f19da30bc64986f68cf`，1,048文件/538份运行期证据；Rust1.89在Ubuntu24.04.5、独立Docker27.5.1 daemon、固定ClickHouse25.8.33.6/MongoDB8.0.16镜像digest及原容器限额运行。工作区484 passed/9 ignored、文档66 passed/18 ignored，格式/Clippy/严格文档通过；环境提前返回与公网忽略另记，不能全部称为真实服务测试。65,536行两库各两次全键verified，四项中断同表恢复8,192键精确回读。

779,200行官方原字节历史子集也已完成两库各两次全键/全数值精确回读，1/4线程和全部数据库导出CSV摘要仍为`80f1b74671bcc1d8d3aa5bf7693e5dbacdcf04c60f40300255f23694fffa46e5`。最终18条命令退出0，私有数据库、容器、匿名卷及daemon均已核实清理，见[实际结果](evidence/acceptance-stage5-20261024/linux/evidence/results.json)、[真实历史](evidence/acceptance-stage5-20261024/linux/evidence/real-history.json)及[清理审计](evidence/acceptance-stage5-20261024/linux/evidence/cleanup-audit.json)。各次浮点偏差/隔离输入/时序测试失败仍留存。

完整过程、真实历史最终结果、实际临时systemd范围和清理审计见[第五阶段报告](../archive/development/REMAINING_ACCEPTANCE_STAGE5_20261024.md)及[Linux支持包](evidence/acceptance-stage5-20261024/linux)。Linux采用debug二进制做功能验收，不作为release性能、正式生产模板安装、权限/配额/SLO或数据库服务端卷恢复通过。

## 服务端原生全备与空目录恢复

第六阶段新增[恢复演练脚本](../../scripts/validation/verify_database_recovery.py)。要求Linux本地Docker，使用固定digest的自建容器、随机loopback端口/数据库；不接收已有服务或生产表名。MongoDB还需要mongodump/mongorestore 100.12.2及mongosh 2.5.10：

```sh
cargo +1.89.0 build --locked -p jelven-rustdx-cli -p rustdx-runner
sudo python3 scripts/validation/verify_database_recovery.py \
  --cli target/debug/rustdx --runner target/debug/rustdx-runner \
  --docker "$(command -v docker)" --mongosh "$(command -v mongosh)" \
  --mongodump "$(command -v mongodump)" --mongorestore "$(command -v mongorestore)" \
  --output acceptance/recovery-001/recovery.json
```

停止后的数据库文件由容器UID拥有，计算其完整SHA库存需要读取权限；本次Linux以root执行，CI配置使用sudo及显式工具路径。CI上传原生/控制备份、快照及日志，排除原始服务器卷目录；普通发布账号的生产权限另验收。

脚本用600只明确合成证券/两日期发布两个真实runner版本，两库各2,400行。写入静止后执行控制层备份、ClickHouse原生ZIP全备和MongoDB BSON/metadata全备，停止原容器，在另一组空数据目录恢复。恢复前验证原生库存SHA-256及空目标，Mongo连接配置0600且结束删除；不用`allow_non_empty_tables`或`--drop`覆盖已有目标。

恢复后固定v1/v2的文件及两库六份CSV与恢复前逐字节一致；全部数据库行、ClickHouse Float64位值/约束、MongoDB `_id`/验证器/完整唯一索引也相同。约束拒绝错误写入；重复ClickHouse恢复返回code608，MongoDB重复恢复以`--stopOnError`退出1；再次核验数据和元数据未改变。最终71条工具命令/SQL请求全部满足预期，5条为主动拒绝检查。静止场景的新服务启动至完整核验约5.81秒，不是生产RTO。

实际[结果](evidence/acceptance-stage6-20261024/linux/evidence/attempt2/recovery.json)、[清理审计](evidence/acceptance-stage6-20261024/linux/evidence/cleanup-audit.json)、[完整报告](../archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)和[支持包](evidence/acceptance-stage6-20261024/linux)分别留证，首次错误措辞判定失败的输入/日志也保留。复用第五阶段已固定的Rust二进制，未重新执行Cargo回归。原生备份、控制备份与导出回传入仓库；停止的原数据库卷留在远端独立运行目录，其SHA库存保存在支持包。

此范围是静止写入的原生完整备份恢复。MongoDB为逻辑BSON备份，没有物理WiredTiger卷、journal/oplog回放或PITR；生产规模、持续写入一致性、用户/角色/ACL和批准的RPO/RTO继续待验。[数据库workflow](../../.github/workflows/database-acceptance.yml)已增加原生恢复步骤及支持包上传配置，配置未推送，GitHub Actions尚未运行。
