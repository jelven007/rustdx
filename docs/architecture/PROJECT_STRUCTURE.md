# rustdx 项目目录与职责

更新日期：2026-10-23。目录覆盖开发、验证、配置、部署、运行和维护。核心包位于仓库根，CLI位于`rustdx-cmd`，新增生产控制层为`rustdx-runner`，外部接口测试包为`tests-integration`；保留现有包名、依赖路径和API，新增接口另行追踪。

## 1. 当前目录

```text
rustdx/
├── Cargo.toml / Cargo.lock         # 核心包与workspace、依赖锁定
├── README.md / CHANGELOG.md        # 项目入口、版本变更
├── LICENSE / rustfmt.toml
├── src/                           # rustdx_complete核心库
│   ├── lib.rs
│   ├── tcp/                       # 同步协议、受管网络及股票Client
│   │   ├── evidence.rs            # 可选完整帧observer、离线解压
│   │   └── stock/client/          # 批次、分页、价格复权
│   ├── file/                      # day、lc、gbbq checked解析
│   ├── data/                      # 标准行情、市场/单位/价格口径
│   ├── calendar/                  # 严格日历Provider
│   ├── limit/                     # 日期规则与证券事实区间
│   └── operation.rs / pool.rs / cache.rs / indicators.rs / atomic_file.rs
├── rustdx-cmd/                    # CLI包、库与rustdx二进制
│   ├── Cargo.toml / build.rs
│   ├── src/
│   │   ├── main.rs / lib.rs
│   │   ├── cmd/                   # day/east/data-check/data-status/show
│   │   ├── io/                    # 分块、sink/schema、精确回读
│   │   ├── eastmoney/             # 分页、标准适配、原始正文/重放
│   │   └── quality.rs / paths.rs / download.rs / fetch_code.rs / observation.rs / tcp_evidence.rs
│   ├── tests/fixtures/            # CLI端到端样本、东财真实留证
│   └── examples/                  # 离线重放等CLI库示例
├── rustdx-runner/                 # 独立单机生产控制crate，不发布
│   ├── Cargo.toml / README.md
│   ├── src/
│   │   ├── config.rs / model.rs / ledger.rs    # 严格Job、契约、SQLite WAL
│   │   ├── supervisor.rs / pipeline.rs        # 私有worker、覆盖/封存/目录CAS
│   │   ├── database.rs / database_mongo.js    # insert-only版本隔离、全范围回读
│   │   └── consumer.rs / storage.rs / ops.rs  # 固定目录读取、留证、指标/备份恢复
│   └── tests/                    # control_plane与wire_archive、opt-in真实服务
├── tests/                         # 核心回归、属性和本地协议服务
│   ├── fixtures/
│   │   ├── tdx/                   # 原assets中的day/gbbq二进制样本
│   │   ├── kline/                 # K线请求、响应、独立期望和清单
│   │   └── protocol/              # 快照/财务/逐笔独立参考
│   └── snapshots/                 # 版本化结构快照
├── tests-integration/             # 不发布的外部接口测试包
│   ├── Cargo.toml
│   ├── src/
│   └── tests/
├── examples/                      # 核心库使用、采样和smoke入口
├── benches/                       # 指标性能观测
├── fuzz/                          # 独立workspace、不可信解析fuzz
│   └── fuzz_targets/
├── config/                        # 配置说明、可复制变量及核实日历
│   ├── README.md / runtime.env.example
│   ├── calendars/                 # 两交易所2026版本化参考数据
│   └── runner/                    # 严格Job JSON、无凭据环境示例
├── deploy/                        # 部署与隔离验收配置
│   ├── clickhouse/
│   │   ├── compose.yml / .env.example
│   │   └── sql/factor.sql         # 前因子查询参考，不自动执行
│   ├── acceptance/                # 固定CH/Mongo镜像、资源限制
│   └── runner/                    # systemd任务/指标、告警、版本隔离schema、恢复文档
├── scripts/
│   ├── dev/                       # 日常workspace测试与日志
│   ├── ops/                       # 受管协议网络诊断
│   ├── bench/                     # day内存和数据库回读测量
│   ├── validation/                # 独立协议、真实历史/故障核验
│   └── release/                   # crates.io发布门禁
├── docs/
│   ├── README.md                  # 按职责导航
│   ├── REQUIREMENTS.md            # 旧路径兼容入口
│   ├── requirements/              # 唯一需求主文档、日期规则
│   ├── architecture/              # 架构、目录、网络和协议
│   ├── development/               # 构建与开发约束
│   ├── validation/evidence/       # 验证方法、验收与持久JSON证据
│   ├── operations/                # 运营、恢复、维护、留证与观测
│   ├── release/                   # 迁移、版本交付及回退
│   └── history/                   # 各轮迭代和优化历史
├── data/README.md                 # 运行数据位置约定；产物不提交
└── .github/workflows/             # 常规CI、覆盖巡检、数据库验收
```

