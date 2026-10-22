# 导入证据与故障恢复

更新日期：2026-10-23；本说明对应本轮仓库源码，尚未发布新crate版本。运营成功判定见[OPERATIONS.md](OPERATIONS.md)，部署、现场核查及恢复步骤见[RUNBOOK.md](RUNBOOK.md)。

`day`、`east` 在解析/采集开始前写入运行清单。默认最新清单为 `<output>.manifest.json`；`--report report.json` 可替换该路径。每次运行还保留 `<清单路径>.<run_id>.json`，供后续核查。清单通过同目录原子替换持久化，Unix 同步目录项。

同一run_id归档在检查点更新，结束后保存最终结果；每个检查点先保存归档，再保存最新清单。两文件分别原子更新，不构成一个事务。参数/路径冲突、锁获取或下载来源解析可能发生在采集开始前，这些失败不保证生成清单。project_version来自CLI Cargo版本，当前仍为1.8.2；需在部署/运营记录另存Git提交以区分同版本号的源码优化。

schema_version=3 保留原质量报告字段和运行标识，增加 database_verification。run_version 是本次客户端开始时间的纳秒标识；数据库版本策略另外记录：ClickHouse 沿用表内服务端纳秒默认版本，MongoDB 按 `(date,code)` upsert。两者不能混作客户端版本。

来源 SHA-256 针对实际传给解析器的同一份字节快照，包括 day、gbbq、前因子和外部日历。单个 day 限制32MiB，gbbq/前因子限制64MiB，日历限制1MiB；读取本身有界。East 摘要针对转换后的完整接口数据，`source=eastmoney:normalized_payload`，不冒充原始 HTTP 正文。股票名单选择及下载地址仍以调用参数和缓存记录为准。

日线文件内部按日期规范化，同一证券的多个输入日期区间重叠时拒绝发布，即使区间实际缺口可能互补。这样可用与源文件数成比例的内存证明逻辑键唯一性，不需为数千万行建立全局 HashSet。CSV 发布前流式回读校验行数、键所属区间及日期边界，再记录最终字节 SHA-256。东财JSON由已校验记录直接序列化并记录输出摘要，不执行CSV回读。清单的 logical_keys 是经过来源检查的输出键数，不是数据库查询结果。

day先用128KiB缓冲复制并计算SHA-256，随后只解析私有磁盘快照；每块最多4096条，质量检查承接上一块日期，累计收益计算承接末日收盘价/因子。A股要求源日期严格递增，可直接写入私有CSV；非A股在固定块内排序，再以最多8路、多轮磁盘归并处理跨块乱序和重复，保留源最后一条。每源最多256块，单day32MiB限制不变。每类质量详情最多64条，FileReport的errors_omitted/warnings_omitted记录省略数量，省略不放宽strict判定。

输出顺序固定为目录参数顺序、目录内文件名升序、文件内日期升序；`-n`在文件名排序后对每个目录取前N个。每线程结果通道容量1，只传私有CSV及质量元数据；等待或错误退出会先释放接收端以解除发送阻塞。记录相关内存随线程数/固定块增长，不随单文件总记录增长；文件列表、清单和日期区间索引仍与源文件数成比例。gbbq/前因子仍是各64MiB有界内存快照。磁盘快照和暂存CSV增加临时I/O/空间需求，正常完成、失败和协作取消会清理；强制终止可留下临时文件，不能据此视为已发布结果。

运行状态：

| 状态 | 含义 |
| --- | --- |
| running / parsing | 尚未发布新输出；进程异常结束时保留running |
| artifact.publication_pending | 已记录待发布文件摘要；是否已rename需核查 |
| artifact.published | 已确认输出文件原子发布 |
| database_status.not_started | 尚未发起数据写入 |
| database_status.unknown | 写入即将或已发起；异常/超时不能证明数据库没有部分数据 |
| database_status.acknowledged | 已读完ClickHouse成功响应，或mongoimport退出码为0 |
| database_status.verified | 已独立回读每个导出键及全部导出数值，行数与日期边界一致 |
| artifact.cleanup_pending | 数据库回读通过，CSV删除/更名结果需要核查 |
| complete / failed | 流程已结束；具体失败阶段和已确认动作仍在清单中 |

