# rustdx-runner

Rust2024 / MSRV1.89 的同步单机生产控制层。当前采集入口使用明确的本地 `.day` 目录，数据契约固定为 A 股、日线、原价、人民币元及股；worker生成私有文件，发布者完成业务覆盖和可选数据库验收后推进SQLite中的目录指针。该crate版本0.1.0，不发布到crates.io。

## 发布链路

1. 严格JSON登记Job，同一业务身份去重；一个根目录同时只运行一个控制进程。
2. 冻结日历、证券区间、点时名单、公告原字节、day文件与实际执行的CLI二进制。Unknown证券状态在启动worker前拒绝。
3. 受管CLI子进程继承剩余deadline；取消标记及SIGINT/SIGTERM触发协作取消，宽限期后终止独立进程组。
4. 独立流式回读CSV，并重放冻结day的全部范围内记录；数值精确对应，成交量从手转换为整数股。
5. SQLite临时表核对全部应有键、意外键及免行情原因，以确定性证券顺序生成日期分区。
6. 私有目录迁移、文件和目录fsync、只读封存后登记release。全部应有键均由已核实事实免行情时，不启动CLI，清单记录`collector_executed=false`及空分区/免行情数量。
7. 可选数据库按唯一release写入新命名空间。每批512行；传输前持久化请求与账本intent，完整成功响应后才确认，再精确比较全部字段和release总范围。
8. 完整日期分区映射按明确source_revision替换；全部配置目标均verified后，在一个事务中CAS更新目录、指针、任务和事件。目录保存固定数据库核验凭据。
9. 消费者固定catalog_version；文件或数据库导出只在全部选定分区核验完成后原子替换目标CSV。数据库多余键、重复或值变化也会拒绝导出，保留旧输出。

源修订必须增加`source_revision`且不得降低`source_as_of`。扩展历史时，候选的日历须覆盖完整目录日期范围；目录不能出现交易日分区缺口。同一Job身份下改变完整配置会拒绝，需通过明确的策略/源修订登记新Job。

## 命令

```sh
cargo +1.89.0 build -p jelven-rustdx-cli -p rustdx-runner
rustdx-runner run --config /etc/rustdx/job.json
rustdx-runner status --root /var/lib/rustdx
rustdx-runner metrics --root /var/lib/rustdx
rustdx-runner metrics --root /var/lib/rustdx --output /srv/metrics/rustdx.prom
rustdx-runner catalog --root /var/lib/rustdx --dataset a-share-day --version 1
rustdx-runner export --root /var/lib/rustdx --dataset a-share-day --version 1 \
  --from 2026-07-06 --through 2026-07-07 --output /srv/exports/bars.csv
rustdx-runner export-database --root /var/lib/rustdx --dataset a-share-day --version 1 \
  --target clickhouse --from 2026-07-06 --through 2026-07-07 --output /srv/exports/database.csv
rustdx-runner recover --root /var/lib/rustdx
rustdx-runner quarantine --root /var/lib/rustdx --attempt a-example \
  --reason '核查后拒绝此候选'
rustdx-runner backup --root /var/lib/rustdx --destination /srv/backups/rustdx-001
rustdx-runner restore --root /srv/backups/rustdx-001 --destination /srv/restore/rustdx-001
```

配置入口为[严格Job JSON](../config/runner/job.example.json)；数据库由DBA预建[表/索引](../deploy/runner/README.md)，运行期只查询/插入，不执行DDL、upsert或删除。凭据使用JSON中指定的`RUSTDX_`环境变量，worker清空继承环境。ClickHouse使用普通MergeTree，MongoDB使用完整唯一索引；不复用旧平面表。

`recover`对已登记、已封存候选继续核验与目录提交；没有持久release登记的中断Attempt会隔离，不自动重采。数据库unknown或某目标未开始时保持待核查，不重新INSERT；全部目标已确认时可继续精确回读。已verified的原凭据保持不变，恢复核验另存证据，使原目录候选可复用。仍有进程组时须通过服务管理器停止，避免误杀复用PID。`quarantine`只能处理未结束、没有活动worker的候选；已发布Attempt及指针不可撤改。

备份持有同一控制锁，拒绝活动/不明确worker，使用SQLite在线备份API取得已提交WAL内容，同时复制候选、release、catalog和数据库请求/响应。只有最后写出的`backup.json`存在且所有摘要通过，备份才完整。恢复要求全新隔离目录，检查库存摘要、SQLite完整性/外键、全部release、历史目录引用和数据库证据，最终写出`restore-validated.json`；不连接数据库、不自动切换生产目录。这不是数据库服务端数据卷备份。

## TCP留证接口

核心库`Tcp::with_config_and_observer`接受同步`FrameObserver`，从首次握手起记录请求及解压前完整帧，重连保留observer；回调失败使请求失败。CLI库`tcp_evidence::FrameArchive`提供限额、fsync及SHA-256库存，`load_response/replay`核查请求并调用checked解码。示例：

```sh
cargo run -p jelven-rustdx-cli --example tcp_wire -- capture /new/archive 127.0.0.1:7709
cargo run -p jelven-rustdx-cli --example tcp_wire -- replay /archive 3
```

使用实际响应序号重放；截断帧只保存失败事件，不冒充完整帧。当前runner的采集Job仍是本地day，TCP接口供后续采集适配使用。

## 边界与验证

调用者提供已经维护并核实的完整市场事实。格式合法、摘要匹配只能证明材料一致，不能代替公告真实性与全市场名单完整性审核。只有明确核实的区间产生交易/停牌/退市状态，不用行情缺失推断状态。

根目录须位于本地持久Unix文件系统；OS锁及SQLite不是跨主机协调。采集期间监测workspace字节数、预留空间并限制单次源和行数；数据库请求/响应各8MiB，证据另受workspace预算约束。严格磁盘硬限额由目标文件系统配额落实。只读权限保护受管流程；worker与控制层当前同UID，`env_clear`不是恶意同UID程序隔离。数据库INSERT权限本身也不能阻止持有者向已封存release追加；身份、权限和外部写入治理须在部署落实。

回归：

```sh
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked
RUSTDX_TEST_CLI="$PWD/target/debug/rustdx" \
  cargo +1.89.0 test -p rustdx-runner -- --include-ignored
```

`include-ignored`还须设置真实隔离目标的`RUSTDX_TEST_CH_URL/USER/PASSWORD`、`RUSTDX_TEST_MONGO_URI/MONGOSH`。本机已执行16项测试，含真实CLI、600只模拟证券/1,200行两库版本隔离、指针中断恢复、确认丢失和离线恢复。MongoDB当前每个512行逻辑请求启动一次mongosh，存在明显连接/进程成本；全量吞吐量另验收。

实际市场事实、稳定Linux、全量负载和长期SLO须分别验收；单机回归不等于这些门禁已通过。部署与操作见[部署说明](../deploy/runner/README.md)，详细结果见[实施记录](../docs/development/PRODUCTION_IMPLEMENTATION.md)。