`target/`、本机`.cargo/`、日志、下载缓存、fuzz语料/产物、私有配置及运行数据不属于源码交付。runner接受显式Job配置并管理单次任务/恢复；调度由systemd等外部系统触发，没有内置Web/API或长期调度服务。

## 2. 目录责任与边界

| 目录 | 放置内容 | 管理规则 |
| --- | --- | --- |
| src、rustdx-cmd/src | 已实现产品逻辑 | 核心计算/协议与CLI任务编排分离；同步I/O和有界线程 |
| rustdx-runner | 单机Job/Attempt、覆盖、发布目录、版本隔离数据库、消费/恢复 | SQLite等依赖留在控制层；root为本地持久Unix文件系统，不提供跨机协调 |
| tests、rustdx-cmd/tests、tests-integration | 断言、模拟服务、真实固定样本、外部opt-in测试 | 测试数据随调用包就近归类；外部skip不作为在线通过 |
| examples、benches、fuzz | 使用演示、性能观测、恶意输入验证 | 不充当运行任务数据目录；不纳入发布crate主体 |
| config | 无凭据示例、带版本/来源的参考日历 | 私有runtime.env被忽略；CLI不自动读取该文件，日历须显式传入 |
| deploy | 持久化部署及独立验收模板、运维SQL | 验收容器与生产数据卷区分；镜像/配置通过不等于目标环境验收 |
| scripts | 按开发/运维/测量/验收/发布分类的工具 | shell自动定位仓库根；Python显式路径；实际命令见[脚本说明](../../scripts/README.md) |
| docs | 按生命周期归类的唯一主文档及历史记录 | 文档间使用相对链接；固定证据归validation/evidence，不改写原始指标 |
| data | 本地或部署任务的运行数据入口约定 | 仅README入库；实际源、输出、清单、证据、备份在部署存储管理 |

## 3. 迁移对应

| 原路径 | 当前路径 |
| --- | --- |
| docs/REQUIREMENTS.md、LIMIT_RULES.md | docs/requirements/；旧需求路径保留跳转 |
| docs/PROJECT_ANALYSIS.md、NETWORK.md、PROTOCOL_REQUESTS.md | docs/architecture/ |
| docs/DEVELOPMENT.md | docs/development/ |
| docs/VALIDATION.md、PRODUCTION_ACCEPTANCE.md、DATABASE_ACCEPTANCE.md | docs/validation/ |
| docs/fixtures/ | docs/validation/evidence/ |
| docs/OPERATIONS.md、RUNBOOK.md、IMPORTS.md、DATA_MAINTENANCE.md、EAST_REPLAY.md、OBSERVABILITY.md | docs/operations/ |
| docs/MIGRATION.md、RELEASES.md | docs/release/ |
| docs/ITERATION.md、OPTIMIZATION.md、NEXT_OPTIMIZATION.md、OPTIMIZATION_ROUND3.md | docs/history/ |
| scripts/run_comprehensive_test.sh、diagnose_network.sh、publish.sh | scripts/dev/、ops/、release/ |
| scripts/measure_*.py、verify_*.py | scripts/bench/、validation/ |
| assets/gbbq、sz000001.day | tests/fixtures/tdx/ |
| assets/factor.sql | deploy/clickhouse/sql/factor.sql |
| data/calendars/ | config/calendars/ |

版本化参考日历与运行数据分开，使data可整体作为不提交的运行产物目录。原二进制样本、日历及验收JSON按摘要保持内容不变；源码中对应示例/测试路径已更新。四份protocol期望CSV按已有清单核对摘要后纳入版本控制，避免依赖本机被忽略文件。

## 4. 新增文件放置规则

新增核心接口回归放tests，CLI任务回归放rustdx-cmd/tests，控制层发布/数据库/留证组合回归放rustdx-runner/tests。固定参考数据附来源、时间、版本和摘要；mock事实明确标注。独立核验脚本放scripts/validation，测量脚本放bench，实际运行日志/大包不入库。

需求编号和可观察行为在[需求主文档](../requirements/REQUIREMENTS.md)维护；架构选择在architecture，命令/开发约束在development，专项运维字段在operations，测试结论在validation，各轮过程在history。发布crate排除config/data/docs/deploy/scripts/tests等工程附件；发布前分别检查两个包的实际清单。

目录整理验证见[验证方法](../validation/VALIDATION.md)，新增组件见[实施记录](../development/PRODUCTION_IMPLEMENTATION.md)。CLI继续读取命令行/环境；runner单独提供严格Job JSON与私有目录创建，部署输入/凭据/调度由部署方管理。