数据库失败保留已发布CSV。回读通过后默认删除CSV，`day -k` 更名保留为 `<output>.csv`；清单保留摘要及确认的文件状态。ClickHouse读取完整响应并检测HTTP错误、异常头、HTTP200末尾异常和截断正文，带query_id发送INSERT；单次HTTP操作上限300秒。MongoDB导入子进程、回读每批请求及最终退出各最多300秒，且都受整任务绝对deadline与取消约束；超时终止并回收。导入前再次检查CSV字节摘要，拒绝清单记录后被修改的文件。

默认每批4096个CSV键，`RUSTDX_VERIFY_BATCH_ROWS=1..16384`可调整。ClickHouse用独立`SELECT ... FINAL`（查询解析上限1MiB），MongoDB每次导入启动一个长驻`mongosh`逐批查询，单请求/响应、管道缓冲和工作线程均有界。逐键对比open/high/low/close/amount/vol；输出含preclose/factor时也逐值核对。数值按Float64精确相等比较，JSON开启float_roundtrip，不用宽容差掩盖漂移。缺键、重复键、字段类型错误、任何数值变化或查询失败均拒绝verified。回读只针对本次导出键，不代表库内全部历史数据或全部应有证券已经验收。验收内存与总行数无关；单批回读文本上限16MiB。MongoDB还需确认最后一批后的stdout结束与进程退出码，迟到错误不能清理CSV。

schema5清单包含回读行数、逻辑键数、批次数、配置批量、日期边界、字段列表及规范化回读值的SHA-256；`sha256_order=csv_row_order`让摘要独立于批量大小。schema3/4按批内键排序，旧新摘要不一定可直接比较；CSV字节摘要不受影响。`fetch_elapsed_ms`是各批获取与解码的串行墙钟耗时之和，`elapsed_ms`覆盖CSV读取、比对和摘要；长驻进程的首次响应包括启动等待，最终退出确认在这两个字段之外。

当前schema6继续保留上述字段，新增每任务`observations`和`freshness`；阶段累计耗时/活跃时间并集、临时文件逻辑字节范围及来源自然日年龄的准确口径见[OBSERVABILITY.md](OBSERVABILITY.md)。阶段耗时不可相加为CPU时间或整命令耗时。

MongoDB需要PATH中的`mongosh`和`mongoimport`，不再依赖`mongoexport`；`MONGODB_URI`默认`mongodb://127.0.0.1:27017`。写入前要求完整非稀疏、非部分的`(date,code)`唯一索引与simple排序规则；缺少时创建索引，已有重复数据或冲突索引会在发布新CSV前失败。键为CSV日期字符串与含市场前缀代码，数值列必须回读为JSON数值。集合不存在时先创建索引，不把空集合当查询失败。

长驻mongosh默认使用`NODE_OPTIONS=--max-old-space-size=128`促进批间垃圾回收；已有NODE_OPTIONS由部署方控制，不会被覆盖。128MiB只限制V8老年代，原生内存和其他堆区域不在其中；实测进程最大RSS与资源取舍见[第三轮记录](OPTIMIZATION_ROUND3.md)，部署须给完整进程树预留内存。

同机、同数据库地址与表的受管day/east任务使用持久锁文件，在采集/前因子读取至验收结束期间互斥，跨工作目录有效，进程退出自动释放。锁标识排除凭据，统一localhost与默认端口；任意DNS别名、不同副本集种子集合、多主机和外部写入者仍需共享调度或数据库侧协调。回读是该时刻的快照，不保证外部写入者随后不会改动数据。

清单没有提供跨文件、跨数据库事务，也不会自动重试不确定写入。恢复前应使用CSV摘要、目标表、query_id及服务端记录核实。acknowledged但未verified的失败可能已经完整写入数据库，保留CSV以便核查。

故障回归覆盖每次运行归档、发布前回读、输入重叠、迟到HTTP异常、子进程非零退出及超时。离线回归不代表生产数据库已验收。

本次保留的两个Git历史标签只用于源码版本检出，运行清单归档不会自动裁剪成两份，保留期限由部署方管理。源摘要不替代输入备份，源码回退也不会回滚数据库或发布文件；见[RELEASES.md](RELEASES.md)。
