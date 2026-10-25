# rustdx-runner

当前runner源码版本：<!-- version:runner -->2.0.1<!-- /version -->；由主线CI同步，publish=false。

<!-- lifecycle:registry -->
读者：开发/部署；责任：runner维护者。源码核验2026-10-25，每30日及配置/入口/操作变化复核，现场另验；见[文档治理](../../docs/development/DOCUMENTATION_GOVERNANCE.md)。
<!-- /lifecycle -->

Rust2024 / MSRV1.89 的同步单机生产控制层。采集支持明确的本地 `.day` 目录或有总预算的显式 TCP 日线来源，数据契约固定为 A 股、日线、原价、人民币元及股；worker生成私有文件，发布者完成业务覆盖和可选数据库验收后推进SQLite中的目录指针。该crate版本0.1.0，不发布到crates.io。

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
rustdx-runner migrate --root /var/lib/rustdx
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

配置入口为[严格Job JSON](../../config/runner/job.example.json)；数据库由DBA预建[表/索引](../../deploy/production/runner/README.md)，运行期只查询/插入，不执行DDL、upsert或删除。凭据使用JSON中指定的`RUSTDX_`环境变量，worker清空继承环境。ClickHouse使用普通MergeTree，MongoDB使用完整唯一索引；不复用旧平面表。

`status/metrics/catalog`及文件消费只打开已有账本，不创建或迁移。空目录和不兼容schema返回错误；初始化/迁移使用`migrate`，写入/迁移由根目录控制锁排他保护。库调用方用`Ledger::open_existing_readonly`查询，持有`RootLock`后用`open_writer`写入；旧`open`在初始化/迁移期间自行取得锁。SQLite只读连接仍可能需要WAL/shm辅助文件，不是不可变快照；严格零写巡检采用留存工具的私有WAL快照。

数据库回读按 `(contract_version,date,market,code,instrument_class)` 完整键继续，dataset/release 为索引的相等前缀；单日读取加日期过滤。MongoDB每目标会话复用一个mongosh，每请求确认query_id，结束时检查EOF与退出状态；半帧、超限、取消和超时终止整个进程组。全字段比较、release总数与终止空查询继续防止缺键、多余键和重复数据被接受。

首次数据库请求前按封存行数与JSON字节预测发布及一次恢复的I/O、证明和证据预算，全部目标共同检查release目录，不满足则在写入前拒绝。预测另存`database/<release>/capacity-*.json`。发布命名空间仍限制50,000次I/O、200,000目录条目及32MiB证明；重复恢复受剩余预算限制。消费查询使用独立命名空间，不再消耗发布`target_io`。

`PinnedDataset`内部catalog及release清单固定，只提供`catalog()`共享只读访问。一次数据库导出按release复用会话，在日期顺序输出时逐键逐字段核对，每日终止空页仍检查，结束时每release检查一次完整总数。查询凭据写入`consumer-audit/q-<timestamp>-<pid>/<release>/<target>`；每请求保存request/response及SHA收据，只有全部回读核验成功才写`verified.json`，它不表示CSV已经原子提交。独立会话最多50,000次请求、200,000条目，字节上限取所选release冻结`max_workspace_bytes`的最小值；不取得控制锁，不修改账本。查询审计不随控制层备份复制，留存预览暂整体保护该目录，保留期限和总磁盘硬配额由部署方另行制定；不自动删除。

账本当前schema4。`migrate`在根锁内从原观测重建指标汇总并增加Job时间/Attempt归属索引；原JSON记录保留且只追加。`metrics`读取汇总，unavailable与缺失记录分别报告。`rustdx-runner rebuild-metrics --root /var/lib/rustdx`在事务内重建，异常或溢出回滚；原schema3备份可原字节恢复，恢复后须显式`migrate`才能运行当前查询。迁移前保留原备份，回退使用旧二进制与旧备份，不手改版本号。

归一化在输入固定后只计量SQLite/journal/WAL/shm、coverage及partitions等变化部分；最终完整扫描与摘要封存继续执行，`release.json`也计入总字节预算。

