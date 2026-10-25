# A01–A12优化执行记录

来源：[系统优化方案](../archive/architecture/SYSTEM_OPTIMIZATION_PLAN_20261025.md)。基线提交`47045cd554de2f3237a7f2f2821146b47807c43b`，开始时仅有该分析报告及支持资料未提交。保留冻结证据，不自动发布、部署或裁剪历史。

| 编号 | 状态 | 交付与验证 |
| --- | --- | --- |
| A01 | 工程及本机回归完成 | 确定性期限/父子测试、同步取消、独立CI质量任务、fail-fast关闭及任务超时；远端待新提交运行 |
| A02 | 已修复并回归 | 实际父路径隔离；SQLite/文件/清单/标记总预算、独立备份数据库、恢复控制锁 |
| A03 | 已修复并回归 | 查询只读打开已有schema；writer须根锁，兼容open的初始化/迁移也排他；CLI migrate显式初始化/升级 |
| A04 | 已修复并回归 | restore标记保存原root、逻辑attempt定位不改账本；完整proof与I/O摘要核验；旧root离线后预览通过 |
| A05 | 维护工具及模板完成，事实待补 | 五类维护/独立复核/升级角色、来源摘要和复核期限审计；示例明确未核实并退出1，不续期、不发消息 |
| A06 | 工程及loopback回归完成 | 固定清单/catalog只读；按release复用会话，独立消费审计；全字段/末页/总数与CSV摘要不变，真实两库待新源码验收 |
| A07 | 已实施并测量 | schema4事务汇总/可重建、原记录保留、两项有依据索引；20,000条历史一致，迁移/溢出回滚及CLI RSS/耗时已测 |
| A08 | 已实施并回归 | 固定输入与动态SQLite/输出分别计量；journal/消失文件/最终完整对账及manifest总预算保护，计量成本单独测量 |
| A09 | 已实施并回归 | 离线应有键/初始并发/预算限制计划，不创建root/账本/网络目录；缺键/Unknown/小预算拒绝 |
| A10 | 工程分层及完整字节审计完成 | 工程scope1约25.28MB、归档7.13MB；就近夹具/来源清单；24,920份历史材料含12份本地collector共2.39GB核验；存储迁移/历史删除未执行 |
| A11 | 门禁工程完成，外部交付待执行 | 所有main/PR触发两库；最新同HEAD全平台/MSRV/质量/消费者实际包/数据库等门禁；官方Action SHA及Node24元信息固定，Mongo解压前SHA核验；未提交新SHA或发包 |
| A12 | 准入工具与部署合同完成，真实验收待补 | 日线/分钟/快照/0547独立矩阵；Unknown/缺字段/键/资源/权限/恢复拒绝；后三类无runner入口，不继承日线资格 |

Rust使用1.89、locked依赖；日志、构建和私有输入位于仓库外。关联回归后执行全workspace格式、Clippy、测试及严格文档；新结果另建支持包。

第一阶段：管理边界4项、control_plane 20项（真实数据库1项排除）、期限/取消3项、Python14项通过；工作区全部targets/features Clippy和最后runner关联Clippy均拒绝警告通过。七个实际CLI管理命令核验缺账本失败、migrate成功、目录别名/小预算拒绝、备份/恢复及旧root离线后的留存预览。第一次紧预算恢复测试发现SQLite辅助文件，改为独立DELETE模式备份后通过，保留失败日志。见[第一阶段实施](../archive/development/SYSTEM_OPTIMIZATION_STAGE1_20261025.md)和[支持资料](../testing/evidence/system-optimization-stage1-20261025)。

第二阶段：runner库12项、管理边界5项、实际CLI/control_plane21项（真实两库1项排除）、Python14项及全workspace Clippy通过。两日1,200行消费8→7请求、CSV SHA不变，但本次墙钟320.527→358.046ms，不宣称总吞吐改善。20,000条观测聚合与2,000个固定文件的重复计量分别减少固定工作；当前metrics CLI10次中位20.812ms、最大RSS11,665,408字节，仅为本机合成范围。schema3备份仍原样恢复后显式升级。见[第二阶段实施](../archive/development/SYSTEM_OPTIMIZATION_STAGE2_20261025.md)和[支持资料](../testing/evidence/system-optimization-stage2-20261025)。

最终阶段：Rust1.89工作区all-targets/all-features 534通过/10忽略，文档57通过/18忽略，
格式、全工作区Clippy拒绝警告、严格Rustdoc、22项Python、实际两包解包/独立新旧签名消费、
发布构建及当前文档导航通过。独立工程归档不含历史证据，534项Rust通过；修正原有
`acceptance/`忽略规则后补齐三份必需配置，Rust输入字节不变，两处22项Python通过。
实际CLI21项串行通过，真实数据库1项排除；并行冷编译时40证券用例触发原期限的失败也保留。
旧status启动测试改为显式migrate，紧凑夹具的分页遗漏已补回；不调宽生产期限、不改原件。
见[最终实施与范围](../archive/development/SYSTEM_OPTIMIZATION_FINAL_20261025.md)及
[最终支持包](../testing/evidence/system-optimization-final-20261025)。A05外部事实、
A01/A11新SHA远端执行、A06真实两库和A12目标生产继续待验收，不自动提交/推送/发布。
