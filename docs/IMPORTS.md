# 导入证据与故障恢复

更新日期：2026-10-22；本说明对应源码401265c及后续优化，尚未发布新crate版本。运营成功判定见[OPERATIONS.md](OPERATIONS.md)，部署、现场核查及恢复步骤见[RUNBOOK.md](RUNBOOK.md)。

`day`、`east` 在解析/采集开始前写入运行清单。默认最新清单为 `<output>.manifest.json`；`--report report.json` 可替换该路径。每次运行还保留 `<清单路径>.<run_id>.json`，供后续核查。清单通过同目录原子替换持久化，Unix 同步目录项。

同一run_id归档在检查点更新，结束后保存最终结果；每个检查点先保存归档，再保存最新清单。两文件分别原子更新，不构成一个事务。参数/路径冲突、锁获取或下载来源解析可能发生在采集开始前，这些失败不保证生成清单。project_version来自CLI Cargo版本，当前仍为1.8.2；需在部署/运营记录另存Git提交以区分同版本号的源码优化。

schema_version=2 保留原质量报告字段，增加 run_id、run_version、stage、sources、artifact、database_status 和 ClickHouse query_id。run_version 是本次客户端开始时间的纳秒标识；数据库版本策略另外记录：ClickHouse 沿用表内服务端纳秒默认版本，MongoDB 按 `(date,code)` upsert。两者不能混作客户端版本。

来源 SHA-256 针对实际传给解析器的同一份字节快照，包括 day、gbbq、前因子和外部日历。单个 day 限制32MiB，gbbq/前因子限制64MiB，日历限制1MiB；读取本身有界。East 摘要针对转换后的完整接口数据，`source=eastmoney:normalized_payload`，不冒充原始 HTTP 正文。股票名单选择及下载地址仍以调用参数和缓存记录为准。

日线文件内部按日期规范化，同一证券的多个输入日期区间重叠时拒绝发布，即使区间实际缺口可能互补。这样可用与源文件数成比例的内存证明逻辑键唯一性，不需为数千万行建立全局 HashSet。CSV 发布前流式回读校验行数、键所属区间及日期边界，再记录最终字节 SHA-256。东财JSON由已校验记录直接序列化并记录输出摘要，不执行CSV回读。清单的 logical_keys 是经过来源检查的输出键数，不是数据库查询结果。

运行状态：

| 状态 | 含义 |
| --- | --- |
| running / parsing | 尚未发布新输出；进程异常结束时保留running |
| artifact.publication_pending | 已记录待发布文件摘要；是否已rename需核查 |
| artifact.published | 已确认输出文件原子发布 |
| database_status.not_started | 尚未发起数据写入 |
| database_status.unknown | 写入即将或已发起；异常/超时不能证明数据库没有部分数据 |
| database_status.acknowledged | 已读完ClickHouse成功响应，或mongoimport退出码为0 |
| artifact.cleanup_pending | 数据库已确认，CSV删除/更名结果需要核查 |
| complete / failed | 流程已结束；具体失败阶段和已确认动作仍在清单中 |

数据库失败保留已发布CSV。成功后默认删除CSV，`day -k` 更名保留为 `<output>.csv`；清单保留摘要及确认的文件状态。ClickHouse读取完整响应并检测HTTP错误、异常头、HTTP200末尾异常和截断正文，带query_id发送INSERT；单次HTTP操作上限300秒。MongoDB子进程上限300秒，超时终止并回收，输出直接继承终端，避免无界缓冲。

清单没有提供跨文件、跨数据库事务，也不会自动重试不确定写入。恢复前应使用CSV摘要、目标表、query_id及服务端记录核实；ClickHouse `FINAL` 和 MongoDB 唯一索引/目标数据核对由部署环境完成。ClickHouse当前边界查询仅作观测，成功回执并不等于逐行独立核对。MongoDB生产验收仍需可用服务及 `(date,code)` 唯一索引；同一数据库目标的多主机并发写入需外部协调。

故障回归覆盖每次运行归档、发布前回读、输入重叠、迟到HTTP异常、子进程非零退出及超时。离线回归不代表生产数据库已验收。

本次保留的两个Git历史标签只用于源码版本检出，运行清单归档不会自动裁剪成两份，保留期限由部署方管理。源摘要不替代输入备份，源码回退也不会回滚数据库或发布文件；见[RELEASES.md](RELEASES.md)。