runner本地day及管理命令默认不启动行情TCP。--tcp-warmup显式测量初始池、
建立TCP客户端并验证0004心跳；TCP来源自行核验冻结节点。三轮、8并发、阶段3秒，
每天北京时间09:00刷新，策略和集成API见[启动与当日IP池](../../docs/architecture/DAILY_IP_POOL.md)。
`--network-limit`默认8，`--network-dir`或`RUSTDX_NETWORK_DIR`指定共享许可目录。
`RUSTDX_TCP_INITIAL_IPS`以逗号分隔地址覆盖40个初始节点；这些地址和许可策略
会显式传给清空环境的采集子进程，父进程在spawn前释放预热连接。
离线状态可直接使用`rustdx-runner status --root ...`；
run的严格JSON可设置`"no_tcp_startup": true`，也可使用命令行开关，
该设置同时传给采集子进程并与显式预热互斥。默认false；`--help`直接退出而不初始化网络。

`recover`对已登记、已封存候选继续核验与目录提交；没有持久release登记的中断Attempt会隔离，不自动重采。数据库unknown或某目标未开始时保持待核查，不重新INSERT；全部目标已确认时可继续精确回读。已verified的原凭据保持不变，恢复核验另存证据，使原目录候选可复用。仍有进程组时须通过服务管理器停止，避免误杀复用PID。`quarantine`只能处理未结束、没有活动worker的候选；已发布Attempt及指针不可撤改。

备份持有同一控制锁，拒绝活动/不明确worker，使用SQLite在线备份API取得已提交WAL内容，同时复制候选、release、catalog和数据库请求/响应。只有最后写出的`backup.json`存在且所有摘要通过，备份才完整。恢复要求全新隔离目录，检查库存摘要、SQLite完整性/外键、全部release、历史目录引用和数据库证据，最终写出`restore-validated.json`；恢复操作本身不连接数据库、不自动切换生产目录，入口默认离线。这不是数据库服务端数据卷备份。

备份/恢复解析已有父目录的实际路径，拒绝目标落入源目录或包含`..`；`max_bytes`包含清单/完成标记。新SQLite备份为独立DELETE模式快照，恢复校验无需额外WAL。恢复标记记录`original_roots`，历史账本绝对路径保持原字节；留存预览据此映射`attempts/<id>`，并验证Receipt的proof及其I/O摘要，不改写旧证据。

## TCP留证接口

核心库`Tcp::with_config_and_observer`接受同步`FrameObserver`，从首次握手起记录请求及解压前完整帧，重连保留observer；回调失败使请求失败。CLI库`tcp_evidence::FrameArchive`提供限额、fsync及SHA-256库存，`load_response/replay`核查请求并调用checked解码。示例：

```sh
cargo run -p jelven-rustdx-cli --example tcp_wire -- capture /new/archive 127.0.0.1:7709
cargo run -p jelven-rustdx-cli --example tcp_wire -- replay /archive 3
```

使用实际响应序号重放；截断帧只保存失败事件，不冒充完整帧。`visit_records`读一次库存，按顺序核验全部请求/响应/失败，拒绝未列入库存的文件，适用于独立回读。

## TCP日线Job

[TCP配置示例](../../config/runner/tcp-day.example.json)使用`tcp_day`与空`day_directories`；两种来源互斥。更换示例日期、端点、路径和全零摘要后运行。新冻结计划支持最多10000只排序唯一沪深A股，`workers.min(8)`为默认窗口并行度；`tcp_day_schedule`可显式设置总原始行、解压正文、期限及并行度。启动每个窗口前预留全部预算，受共享网络许可约束；余量不足减小窗口或失败。旧无schedule请求保持串行和32只上限。端点按独立IP池真实三轮探测后使用，不回退通用初始池；Job身份包含来源合同、端点和单位依据摘要。

