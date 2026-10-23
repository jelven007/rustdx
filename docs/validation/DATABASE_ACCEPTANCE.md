# 固定数据库配置与故障验收

验收配置位于[deploy/acceptance](../../deploy/acceptance/compose.yml)：Linux ClickHouse 25.8.33.6 Alpine、MongoDB 8.0.16，固定镜像索引SHA-256；2026-10-23已向Docker Hub核对版本tag与digest。ClickHouse的25.8大版本tag现在也固定到该digest，避免下次启动静默改变版本。

验收数据库只绑定本机18123/27018，使用临时容器存储。ClickHouse限制2 CPU、容器2GiB、单查询512MiB/2线程，全局线程池512槽位/最多64个空闲线程；MongoDB限制2 CPU、容器1GiB、WiredTiger缓存0.25GiB。本机实验构建的大批量实测发现128槽位被后台/嵌套线程池占满并阻塞查询，因此提高到仍有上限的512；CLI有界查询额外关闭并行输出格式化，减少附加线程池需求。这是可复现验收配置；生产ClickHouse继续使用[原部署配置](../../deploy/clickhouse/compose.yml)的凭据与持久卷，生产容量须按实际负载测量。

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

这些故障仅作用于测试客户端，不修改系统网络或强制停止共享数据库。SIGKILL记录的是最后一个检查点，不能期待RAII清理或最终新鲜度报告。此验收不覆盖真实服务器重启、磁盘损坏、跨主机互斥、备份恢复或外部写入并发。

真实历史验收：

```sh
curl --fail --location --retry 2 --max-time 600 \
  https://data.tdx.com.cn/vipdoc/hsjday.zip -o /tmp/hsjday.zip
python3 scripts/validation/verify_real_day_database.py --binary target/release/rustdx \
  --zip /tmp/hsjday.zip --output acceptance/real-history.json
```

ZIP由Python zipfile检查所选成员CRC；按名字固定选择前128个sh600历史文件，记录ZIP和每个输入SHA-256，不筛掉质量失败的文件。Python struct独立解码日期/OHLC/金额/量，逐行精确比较CSV，1/4线程字节一致，再在两库各写入两次、独立全键回读并核对最终行数。此子集含历史停牌/退市证券，日历越界与缺口如实告警；不等于全市场证券状态已核实。

[数据库CI](../../.github/workflows/database-acceptance.yml)在Ubuntu24.04/Rust1.89运行固定配置、小表、大批量和故障恢复；手动workflow_dispatch勾选real_zip可增加官方真实历史子集验收，上传全部JSON/日志/归档。当前配置未推送，CI尚未运行；本机没有Docker，实际服务为macOS实验ClickHouse26.11.1.548和MongoDB8.0.16，不能作为稳定Linux镜像或生产环境通过的证据。实测结果见[第三轮记录](../history/OPTIMIZATION_ROUND3.md)。
