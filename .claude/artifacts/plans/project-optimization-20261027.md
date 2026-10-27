# rustdx 优化实施方案

> 状态：方案经单模型规划、架构质疑和验收审查后可用于任务拆分；尚未实施，也不代表生产验收通过。<br>
> 来源：用户请求“全面分析下当前项目，输出优化方案”；[分析报告](file:///Users/bytedance/Documents/rustdx/reports/project-optimization-20261027/项目分析.md)。<br>
> 模式：默认；2轮 / 最多3轮。<br>
> 基线：`18cc661d1b0ff0169dcbfcaa571f48a5b96dcdf3`，版本2.0.2。<br>
> 更新：2026-10-27。主要工程决策路由：`architecture-decisions`，置信度高；目的为维护阶段的优化决策，保留现有责任边界并减少正确性与运行成本风险。

## 1. Requirements summary

方案覆盖当前核心库、CLI、runner、事实维护、测试发布、部署恢复和文档，按照P0/P1/P2安排优化，并为每项给出位置、依赖、成本及可验证完成条件。已经实现的优化继续保留；新增建议以本次源码及核验发现为依据。

本轮只输出方案。后续实施按独立分支/工作目录推进；不改冻结证据，不手动递增版本，不默认为公共源授予准确、完整历史或被动推送资格。

| 目标 | 衡量方式 |
| --- | --- |
| 单一可交付身份 | 主线、候选、实际包、二进制和检查记录能绑定同一提交/树 |
| 可消费数据正确 | 所需日期、单位、完整性及语义资格有独立范围证据，缺口明确拒绝 |
| 可解释容量 | 采集、数据库证明、全目标证据、进程树和长期留存均有预算 |
| 可恢复运行 | 不明确写入不重试，固定版本保持，隔离恢复能精确核验 |
| 有依据的性能提升 | 同环境同输入对比；输出和资源约束不退化 |

当前未知：实际生产范围、目标部署负载、允许的分钟新鲜度、长期保存期及批准RPO/RTO。下文给出可以先实施的确定项，并将这些值明确留作阶段准入输入；不因缺少生产参数停止静态和工程改进。

## 2. Acceptance criteria

| AC | 完成条件 | 对应任务 |
| --- | --- | --- |
| AC-01 | 候选与原目录修改分别核对；可交付树明确；原文件无覆盖；同树完成包/消费/质量检查 | B01 |
| AC-02 | 实际消费范围的日历/规则/点时事实独立审核合格；越界、缺事实及Unknown仍被拒绝 | B02 |
| AC-03 | 已知源差异逐项保留并分类；提高资格的每个范围有独立依据，其余仍不可消费 | B03 |
| AC-04 | 被动与主动刷新来源可机器区分；分钟档案的日期/槽/量/修订/保存期合同成立，缺口不伪装完整 | B04 |
| AC-05 | 回收失败有可解释根因与回归；取消/超时、pipe EOF及后代状态均可核对，不明确状态阻断发布 | B05 |
| AC-06 | 历史任务在采集前显示数据库操作/证明/证据预算；分批窗口完整接续，缺预算在写入前拒绝 | B06 |
| AC-07 | 指定Linux目标的权限、硬配额、进程树、调度及恢复证据齐全，RPO/RTO达到事先确定值 | B07 |
| AC-08 | 应有Job、事实截止、缺键、资格及指标失效能触发正确告警；假期/停牌测试不误报缺行情 | B08 |
| AC-09 | 缓存改动在同输入/同环境下有收益，跨进程及崩溃后用量一致，原键/期限/损坏检查保留 | B09 |
| AC-10 | 采集优化同输入CSV摘要一致；有界队列/RSS、取消和慢来源拒绝行为不退化 | B10 |
| AC-11 | 新证据分发路径可取回并逐字节重放；历史摘要/来源不变；CI缺证据时明确失败 | B11 |
| AC-12 | 外部未执行不计为在线通过；fuzz corpus与崩溃产物可恢复；性能门禁比较同环境原始数据 | B12 |
| AC-13 | 公共API路径和已声明行为兼容；资格没有被合并弱化；独立调用方覆盖新旧接口及拒绝边界 | B13 |
| AC-14 | 当前schema/指标/实际模块与权威文档一致；导航、生命周期、FR01–18及版本检查通过 | B14 |

这里的“方案完整”是以上验收条件已经定义；“任务完成”必须等实际实现和检查满足相应AC。本次报告不把B01–B14标为已实施。

## 3. RALPLAN-DR

### 3.1 Principles

1. 沿用Rust2024/MSRV1.89、同步I/O、有界线程及核心/应用责任分离。
2. 先解决交付身份、已核实事实与消费资格，再扩大业务范围。
3. 保留原wire、原单位、源差异和冻结证据；派生解释另存，不能覆盖原值。
4. 原子文件、同机锁、账本事务、insert-only及固定catalog不能因性能优化而放松。
5. 改动由测量或确认缺陷触发，先采用最小可逆方案；不重构已正常工作的邻近代码。

### 3.2 Decision drivers

| 驱动 | 当前证据 | 对选择的作用 |
| --- | --- | --- |
| 数据正确性和可交付身份 | 候选未合入，规则核实越界，源差异未闭合 | 决定先做B01/B02，建立范围性拒绝 |
| 可恢复性与资源可解释 | 本机账本/版本模型健全，回收偶发与大历史证明预算待解 | 保留发布模型，完善诊断和前置规划 |
| 维护/运维成本 | 已有广泛质量门禁，证据占约96.71%跟踪字节 | 扩展现有机制，避免新增分布式运维成本 |

### 3.3 Viable options

| 方案 | 实现思路与位置 | 优点 | 代价与适用条件 |
| --- | --- | --- | --- |
| A：保留现有模块，分阶段闭合合同 | 现有`core/src/tcp/stock`、`CLI/src/tcp_day`、`runner/src/{pipeline,database,ops}`；按B01–B14实施 | 复用回归与恢复机制，改动可独立评审 | 仍需外部事实/目标环境；公共源不确定性不能由代码消除 |
| B：独立分钟采集与本地归档组件 | 新组件由独立ADR定义；复用core decoder/operation和现有留证工具，不能直接套日线runner契约 | 能从采集起点保证自有档案保存期，分钟任务独立资源/发布边界 | 新增闭合槽、修订、缺口、存储及运维合同；历史缺口仍需独立来源 |
| C：保持源码现状，仅收紧应用使用范围 | `APPLICATION_INTEGRATION.md`、`TASK_ADMISSION.md`，部署端只允许已核验日期/来源 | 低改动，可临时降低误用 | 不解决原目录旧路径、交付漂移、回收诊断和规模问题，只适合作为过渡 |
| D：大规模异步/多机改造 | 涉及TCP生命周期、调度、发布协调和部署模型 | 若有明确独立伸缩/隔离需求，可提供新上限 | 当前没有实测收益和跨机失效写入隔离依据，不选；需新需求和故障模型 |

推荐A；若主业务要求可承诺的分钟长期档案，B作为单独合同的并行任务，而非等待公共源证明固定TTL。C是证据未闭合期间的运行策略。D需测量证明同步有界架构无法满足已定义负载后再评估。

## 4. Implementation steps

路径中的`core`、`CLI`、`runner`分别指`crates/rustdx-core`、`apps/rustdx-cli`、`apps/rustdx-runner`；省略前缀的模块沿用同任务内最近的crate。B01/B03/B04的“候选”仍是未合入源码，详见配套分析的身份表。

### 4.1 第一阶段：交付身份、事实与权威合同

#### B01 · P0 · 统一交付源码 · M

- 位置：候选`crates/rustdx-core/src/tcp/stock/{quote_subscription,finance_semantics,recent_minutes,daily_reference}.rs`及其测试；原目录`docs/testing/VALIDATION.md:1-10`、`scripts/validation/verify_database_recovery.py:389-400`；`scripts/release/{preflight,check_consumers}.py`。
- 措施：先核对累计补丁基线和最终树；分别记录原目录两项修改的意图及必要性，再形成选定变更树。候选的固定范围资格、源差异、回归及文档成套保留；恢复脚本引用真实生产模板路径。使用新的工作目录，避免覆盖用户文件。
- 依赖：无；若重新基于更新主线，重建身份和核验，不以原树摘要授予新树通过。
- 验收：AC-01；实际打包/解包/独立调用方与全部质量检查绑定该树；补丁未包含的修改不得偷偷加入。
- 收益：应用能明确使用修复；降低“报告已修、实际源码未修”的风险。撤回成本为保留未合入PR和补丁，较低。

#### B02 · P0，当前规则/全市场使用前 · 完成范围化事实维护 · L

- 位置：`core/src/limit/rules.rs:47-67`、`calendar/provider.rs:152-212`、`limit/history.rs`；`config/maintenance/facts.example.json`、`scripts/validation/audit_fact_maintenance.py`、`CLI/src/maintenance.rs:21-114`。
- 措施：为实际范围维护独立注册表及原公告；分别审核规则、年度日历、上市退市、ST/停复牌和点时名单。规则截止仅在审核后更新；更早历史用覆盖完整catalog的外部日历。新年度公告未完整取得时继续越界拒绝。
- 依赖：独立事实及承担审核的角色；先做范围小但完整的集合，不能拿当前全市场名单回填历史。
- 验收：AC-02；审计返回0且required_through等于实际任务截止；规则边界、IPO/ST/停牌、缺区间、未来年度均有正反例。
- 收益：恢复所需日期的规则可用性及业务发布资格。事实版本更新可回退旧范围，但不能假装旧版本覆盖新日期。

#### B14 · P1文档事实 / P2自动化 · 校准文档 · S/M

- 位置：`docs/architecture/PRODUCTION_FUSION_PLAN.md`、`docs/operations/OBSERVABILITY.md:32-38`、`docs/requirements/TRACEABILITY.md`、`config/documentation/registry.json`及`check_documentation*.py`。
- 措施：修正schema4迁移、汇总投影和实际验证模块；共用消费合同继续维护在`APPLICATION_INTEGRATION.md`。增加schema/命令/模块的机器锚点；只记录真实核验日。
- 依赖：B01选定树；与受影响FR、权威合同同PR更新，历史分析及冻结证据不改。
- 验收：AC-14；导航、生命周期及版本检查通过；人工核对迁移/回退命令与schema；本次原目录缺标记问题明确结案。
- 收益：降低错误运维指令和不一致契约的风险；不需要以大范围文档重写开始。

### 4.2 第二阶段：上游差异、更新来源和回收正确性

#### B03 · P1 · 解释源差异并扩大可验证范围 · L

- 位置：候选`core/src/tcp/stock/daily_reference.rs`、`recent_minutes.rs:153-225`、`finance_semantics.rs`；`tests/{daily_reference,minute_reference,finance_q3,finance_cash_basis}.rs`，`docs/testing/PRODUCTION_ACCEPTANCE.md`。
- 措施：以证券/日期/端点/契约/原bits建立差异清单，分别检测精度表示、量单位/累计、午间/尾点分桶、除权/来源修订和不可解释冲突。独立资料明确后扩大范围，过期或不同raw撤销资格。
- 依赖：B01、B02，以及与待核验范围匹配的独立公告/分钟参照。
- 验收：AC-03；原44日线、33跨节点分钟键及35/106端点差异不被抹去；每个被授予的范围有证据，未授予范围仍拒绝。金融固定现金解释不得升级原冲突字段。
- 收益：减少错误消费；不承诺公共源最终可提供统一真值。

#### B04 · P1 · 明确实时模式，制定自有分钟档案合同 · L

- 位置：候选`quote_subscription.rs:84-110,187-199,438-535`、`recent_minutes.rs:13-151`；`examples/validation/validate_quote_push.rs`、`examples/capture/capture_recent_minutes_boundaries.rs`；`TASK_ADMISSION.md`、`APPLICATION_INTEGRATION.md`。
- 措施：交易时段独立观测无客户端请求期间的入站帧，绑定节点/会话/代次/游标/时点；未通过则使用已标识的请求刷新。另在独立ADR定稿分钟档案的来源、日期、槽、量口径、日闭合、修订、缺口、保存期、预算及重放方式，再创建组件或runner新契约。
- 依赖：B03适用范围、实际交易窗口；分钟持久化的业务新鲜度和保存期需在其实施前定稿。核心缓存TTL不控制源历史或档案保存期。
- 验收：AC-04；建议至少两个节点、两个交易日及一轮断线恢复，记录真实PassivePush数量；没有真实被动帧时判未通过。档案已声明范围逐日/逐槽核对，不用单点非空下界证明连续历史。
- 收益：更新来源透明，自有档案保存期可控。新分钟契约是独立改动，不挤入日线表。

#### B05 · P1 · 回收根因诊断和最小修复 · M

- 位置：`runner/src/supervisor.rs:17-59,169-215`、`database_process.rs:125-198,253-305`、`tests/control_plane.rs`及Unix CI。
- 措施：记录signal返回、PGID、leader/后代状态、pipe EOF、回收时间线；用通道确认子进程进入阻塞点后触发取消/超时。在macOS与Linux、空闲与并行负载下诊断，仅修证实原因。
- 依赖：可控复现环境；若无法复现保留未结问题和采证，不声称修复。
- 验收：AC-05；建议每种环境100次压力回归为工程采样，必须保留零未解释失败、无活动后代和旧输出/目录不变的证据；历史失败原因有单独说明。不能以自动重试后成功作为通过条件。
- 收益：可靠恢复与CI稳定性；若问题为OS僵尸可见延迟，要区分状态观测与可运行后代，不改变不明确状态的阻断原则。

#### B12a · P1 · 外部验收状态显式化 · M

- 位置：`core/tests/{tcp,auto_reconnect,bars_range_live,minute_time_live,k_batch_adjusted}.rs`、`tests/integration/tests/stocklist.rs:25-65`及`.github/workflows/ci.yml`。
- 措施：外部用例使用显式ignored/单独在线入口，或记录机器可读的executed/skipped/failed；严格在线任务缺配置、无服务或无数据应非零。离线回归的模拟服务保留。
- 依赖：B01；不要求普通PR连接公共市场。
- 验收：AC-12的状态部分；离线报告明确列出外部未执行，独立在线任务没有提前返回的假通过。
- 收益：让测试数量与实际验证范围一致，防止CI数字被误用为业务可用证明。

### 4.3 第三阶段：历史容量与目标运行闭环

#### B06 · P1 · 历史窗口及数据库容量plan · M

- 位置：`runner/src/database/capacity.rs:6-82`、`database.rs:612-644`、`pipeline.rs:835-935`、`runner/src/pipeline/plan.rs:23-169`、`scripts/bench/measure_database_readback.py`。
- 措施：复用容量公式，在只读plan中输出每目标操作、证明上限、全目标证据字节及余量。按完整日期窗口生成历史计划并验证catalog连续性；每个日期保持完整应有名单。收集实际证明大小/预留使用率，评估保守估算的真实拒绝范围。
- 依赖：B02提供覆盖全历史的日历及点时事实；来源真实可用范围明确。
- 验收：AC-06；预算刚好不足时在任何数据库传输前拒绝；相邻日期窗口接续后全键/字段一致，跨窗修订不降低source_as_of/修订号；plan不创建账本、不联网。
- 收益：大历史任务更可预测，减少采集完成后才因数据库证据预算拒绝。首选现有窗口分批；版本化分段证明需要另行设计，当前不实施schema迁移。

#### B07 · P1 · 单一目标Linux部署验收 · L

- 位置：`deploy/production/runner/{rustdx-runner.service,README.md,alerts.yml}`、`ops.rs:210-310`、`scripts/validation/{verify_local_recovery,verify_database_recovery}.py`及`measure_process_tree.py`。
- 措施：固定提交/两个二进制SHA、Job/事实及目标身份；落实磁盘硬配额、进程树限制、独立只读/发布账号和必要ACL。完成实际业务范围采集、停发/unknown隔离、源/数据库异常与新目录恢复。consumer-audit单独备份/留存；需要敌对worker隔离时采用不同身份。
- 依赖：B01/B02/B05/B06、实际平台及事先确定的业务截止/RPO/RTO。不在本次方案中擅自指定生产数值。
- 验收：AC-07；至少覆盖健康任务和取消/强杀/确认丢失/磁盘满；所有失败保持旧正式版本，恢复后全键/字段/约束一致；资源峰值在目标硬配额内。
- 收益：证明目标能运行和恢复，明确权限边界。初期在新目录/命名空间验证，切换为最后一步。

#### B08 · P1 · 业务SLO与告警 · M

- 位置：`runner/src/{observations,ops}.rs`、`CLI/src/observation.rs`、`docs/operations/OBSERVABILITY.md`、`deploy/production/runner/alerts.yml`。
- 措施：在现有汇总旁增加所需业务信号：应有Job发布截止、事实剩余期、缺/额外键、资格缺口、实际更新来源和consumer-audit增长。跨任务延时使用统一样本/直方图；不合并各任务P95。
- 依赖：B02和实际Job/日历；指标和告警定义需与业务完成标准一致。
- 验收：AC-08；本地场景验证正常交易、假期、核实停牌、事实缺失、未结束attempt、指标失效和空间不足分别触发预期；目标环境验证实际通知和恢复解除。
- 收益：发现“技术成功但所需数据未发布”。保留原观测与可重建投影，不丢弃审计原始记录。

### 4.4 第四阶段：按实测优化性能与维护成本

#### B09 · P2 · 文件缓存 · M

- 位置：`core/src/cache.rs:400-608,847-937`、`crates/rustdx-core/benches`及缓存模块回归。
- 措施：先记录目录打开/读取次数、锁等待、命中/写入P95、总用量和有效并发。候选A为分桶+整体预算，候选B为可重建容量索引；根据多进程共享需求和测量选择。同键合并已存在，条带碰撞仅在证据显示重要时优化。
- 依赖：性能基线；不能把缓存当权威来源或降低日期/语义资格。
- 验收：AC-09；建议用1,000/10,000条真实可重放条目及1/4/8并发，对同一输入至少5轮比较；中位改善超过10%才进入性能改动，P95不恶化超过10%，两项为建议工程门槛。强杀重启、覆盖、过期、损坏及容量边界回归通过。
- 收益：降低重复扫描和串行锁成本；未测前不承诺收益百分比。若未达到门槛，保留现实现和采样。

#### B10 · P2 · 有序采集流水线 · M

- 位置：`CLI/src/io.rs:80-168`、`io/streaming.rs:20-150`、`tcp_day/acquisition.rs`；`measure_day_memory.py`、`measure_process_tree.py`。
- 措施：用固定真实来源测量快慢文件混合及重复获取中的snapshot/parse/spool/replay/readback成本。有顺序等待瓶颈时改为全局有界任务分派+序号spool，或限制窗口的采集/回放流水线；保持总许可/行/字节预留，避免无界提前完成队列。
- 依赖：B06容量合同；测量证明等待或重复I/O为主要成本。
- 验收：AC-10；1/4/8worker输出SHA相同，跨块因子/质量一致；取消、慢来源、缺页/额外页和磁盘不足保持拒绝；建议端到端中位耗时改善超过10%、峰值进程树RSS及临时磁盘不增加超过10%，在相同输入/5轮对比中核对。
- 收益：提高固定资源下利用率；不将微基准结果升级为生产吞吐承诺。

#### B11 · P2 · 证据分发与长期留存 · M/L

- 位置：`docs/testing/evidence`、`scripts/release/evidence_manifest.py:49-164`、`scripts/validation/evidence_paths.py:15-33`、`.gitattributes`、`scripts/ops/retention_preview.py:94-150`及`.github/workflows/ci.yml`。
- 措施：复用已有inventory/verify/bundle及冻结属性，落实尚未配置的不可变存储和按需取回；普通开发试验可选sparse checkout，需要完整证据的CI步骤显式取得。运行release、consumer-audit、数据库证据和备份分别规定保存期/硬配额，先保留只读预览。
- 依赖：每种证据的owner及存储可用性；历史来源、字节和摘要不得改写。不能把release所有引用仍受保护的事实解释为可自动删除。
- 验收：AC-11；断网/损坏归档/缺文件明确失败；取回后全部摘要及重放一致；历史清单核验不变；比较冷checkout文件数/耗时和CI所需取回量。
- 收益：降低源码工作树和重复分发成本；不保证Git压缩包同比缩小。旧历史不重写，转向成本有限。

#### B12b · P2 · 完善fuzz、端到端基准和CI开销 · M

- 位置：`tools/fuzz/fuzz_targets/{checked_parsers,modern_parsers}.rs`、`.github/workflows/ci.yml:188-257`、`crates/rustdx-core/benches/indicators.rs`及`scripts/bench`。
- 措施：从已取得的真实帧构建冻结种子，加入合法前缀、截断、超长计数、压缩边界等变异；建立长时任务及崩溃corpus上传。增加cache/采集/数据库基准，再按CI实际任务时间评估缓存或变更路径选择。未知依赖变更仍全量运行，不能简单跳过质量/发布门禁。
- 依赖：B01/B09/B10及实际CI观测；没有远端结果前不承诺节省分钟数。依赖安全审查记录工具/漏洞库时点，不无依据大升级；ureq固定transport接口升级须重跑取消/TLS。
- 验收：AC-12；建议定时每目标至少10分钟、保留随机种子/corpus/崩溃最小化输入，PR保留smoke；有语义边界覆盖证据；性能对比绑定输入SHA、工具链、平台和二进制。
- 收益：提高深层解析和性能退化发现能力，减少无证据的门禁裁剪。

#### B13 · P2 · API消费与局部模块边界 · M

- 位置：`core/src/lib.rs:3-37`、`error.rs:250-278`、`indicators/incremental.rs:6-12`、`cache.rs`、`tcp/stock/client.rs`、`runner/src/{database,pipeline}.rs`、`examples/usage`。
- 措施：补完整checked调用链和owned持久化示例，明确两类IndicatorError及各资格轴；先通过别名/转换/说明减少歧义，不立即删除公开错误类型。下一次真实变更时按存储、状态机、适配职责拆文件，保持重导出路径；不因行数单独立大重构。
- 依赖：B01选定API和B14权威文档；只有多个真实调用点才抽共享层。
- 验收：AC-13；当前/旧签名独立消费者预期不变，owned转换保留市场及全部契约维度，业务拒绝不被丢弃；适用相关测试与包消费通过。
- 收益：降低误用及审查难度；避免新错误体系/泛化DTO造成迁移成本。

## 5. Workspace setup

实施前执行`git status --short --branch`和`git worktree list`，记录已有修改；每项任务按仓库规则使用独立分支与工作目录。原目录已dirty，不覆盖、混合或恢复其文件；已授权的后续任务可以自行建立隔离worktree，无需反复确认。

本次分析位于`analysis/project-optimization-20261027`；行情候选仍在`fix/upstream-capabilities-20261027`。后续B01应审查实际候选，不能从分析目录误认为最新修复已经存在。其他任务可以从选定主线提交新建 `optimization/<任务>`。

开发/提交/功能分支推送不变版本；通过PR合入main。远端main push由CI只递增一次，全部质量检查通过才打v标签。不得以报告生成授权推断用户要求提交、推送或部署。

## 6. Risks & mitigations

| 风险 | 可能性 / 影响 | 缓解、失败动作和责任入口 |
| --- | --- | --- |
| 把候选测试通过套到主线或原目录 | 当前存在 / 修复实际不可用 | B01绑定树/包/二进制；`preflight.py`、PR审查；身份不符拒绝交付 |
| 仅刷新规则/事实截止 | 有诱因 / 错误规则与覆盖 | B02原公告+独立语义审核；`audit_fact_maintenance.py`；不合格拒绝任务 |
| 用多节点一致或容差消除冲突 | 可能 / 错误行情消费 | B03保存双方raw、按范围资格；差异未解释不授予准确性 |
| 把RequestedRefresh算PassivePush | 可能 / 实时能力误报 | B04来源枚举和无请求时段留证；未观测保持未验收 |
| 清理只看到leader结束 | 历史单次失败 / 后代占资源或恢复不安全 | B05采PID/PGID/状态/EOF；不明确后代阻断；supervisor回归 |
| 按证券切分同一天造成部分日期发布 | 可能 / 业务缺键 | B06只按完整日期窗口分批；pipeline应有键与catalog连续性检查 |
| 缓存索引漂移或分桶扩大容量 | 可能 / 资源越限、损坏数据 | B09崩溃重建/整体配额，原子文件与原键保留；失败回退旧后端 |
| 采集提前完成队列无界 | 可能 / RSS和spool增长 | B10总窗口预留/有界通道；进程树及磁盘阈值不通过则撤回改动 |
| 外置证据不可取得 | 可能 / 复现和CI失败 | B11不可变归档、摘要、回取失败显式拒绝；历史文件先保留 |
| 同UID worker访问publisher资产 | 受信worker时较低，敌对worker时高 / 权限边界失效 | B07目标威胁模型+不同身份/ACL实测；不能用env_clear声明隔离 |
| 只有控制层备份，缺数据库/查询审计 | 当前边界 / 灾难恢复不完整 | B07静止一致边界、服务端备份、审计另备；新目录全量核验 |
| schema文档陈旧但静态检查通过 | 当前存在 / 迁移/恢复误操作 | B14机器锚点+权威合同人工核验，同PR更新FR及日期 |

## 7. Verification steps 与架构持续检查

### 7.1 常规工程检查

在每项选定源码树上使用仓库工具链及锁执行：

```sh
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked --no-fail-fast
RUSTDOCFLAGS="-D warnings" cargo +1.89.0 doc --workspace --all-features --no-deps --locked
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/validation/tests
python3 scripts/validation/check_documentation.py
python3 scripts/validation/check_documentation_lifecycle.py
python3 scripts/release/versioning.py check --strict
python3 scripts/release/check_consumers.py --output /private/tmp/rustdx-task-consumers
```

离线环境为Cargo添加`--offline`。外部/数据库/Unix ignored用例按任务实际条件另跑；包预检、独立consumer、数据库及fuzz不把“配置存在”当作通过。本次报告585通过/28忽略只证明干净基线的本次macOS离线检查。

### 7.2 Fitness functions

| 属性 | 指标与阈值/规则 | 来源与频率 | 失败动作 / 本地入口 |
| --- | --- | --- | --- |
| 依赖方向 | core不依赖CLI/runner/SQLite发布控制 | Cargo metadata，每次涉及依赖的PR | 拒绝倒置；`Cargo.toml`及crate manifests |
| 公共消费兼容 | 新旧签名调用方符合预期，所需资格不降低 | 实际包/独立consumer，每次API变更 | 拒绝交付；`preflight.py`、`check_consumers.py` |
| 日期/语义资格 | 未核实/超期/Unknown不能发布；独立证据范围绑定 | B02–B04夹具及事实审计，每次来源/事实变更 | 停止该范围任务；`TASK_ADMISSION.md`、相关decoder测试 |
| 受管并发 | 同目录受管许可不超过设置；队列/行/字节预算不超 | network/shared_network及进程树测量，每次并发变更 | 拒绝性能改动；`network_budget`、`shared_network` |
| 延时/吞吐预算 | 测量先固定输入和平台；B09/B10建议中位改善>10%，P95/资源按各项门槛 | 原始样本/输入SHA，至少5轮；性能PR | 未达门槛保留旧实现；`scripts/bench` |
| 发布隔离 | 任一目标不verified/出现unknown时目录不推进；旧版本仍可固定读 | 故障/控制层回归，每次发布/恢复变更 | 阻断发布/进入reconciliation；`control_plane`、`verify_database_faults.py` |
| 保存与恢复 | 可达release/证据摘要不变；恢复全键字段一致、目标RPO/RTO达标 | 备份/留存预览及恢复演练，每次策略变更和定期目标演练 | 禁止切换；`retention_preview.py`、两类recovery脚本 |
| 文档生命周期 | 当前文档schema/命令与选定树一致，FR01–18、导航/登记合格 | 自动检查+合同人工review，每次PR | 文档未同步拒绝合并；`check_documentation*.py` |

生产延时、吞吐、Job截止、RPO/RTO的数值必须在B07实际目标验收前登记，并用真实负载验证；10%门槛只用于判断是否值得合入性能改动。没有记录值或真实测量时，生产性能项判“未验收”。

### 7.3 关键运行依赖的采用与退出

| 依赖 | 排查/变更方式 | 故障退化 | 隔离或退出 |
| --- | --- | --- | --- |
| 公共行情节点 | 请求/压缩帧、消息身份、节点能力观察、独立参照 | 换已核验来源或明确请求刷新；不能跨节点拼接未冻结范围 | 保留raw与源身份；自有档案只覆盖已验证范围 |
| SQLite与本地持久盘 | integrity/foreign_key、WAL在线备份、根锁、空间与catalog检查 | 保留旧正式版本，不对不明确状态自动发布 | 新目录restore及固定版本核验；不迁移到未经验证共享盘 |
| ClickHouse/MongoDB | query_id、原请求/响应、完整键/字段、索引与角色 | unknown不重写；独立文件版本仍可读 | 静止一致备份或新namespace/release重建，目标变更另验 |
| 事实维护输入 | 原公告、责任注册表、覆盖与复核日期 | 超期/缺状态拒绝，不推断停牌和未来规则 | 原子切换版本，新范围独立审查；旧版本仅用于旧覆盖 |
| 外置证据存储 | 不可变对象、摘要及可执行重放入口 | 缺证据不授予结论 | 历史证据仍保留；新归档可重新分发而不变原字节 |

## 8. Planner pass

第1轮提出方案A：先统一修复候选、完成事实，再补上游差异和性能/容量证据。该规划复用现有同步与发布边界，避免重复实现已完成的EMA/MACD、共享缓存、会话复用和指标汇总。

第2轮纳入质疑：把“关闭上游问题”改成“按独立证据授予范围资格，余者拒绝”；分钟长期保存单独定合同；历史只按完整日期窗口切分；性能动作以同环境测量门槛触发。最终实施步骤为第4节B01–B14。

## 9. Architect challenge

对A的最强反驳：即使库和runner全部工程回归通过，公共源仍可能没有所需真实被动推送或完整分钟历史。持续补解析与测试不能创造不存在的上游能力；如果分钟长期档案是主业务，方案B能更早提供从采集起点可管理的存储责任，应该并行。

另一项矛盾是严谨留证与吞吐/空间：逐帧原字节、全字段读回及冻结版本造成真实I/O与容量成本。选择保留正确性证据，先减少重复扫描、固定工作和不必要分发；只有大历史仍受单release限制时才考虑版本化分段证明，接受其设计/迁移成本。

综合选择：A为现有日线工程主路，B作为具有独立分钟合同的任务；未取得真实能力时采用显式轮询/资格不足输出；容量、权限和恢复通过后才扩大范围。沿用模块化单机的原因是当前责任和故障域已经明确，分布式成本尚无收益证据。

## 10. Critic verdict

以下为同一模型内的分角色审查，不是独立团队或子代理评审。

| 维度 | 第1轮 | 第2轮 | 依据 |
| --- | --- | --- | --- |
| Principle-option consistency | 通过 | 通过 | 同步有界、冻结证据及发布模型保留 |
| Fair alternative exploration | 通过 | 通过 | B确有分钟档案收益，C有过渡价值，D有明确再评估触发 |
| Risk mitigation clarity | 修订 | 通过 | 明确无法保证上游能力、分日窗口、容量索引重建和同UID条件 |
| AC testability | 修订 | 通过 | 不要求把不可观测能力“修成通过”，改为范围授予/拒绝 |
| Verification concreteness | 修订 | 通过 | 同树工程命令、特定故障和建议性能门槛明确 |
| File/line coverage | 通过 | 通过 | 所有B任务给出实际模块/文件位置，新增组件需独立ADR |
| Residual uncertainty | 保留 | 保留 | 真实负载、交易窗口和外部事实未取得，生产判据不能虚设 |

第2轮判定：APPROVED，含保留意见；这仅表示方案可以执行和评审，所有实现/生产结果仍未验收。

保留意见：B09分桶可能无法单独消除全局容量锁，需要真实多进程基线才可选；B06实际可接受历史窗口还受来源、事实和证据预算共同约束；B04历史缺口和PassivePush能力可能永远无法从现有公共源获得，不能把外部问题列成确定工期。

## 11. ADR

- **状态**：优化方案选定，未实施。
- **决策问题**：怎样在不破坏现有数据/恢复合同的前提下，提高当前工程可交付性、范围正确性、容量与维护效率？
- **Decision**：保留同步核心库、有界采集和单机版本发布；优先交付身份与事实，按范围关闭能力与恢复问题，然后采用实测支持的局部性能/分发优化。
- **Drivers**：独立事实正确性和可交付身份起决定作用；其次是恢复可解释性与维护成本。
- **Alternatives**：A选择；B在分钟长期保存需求成立时独立并行；C作为未验收范围的临时运行策略；D当前拒绝，缺负载及隔离收益依据。
- **正面影响**：复用现有质量门禁与故障模型，降低资格误用和无预算历史任务风险；大部分改动可单独撤回。
- **负面影响**：继续承担完整原字节和读回的成本；上游及生产证据不能承诺时间；分钟独立合同与运营工作增加。
- **Reversibility**：分桶、局部调度和文档/观测为可逆门，保留旧实现和新版本文件即可回退；外置证据必须保留可重放归档。若以后实施数据库证明/分钟存储schema变更，属于高转换成本门，需要旧消费者、迁移前备份和单独ADR。
- **Reconsideration trigger**：已定义真实负载证明同步有界架构仍不能满足吞吐/截止，或需要独立发布/权限/跨机故障隔离；此时才讨论异步/分布式。主业务以可承诺分钟档案为中心时提高方案B优先级。
- **Responsibility**：仓库维护者及各模块本地检查入口；数据事实由`config/maintenance`登记角色审核；目标部署/恢复由实际运维方负责。实际人员及生产值在阶段准入登记，不虚构已指定负责人。
- **Follow-ups**：独立上游范围核验；指定Linux目标容量/权限/恢复两条未关闭证据线。其余具体工作已列B01–B14，不扩展为额外架构项目。

## 12. Review trail

1. Planner v1：复用既有架构，先交付和事实，再能力/容量/性能。
2. Architect v1：公共源可能无法提供分钟/被动能力；留证与吞吐矛盾；按证券切窗会破坏日覆盖。
3. Critic v1：REVISE——能力关闭条件过强、生产阈值未定、分批边界及缓存选型需明确。
4. Planner v2：改为范围资格和拒绝；独立分钟合同；完整日期窗口；只把10%作为性能改动建议门槛。
5. Architect v2：支持A为日线主路/B条件并行，保留索引恢复与外部历史缺口风险。
6. Critic v2：APPROVED with reservations——需要实施前基线及目标准入输入；不得声称任务或生产通过。
7. 最终迭代：2 / 3。