先执行`rustdx-runner plan --config /path/to/job.json --max-keys 10000 --network-limit 8`。预检只读取显式输入与磁盘容量，不初始化root、账本或网络许可目录；列出全部应有键的计数/摘要、有上限的键样本、事实/固定输入字节、初始有效并发及原行/正文/归档/wire限制。`can_start=false`输出报告后退出1；Unknown事实直接拒绝。最坏情况预算缺口单列，初始可启动不等于完整历史可完成。请求数上界仅包括逐证券采集握手/分页，探测另计；期限是中断上限，不是成功耗时预测。执行仍重新冻结并核验事实和预算。

`source_as_of=planned_date`须等于实际北京时间采集日，`through`必须早于它。该策略只接收已经结束的日期，不支持当天收盘后特例或历史时点回溯；历史bar是今天主站提供的版本。新采集来源为`a-share/tcp-052b/day/raw/cny/integer-shares/v2`，请求/收据schema2，股量直接来自052b整数股数，金额按binary32人民币元解码。`volume_evidence_sha256`引用facts目录中的单位材料，随日历/公告冻结；摘要只证明材料一致，仍须维护者独立审核范围与原价口径。旧052d来源v1仅支持原字段离线重放。单位、日期及来源合同见[应用接入](../../docs/development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。

worker调用`rustdx --no-tcp-startup tcp-day --directory <candidate>`，此全局开关只跳过通用预热；TCP采集仍探测配置端点并使用受管许可。runner配置`no_tcp_startup=true`则拒绝TCP Job。父进程和worker不保留无用预热socket。

各证券的`raw/<code>/wire`保存从握手开始的完整压缩帧及库存，`receipt.json`保存来源、连接、观察日、采集时间、原始偏移/数量、重复边界、停止原因和标准股数。短页继续，空页在同连接确认；断线、错身份、不推进、冲突或预算耗尽拒绝候选。`max_wire_records`为所有证券合计，`max_source_bytes`扣除冻结facts后留给所有wire/receipt；还预留单证券receipt空间，预算过小在传输前拒绝。CSV写出/发布前均有界检查。

窗口线程只返回计数，全部退出后按输入顺序重放再写CSV；不会累积所有证券的解码结果。正文预留包含最后一页及确认的131070字节余量，失败取消采集上下文并等待线程退出，父任务保留自身状态。实际计数在schema8的`acquisition`中；runner重验总量及原码。完整规则与分钟/快照/订阅的独立合同见[应用接入](../../docs/development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。

schema8采集报告声明`volume_unit=shares`、独立来源合同和`business_coverage=unverified`。runner按库存重解请求配对、身份、分页参数/数量与字段，精确对应CSV后再核对事实的全部键；来源枚举到空页仍可能被`missing`阻断。旧day schema6/7继续从手换算股。两者共用封存、可选数据库核验、修订CAS及固定版本消费。补采推进`source_revision`；已封存恢复只重验归档，不再次联网采集。

实现和本地模拟结果见[第四阶段记录](../../docs/archive/development/REMAINING_FIXES_STAGE4_20261024.md)；整数股数修复与两节点24条全字段精确核对见[新报告](../../docs/archive/development/TCP_DAILY_INTEGER_SHARES_FIX_20261024.md)。五分钟、当天bar及全市场TCP负载仍未完成验收。

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

`include-ignored`还须设置真实隔离目标的`RUSTDX_TEST_CH_URL/USER/PASSWORD`、`RUSTDX_TEST_MONGO_URI/MONGOSH`。旧实现的两库实测见归档；2026-10-24的分页/持久会话变更已有本地HTTP和进程回归，真实两库及全量吞吐需重新验收。数据库CI已预建runner独立目标并显式执行全部control_plane测试，含600只模拟证券/1,200行版本隔离、指针中断恢复、确认丢失和离线恢复；配置更新不等于远端CI已经通过。

实际市场事实、稳定Linux、全量负载和长期SLO须分别验收；单机回归不等于这些门禁已通过。部署与操作见[部署说明](../../deploy/production/runner/README.md)，详细结果见[实施记录](../../docs/archive/development/PRODUCTION_IMPLEMENTATION.md)。
