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
| A10 | 待实施 | 工程归档与证据分发核验分层，保留历史原件 |
| A11 | 触发与工具固定已实现，交付门禁待完成 | main路径变更触发数据库验收；Mongo两工具官方来源归档摘要固定、解压前校验；正式同SHA检查后续完成 |
| A12 | 待实施 | 任务准入与目标部署验收合同，外部事实分列 |

Rust使用1.89、locked依赖；日志、构建和私有输入位于仓库外。关联回归后执行全workspace格式、Clippy、测试及严格文档；新结果另建支持包。

第一阶段：管理边界4项、control_plane 20项（真实数据库1项排除）、期限/取消3项、Python14项通过；工作区全部targets/features Clippy和最后runner关联Clippy均拒绝警告通过。七个实际CLI管理命令核验缺账本失败、migrate成功、目录别名/小预算拒绝、备份/恢复及旧root离线后的留存预览。第一次紧预算恢复测试发现SQLite辅助文件，改为独立DELETE模式备份后通过，保留失败日志。见[第一阶段实施](../archive/development/SYSTEM_OPTIMIZATION_STAGE1_20261025.md)和[支持资料](../testing/evidence/system-optimization-stage1-20261025)。

第二阶段：runner库12项、管理边界5项、实际CLI/control_plane21项（真实两库1项排除）、Python14项及全workspace Clippy通过。两日1,200行消费8→7请求、CSV SHA不变，但本次墙钟320.527→358.046ms，不宣称总吞吐改善。20,000条观测聚合与2,000个固定文件的重复计量分别减少固定工作；当前metrics CLI10次中位20.812ms、最大RSS11,665,408字节，仅为本机合成范围。schema3备份仍原样恢复后显式升级。见[第二阶段实施](../archive/development/SYSTEM_OPTIMIZATION_STAGE2_20261025.md)和[支持资料](../testing/evidence/system-optimization-stage2-20261025)。
