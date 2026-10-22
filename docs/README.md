# rustdx 文档索引

更新日期：2026-10-23。本文档组包含10月22日四个源码优化提交及10月23日六步优化，逐步证据见[NEXT_OPTIMIZATION.md](NEXT_OPTIMIZATION.md)。Cargo版本仍为核心库1.12.2、CLI1.8.2；这些新增行为属于仓库源码，尚未发布到crates.io。使用该行为须从仓库源码构建。

## 按工作查找

| 工作 | 主文档 | 内容 |
| --- | --- | --- |
| 需求 | [REQUIREMENTS.md](REQUIREMENTS.md) | 用户任务、已实现功能、完成条件及异常行为 |
| 方案与架构 | [PROJECT_ANALYSIS.md](PROJECT_ANALYSIS.md) | 模块职责、同步有界架构、设计选择及剩余工作 |
| 开发 | [DEVELOPMENT.md](DEVELOPMENT.md) | 构建、源码导航、接口选择及兼容约束 |
| 测试 | [VALIDATION.md](VALIDATION.md) | 检查命令、回归范围、fuzz及性能实测 |
| 验收 | [PRODUCTION_ACCEPTANCE.md](PRODUCTION_ACCEPTANCE.md)、[NEXT_OPTIMIZATION.md](NEXT_OPTIMIZATION.md) | 分轮记录，区分历史全量、当前在线小样本与离线验证 |
| 运营 | [OPERATIONS.md](OPERATIONS.md) | 日线/快照使用、数据质量判断、补数和业务口径 |
| 运维 | [RUNBOOK.md](RUNBOOK.md) | 部署、监控、备份、故障恢复及版本回退 |
| 版本交付 | [RELEASES.md](RELEASES.md) | 分支合并、源码推送、两个历史版本及包发布边界 |
| 迭代记录 | [ITERATION.md](ITERATION.md)、[OPTIMIZATION.md](OPTIMIZATION.md) | 各阶段交付和优先级处理结果 |
| 行为迁移 | [MIGRATION.md](MIGRATION.md) | API、CSV、数据库和旧调用迁移 |

专项规则只在以下文档完整定义，其他文档引用这些定义：

- [NETWORK.md](NETWORK.md)：受管TCP预算、时限、配置和观测。
- [LIMIT_RULES.md](LIMIT_RULES.md)：日期化涨跌幅、上市阶段及原始依据。
- [IMPORTS.md](IMPORTS.md)：运行清单字段、状态、摘要范围及数据库确认边界。
- [DATA_MAINTENANCE.md](DATA_MAINTENANCE.md)：日历交叉核验、原子更新与有公告证据的证券状态区间。
- [NEXT_OPTIMIZATION.md](NEXT_OPTIMIZATION.md)：2026-10-23后续六步优化及本轮交叉验证证据。
- [OPTIMIZATION_ROUND3.md](OPTIMIZATION_ROUND3.md)：覆盖告警、来源重放、回读效率、协议安全、阶段观测及部署故障验收。
- [PROTOCOL_REQUESTS.md](PROTOCOL_REQUESTS.md)：K线/快照/财务/逐笔checked请求、独立解码与源单位迁移。

## 本轮变更覆盖

| 源码提交 | 变更 | 对应需求 | 方案/开发依据 | 测试及运营、运维依据 |
| --- | --- | --- | --- | --- |
| `7cd02e7` | 严格外部日历、历史规则、标准行情、股票名单HTTPS | FR-01、FR-02、FR-03、FR-08 | 架构评估、开发说明、日期规则、迁移说明 | 验证方法；运营的数据口径/日历维护；运维更新日历 |
| `5fb5b70` | 进程连接预算、轮换/冷却、总体时限、统计 | FR-04 | 架构评估、开发说明、网络预算 | 独立进程网络回归；运营采集限制；运维网络排障 |
| `401265c` | 导入清单、摘要/回读、故障状态、安全二进制解析 | FR-05、FR-06、FR-07 | 架构评估、开发说明、导入说明 | 清单/坏文件/数据库故障回归；运营成功判定；运维恢复 |
| `1f9f739` | 线性指标、分页/复权/sink拆分、属性/fuzz/性能CI | FR-09、FR-10 | 架构评估、开发说明、优化记录 | 验证方法；运营指标口径；运维部署门禁 |
| 本轮文档更新 | 文档对齐、分支合并与两个历史版本保留 | FR-11 | 本文档组、版本交付说明 | 文档链接和代码对照检查；远端引用核验 |
| 10月23日步骤1–2 | data-check/证券证据、当前行情及隔离数据库验收 | FR-01、FR-02、FR-08、FR-10 | 数据维护、架构评估、六步结果 | 两交易所逐日日历、真实TDX/东财与数据库小表 |
| 10月23日步骤3–4 | 精确数据库回读、目标写锁、整任务deadline/取消、同机许可 | FR-04、FR-06 | 导入说明、网络预算、开发说明 | 故障注入、独立进程、完整CLI及实际服务 |
| 10月23日步骤5–6 | 分块/确定性输出、东财标准适配、不可变K线请求/独立解码 | FR-03、FR-05、FR-07 | 架构评估、开发说明、迁移说明 | 新旧实际RSS/字段摘要、跨块因子、固定pytdx真实协议样本及fuzz |

内置日历只覆盖2024–2026；历史规则核实截至2026-10-22。当前源码已实测通达信、东财及隔离ClickHouse/MongoDB小表，目标生产版本/配置和全量负载仍须验收；2027完整数据、全市场证券状态和长期P95未取得。详细边界见各专项文档。[项目入口](../README.md) · [变更记录](../CHANGELOG.md)。
