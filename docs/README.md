# rustdx 文档索引

更新日期：2026-10-23。文档按生产交付生命周期归类，需求主文档版本2.0。当前源码采用Rust2024/MSRV1.89及同步有界架构，运行清单schema6；Cargo版本仍为核心库1.12.2、CLI1.8.2，后续优化尚未发布新crate版本，使用新行为须从源码构建。

## 按工作查找

| 工作 | 主文档 | 内容 |
| --- | --- | --- |
| 产品需求 | [requirements/REQUIREMENTS.md](requirements/REQUIREMENTS.md) | 产品范围、角色/对象/状态、FR-01至14、非功能要求、证据及待补事实 |
| 项目目录 | [architecture/PROJECT_STRUCTURE.md](architecture/PROJECT_STRUCTURE.md) | 实际目录、责任边界、迁移对应及新增文件规则 |
| 技术架构 | [architecture/PROJECT_ANALYSIS.md](architecture/PROJECT_ANALYSIS.md) | 模块职责、同步有界架构、设计选择、兼容边界 |
| 开发 | [development/DEVELOPMENT.md](development/DEVELOPMENT.md) | 构建、源码导航、接口及实现约束 |
| 测试 | [validation/VALIDATION.md](validation/VALIDATION.md) | MSRV/fmt/Clippy/测试、fuzz、数值及性能证据 |
| 验收 | [validation/PRODUCTION_ACCEPTANCE.md](validation/PRODUCTION_ACCEPTANCE.md)、[DATABASE_ACCEPTANCE.md](validation/DATABASE_ACCEPTANCE.md) | 历次真实服务范围、固定配置、小表/历史子集及故障恢复 |
| 运营 | [operations/OPERATIONS.md](operations/OPERATIONS.md) | 数据选择、完成判定、单位、补数和来源风险 |
| 运维 | [operations/RUNBOOK.md](operations/RUNBOOK.md) | 部署、配置、观测、备份和按状态恢复 |
| 发布/迁移 | [release/RELEASES.md](release/RELEASES.md)、[MIGRATION.md](release/MIGRATION.md) | 源码版本、历史快照、包发布门禁及旧调用迁移 |
| 迭代历史 | [history/ITERATION.md](history/ITERATION.md)、[OPTIMIZATION.md](history/OPTIMIZATION.md) | 早期阶段与10月22日四步记录 |
| 后续优化 | [history/NEXT_OPTIMIZATION.md](history/NEXT_OPTIMIZATION.md)、[OPTIMIZATION_ROUND3.md](history/OPTIMIZATION_ROUND3.md) | 10月23日两轮六步实现及交叉验证 |

配置、脚本、固定夹具和部署入口分别见[config](../config/README.md)、[scripts](../scripts/README.md)、[tests/fixtures](../tests/fixtures/README.md)、[deploy](../deploy/README.md)。运行数据约定见[data](../data/README.md)。旧docs/REQUIREMENTS.md只保留跳转。

## 专项定义

| 主文档 | 唯一定义范围 | 关联需求 |
| --- | --- | --- |
| [日期规则](requirements/LIMIT_RULES.md) | 日期化涨跌幅、上市阶段、原始依据及覆盖 | FR-02 |
| [网络预算](architecture/NETWORK.md) | 任务时限、受管TCP/HTTP、同机许可与配置 | FR-04 |
| [协议请求](architecture/PROTOCOL_REQUESTS.md) | checked请求、独立解码、参数及旧对象边界 | FR-07 |
| [导入证据](operations/IMPORTS.md) | 清单字段/状态、摘要、精确回读和CSV恢复 | FR-05、FR-06 |
| [数据维护](operations/DATA_MAINTENANCE.md) | 日历交叉核验、证券公告证据与覆盖巡检 | FR-01、FR-02 |
| [东财重放](operations/EAST_REPLAY.md) | 原始正文、摘要范围、重放及前因子限制 | FR-13 |
| [阶段观测](operations/OBSERVABILITY.md) | 耗时并集/累计、临时逻辑字节、新鲜度口径 | FR-14 |
| [验证方法](validation/VALIDATION.md) | 工程检查、指标/缓存/文件回归和测量 | FR-09、FR-10、FR-12 |
| [目录与版本](architecture/PROJECT_STRUCTURE.md) | 源码附件、配置/数据分界和交付 | FR-11 |

其他文档引用专项定义，历史章节保留当时结论。需求变更先更新稳定编号及用户可观察行为，再同步受影响架构、开发、验证和运营/运维文档。

## 当前证据与边界

第三轮真实官方ZIP子集为128文件、779,200行，Python独立逐行解码，1/4线程CSV一致；两库首次/重复全部键及数值精确回读通过，四项断连/确认丢失/SIGKILL恢复通过。原始指标、失败与恢复分别保存：

- [真实历史](validation/evidence/round3-real-day-databases.json)、[故障恢复](validation/evidence/round3-database-faults.json)。
- [回读基准](validation/evidence/round3-readback-baseline.json)、[优化测量](validation/evidence/round3-readback-optimized.json)。
- [格式化等待失败](validation/evidence/round3-real-day-formatting-failure.json)、[线程池耗尽失败](validation/evidence/round3-real-day-thread-pool-failure.json)。
- [分块内存](validation/evidence/day-memory-20261023.json)、[前轮最终验收](validation/evidence/final-acceptance-20261023.json)。

JSON中的原始命令和本机绝对路径保留当时记录；复现使用[当前脚本入口](../scripts/README.md)及[迁移对应](architecture/PROJECT_STRUCTURE.md)。/tmp日志不属于长期备份，固定样本及仓库中的摘要/指标才是本次持久材料。

实际服务是macOS实验ClickHouse26.11.1.548与MongoDB8.0.16；固定Linux镜像/数据库CI已配置，尚未在该平台运行。未重跑最新全包约2971万行，不能以历史10月10日2981万行结果替代当前全量。内置日历为2024–2026、规则核实截至2026-10-22，2027完整日历、全市场证券状态、长期P95、目标生产配置及备份恢复仍待相应证据。

[项目入口](../README.md) · [变更记录](../CHANGELOG.md)
