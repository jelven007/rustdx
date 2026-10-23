# 生产新增组件实施记录

开始日期：2026-10-23。实施基线：`dce1fbd`。依据：[生产融合技术方案](../architecture/PRODUCTION_FUSION_PLAN.md)。用户已授权开始实施新增组件，本文件记录交付范围、实现进度和实际验证，不是另一次审批流程。

## 实施范围

优先实现技术方案G3所需的单机生产组件，并为G2/G4准备可以执行的验证入口。保留Rust2024/MSRV1.89、同步I/O和有界并发；核心协议库、一次性CLI采集和新增生产控制层各自承担职责。

| 工作 | 计划交付 | 当前状态 |
| --- | --- | --- |
| 任务与控制账本 | 独立rustdx-runner crate；SQLite WAL/FULL持久事务，Job/Attempt/追加事件与控制锁 | 已实现；去重、活动唯一、事务回滚回归通过；账本schema2 |
| 任务执行 | 严格配置、私有工作目录、CLI子进程、绝对deadline、取消与未结束任务恢复 | 已实现本地day；进程强制终止/恢复不重采通过 |
| 契约与业务覆盖 | 标准元/股、版本化事实与点时名单、应有/意外键检查、Unknown拒绝 | 已实现A股原价日线；缺一个日期及全部明确免行情回归通过 |
| 证据与候选 | 原字节/清单归档、哈希与限额、校验后封存、失败隔离 | day、公告、事实、CLI二进制及产物归档/恢复通过 |
| 发布与消费 | 不可变release、完整日期映射、事务CAS目录、固定版本原子CSV导出 | 文件/数据库导出通过；旧修订隔离、目录失败恢复和旧读者通过 |
| 数据库版本隔离 | 新CH/Mongo目标、insert-only、完整release核验、unknown不重写 | 已实现；真实本机两库/1,200行模拟数据跨批次及故障回归通过 |
| TCP原始留证 | 从握手起同步完整帧留证、fsync/限额/库存、checked离线重放 | 已实现接口/示例；压缩、重连、错误/截断、篡改及传输前限额通过 |
| 观测与备份 | 状态/Prometheus原子textfile、SQLite在线备份及全部归档隔离恢复 | 已实现；WAL、库存/引用核验与篡改拒绝通过 |
| 部署与工程门禁 | Linux/systemd/告警/DBA schema、配置/文档、MSRV及故障回归 | 模板已交付；Rust1.89 workspace fmt/Clippy/离线测试通过；目标Linux未执行 |

跨主机采集和publisher高可用属于方案G5的条件扩展。单机实现不能将本地SQLite/OS锁宣传为跨机协调；完整高可用必须落实任期、受控写入、在途请求封存、事务指针及消费者目录。真实目标Linux、全量负载、长期SLO和全市场事实数据按各自门禁验收，代码实现不自动使这些外部条件通过。

## 实现约束

1. worker只向私有目录采集；正式数据写入与指针推进由publisher负责。
2. CLI complete、候选verified/封存和业务released分别记录；未核实的证券状态不补成停牌事实。
3. 发布前保存能恢复内容的原字节和规范产物，SHA-256不是备份。
4. 目录提交同时检查上一catalog_version；候选修订按来源/明确范围比较，不能仅按运行时间覆盖。
5. 强制终止、确认丢失或unknown写入保留恢复材料；恢复命令核查具体阶段，不盲重跑。
6. 新依赖只进入需要它的新增控制层；真实编译确认Rust1.89兼容后才记录通过。

## 实现与恢复不变量

`rustdx-runner`版本0.1.0、`publish=false`。SQLite依赖`rusqlite=0.40.2`使用bundled/backup，Rust1.89构建通过；不增加异步运行时，核心库无新增依赖。发布适配复用CLI受管HTTP，MongoDB每个512行逻辑请求启动mongosh。目录/文件原子发布与SQLite控制事务分开，孤立候选不自动成为正式版本。

| 不变量 | 实现与回归 |
| --- | --- |
| complete不等于released | 重放day、独立CSV回读、全部应有键/免行情、可选数据库及目录事务分别判定 |
| 未知事实不补停牌 | Unknown在worker启动前拒绝；全部明确免行情可生成空分区，`collector_executed=false` |
| 恢复不重采封存输入 | recover验证release库存和原字节；外部day改变也不重新读取 |
| 旧源不能覆盖新目录 | source_revision增加且source_as_of不降低；CAS与发布事件共事务，失败保持旧指针 |
| 网络动作之前有恢复材料 | 数据库请求fsync后事务登记intent，再传输/spawn；完整限额内响应持久后确认 |
| unknown不重INSERT | 缺响应/确认丢失保持待核查；显式隔离后使用全新release，旧在途写入不作用于新候选 |
| 单键回读不能遗漏额外键 | 全字段精确比对+完整release计数；CH重复及Mongo额外键使固定版本导出失败并保留旧输出 |
| 恢复不改变目录凭据身份 | fresh回读另存证据，原verified Receipt保持不变，目录事务失败后可复用原孤立目录 |
| 备份是真实字节 | SQLite在线备份包含已提交WAL，复制attempt/release/catalog/database；隔离恢复核验全部引用 |
| 留证失败不返回无证据成功 | TCP传输前Request、解压前完整Response及失败事件；归档限额耗尽可在握手传输前拒绝 |

实现期间定位并修复：macOS封存目录移动权限问题（rename后fsync/只读再登记）；CLI原子暂存删除与工作目录扫描的ENOENT竞态（仅忽略已消失条目，静止封存重计）；verified后恢复重写凭据导致孤立catalog身份失配（保留原Receipt）；未压缩帧无条件复制正文（保留原有移动路径，仅压缩时解码）。

