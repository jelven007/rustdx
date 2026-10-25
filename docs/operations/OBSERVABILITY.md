# 采集阶段观测

<!-- lifecycle:registry -->
读者：值守/运营；责任：运维维护者。源码核验：2026-10-25；每30日及指标、阈值或告警变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

更新日期：2026-10-24。本文定义统计范围；运营人员按业务名单和日期判断可用性，运维人员按[运维手册](../runbooks/RUNBOOK.md)设置告警和恢复。生产阈值依据目标环境和批准SLO维护。

当前schema7在day清单中保留schema6引入的`observations`及`freshness`，不改变CSV列或数据库逻辑键。`rustdx_cli::observation`提供可克隆的每任务观测对象、线程作用域、失败自动计数的RAII阶段及临时文件写入包装；解析工作线程显式进入同一任务，不使用进程全局阶段计数。

| 字段 | 统计口径 |
| --- | --- |
| elapsed_wall_ms | 本次来源准备开始至当前清单序列化的单调时钟墙钟时间；不含输出锁之前的参数校验 |
| calls / failures / in_flight | 阶段调用次数、未成功结束次数、仍在执行次数；错误、取消和panic展开均计入失败 |
| completed_span_ms | 已结束阶段调用的耗时之和；包括等待与I/O，并发/嵌套调用可重复累计 |
| active_wall_ms | 此阶段至少一个调用正在执行的时间并集；同阶段并发或嵌套只计一次 |
| temporary_peak_bytes | 本任务受跟踪临时路径同时存活的已写入逻辑字节峰值 |
| temporary_current_bytes | 当前仍存活的受跟踪临时路径字节；正常结束、失败和协作取消后应为0 |

累计耗时和活跃墙钟均不是CPU时间。阶段可能嵌套，因此不能相加得到总耗时。未使用的阶段保留0，运行中累计耗时只包含已结束调用，活跃墙钟包括未结束调用。观测序列化和清单持久化本身不计入阶段。

`source`覆盖day来源准备，`download`覆盖HEAD及各下载调用/重试，`extract`覆盖解压及目录发现；目录输入不产生下载/解压。即使来源准备发生在创建最终清单之前，耗时和started_at仍承接同一任务。来源准备失败时会保存失败观测；此阶段遭SIGKILL不保证留下清单。

`snapshot`覆盖day私有磁盘快照及摘要，`parse`覆盖分块解码与质量检查。质量拒绝也增加失败调用。`sort`覆盖非A股块内排序和磁盘归并，后者包含读写I/O。`write`覆盖CSV序列化、归并、最终flush/原子提交、数据库准备和导入。`readback`覆盖CSV回读、摘要核实及全部数据库键/字段比对；Mongo首次启动与最后退出确认也在该阶段内。旧`database_verification`的fetch_elapsed_ms/elapsed_ms保持原口径。

临时字节精确累计成功Write返回的字节，文件在队列中或跨线程持有时继续计数；临时路径删除或发布为正式文件后释放。覆盖day源快照、排序run、归并spool、待发布CSV/JSON。**不覆盖**ZIP/续传缓存、解压目录、清单/原始响应证据、因子输出及数据库子进程scratch；不是文件系统块占用、全盘峰值、内存RSS或数据库空间。SIGKILL不能执行RAII释放，须按[恢复说明](IMPORTS.md)核查遗留文件。

`freshness`报告交易所当前日期、无质量错误且非空来源的最旧/最新末日、最旧来源的自然日年龄和未来日期来源数，以及质量错误/告警总数（包含被省略详情）和任务错误数。自然日年龄允许为负，不代表缺失交易日或证券异常；停牌、退市和未知状态仍需来源证据。日历、规则及证券状态覆盖见`data_coverage`。失败或strict拒绝时这里的日期也不能冒充已发布数据，发布范围应读`artifact`。

回归检查包含同任务并发和嵌套时间并集、其他任务隔离、panic/失败RAII释放、临时文件跨线程排队与发布释放、1/4/8线程CSV字节一致、跨块质量失败、取消清理，以及本地ZIP解压观测跨清单创建承接。

新增runner可从SQLite账本导出结构化状态和Prometheus textfile，包含Job状态、未结束Attempt、事件、发布时间、指标刷新时点与剩余磁盘。`metrics --output`原子替换；它不把CLI阶段P95合并为跨任务P95，也不把自然日发布时间差当交易日覆盖。systemd指标timer及告警只是部署模板，阈值须结合实际Job预算/日历与批准SLO；字段/命令和接入步骤见[runner部署](../../deploy/production/runner/README.md)。本机回归不提供长期生产P95或SLO。

runner现在为collector设置私有`RUSTDX_TASK_OBSERVATIONS`路径与attempt身份。CLI成功、业务失败或协作取消退出时原子保存本进程网络计数和任务阶段；runner核对attempt并记录监督进程PID后写入SQLite schema3的`attempt_observations`，兼容collector包装器启动的CLI子进程。独立metrics进程累加这些持久记录，以owner=worker/controller区分；controller请求计数取任务前后差值，进程峰值/P95不当作任务值。SIGKILL或未进入主逻辑的worker保存unavailable记录，不能当零请求；不把metrics进程的NetworkStats代入worker，也不合并各任务的P95。controller记录在发布事务前持久化，发布成败读取账本状态。

`scripts/bench/measure_process_tree.py --output new.json --disk-root ROOT --interval 1 -- COMMAND`在Linux/macOS采样专属进程组及后代的同时RSS、FD、线程、进程数和声明目录逻辑字节，保留原始样本、缺失采样和峰值。短命进程或采样间峰值可能遗漏；磁盘逻辑字节不是分配块或未声明远端数据库容量。工具只终止自身创建的进程组。线上硬上限仍由systemd/cgroup/文件系统配额保障；worker同时巡检workspace字节和最小剩余磁盘。

本地固定版本恢复入口为`scripts/validation/verify_local_recovery.py`，它在新目录执行一致性备份、隔离restore及同一catalog版本全CSV字节比较，保存命令、耗时和摘要。数据库原生恢复继续走`verify_database_recovery.py`；本地恢复不证明远端数据库、持续写入、生产权限或批准RPO/RTO。