## 验证记录

平台：macOS27.0.1/arm64，Rust1.89.0。真实隔离本机服务：ClickHouse实验26.11.1.548、MongoDB8.0.16、mongosh2.5.10；端口28123/27029。公告、证券区间及行情夹具明确为模拟，不作为真实市场事实；600只模拟证券的两日1,200行跨越512行批次。测试源码随仓库保存，临时数据/日志不作为长期备份。

| 检查 | 实际结果 |
| --- | --- |
| runner全部案例 | `cargo +1.89.0 test -p rustdx-runner -- --include-ignored`退出0；1库单测+12control_plane+3wire_archive，共16项全部执行，0忽略 |
| 真实CLI/文件发布 | 首次发布、同Job不重采、两次修订、旧读者、101股转换、缺日期保护、免行情、deadline终止/恢复、封存后恢复不重采 |
| 真实数据库/模拟行情 | 两release、所有目标verified后注入目录事件失败/恢复、旧目录导出1,200行/旧价格、CH重复和Mongo额外键拒绝、指针不变 |
| 确认丢失 | 本地HTTP服务返回不完整确认，恢复保持待核查；INSERT计数不增加，原请求证据保留 |
| 备份/恢复 | WAL在线备份、控制层/完整归档/数据库证据隔离恢复、引用校验与篡改拒绝 |
| TCP | 压缩/重连完整帧、checked重放、坏解压先留证、截断标failed、库存篡改拒绝、限额耗尽握手前失败 |
| 最终workspace | Rust1.89 `fmt --check`、all-targets/all-features Clippy `-D warnings`、all-features离线test均退出0 |
| 最终构建/包清单 | locked/offline构建CLI/runner及核心/CLI `package --list`退出0；包含新TCP源文件，排除runner/config/deploy；未执行注册表上传包准备 |
| 文档/模板静态 | 367个本地Markdown目标存在，FR-01至18唯一连续；Job/证据JSON、4份systemd段/引用、4条告警YAML及2份Mongo JS语法通过；未运行systemd-analyze/promtool或Linux服务 |
| CI范围/命令 | Linux/macOS覆盖runner；Windows保持核心/CLI，排除Unix runner。新增真实CLI/本地故障步骤的同一命令在本机11项通过/1真实DB过滤；远端CI未执行 |

最终workspace测试含核心160、CLI库9/命令31、其余端到端/属性/固定协议/本地网络、普通文档55与compile_fail2，runner离线12项通过/4项忽略。4项runner忽略案例已在上述阶段全执行；两种运行不重复相加。`RUSTDX_SKIP_INTEGRATION_TESTS=1`及未启用外部DB开关会令既有外部案例提前返回ok，它们不计在线通过。最后未压缩帧优化后重新执行workspace中的3项wire_archive回归，通过；无数据库适配变化，没有重复昂贵数据库验收。

持久摘要见[组件证据JSON](../validation/evidence/runner-implementation-20261023.json)。原始日志`/tmp/rustdx-runner-recovery-final.log`和`/tmp/rustdx-runner-workspace-final.log`在JSON记录摘要/命令/退出码；临时路径非备份。既有779,200行及四项数据库故障证据保留原轮次，不计作新发布层全量通过。

最终构建与包清单日志为`/tmp/rustdx-runner-packaging-final.log`，二进制摘要在JSON区分阶段二验收和最后构建。本次新增隔离CH/Mongo进程已按PID命令、可执行文件及测试配置/数据目录核实后停止；保留临时诊断和既有数据，不清理其他服务。

## 复现与部署入口

```sh
cargo +1.89.0 build -p jelven-rustdx-cli -p rustdx-runner --locked
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked
```

全执行runner测试前，使用独立临时目标按[部署schema](../../deploy/runner/README.md)预建CH表/Mongo集合，并设置`RUSTDX_TEST_CLI`、`RUSTDX_TEST_CH_URL`、`RUSTDX_TEST_CH_USER`、`RUSTDX_TEST_CH_PASSWORD`、`RUSTDX_TEST_MONGO_URI`、`RUSTDX_TEST_MONGOSH`。测试会执行DBA故障注入，只能用于隔离验收目标；凭据值不保存至代码/证据。随后执行`cargo +1.89.0 test -p rustdx-runner -- --include-ignored`。

命令与配置见[runner](../../rustdx-runner/README.md)，操作/权限/systemd/告警及恢复见[部署说明](../../deploy/runner/README.md)，业务行为追踪为[需求2.1](../requirements/REQUIREMENTS.md)FR-15至18。

## 未关闭的生产门禁

G3单机组件工程实现已完成，G3生产门禁未整体通过。G1真实全市场事实/其余协议、G2目标稳定Linux与全量/服务器恢复、G4批准周期及SLO、G5跨机/HA分别推进。模板静态检查不等于systemd服务启动通过。

硬磁盘配额、独立故障域备份、数据库服务端备份恢复、发布/消费者账号权限和外部写入治理须在部署证明。当前worker与publisher同UID；`env_clear`防止凭据继承，不隔离恶意同UID程序。INSERT权限也不能阻止凭据持有者追加已封存release。固定版本DB读取精确拒绝篡改，是检测机制，不替代权限。

MongoDB每逻辑请求启动mongosh带来连接/进程成本；1,200行回归不作为全量吞吐量、峰值或长期P95证据。RPO/RTO、保留期及完整市场覆盖须依据真实需求和演练取得。本次仅本地源码/文档/测试交付，不推送、发布crate或部署生产。
