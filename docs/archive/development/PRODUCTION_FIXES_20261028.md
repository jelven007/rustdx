# 七项生产改进实施记录

日期：2026-10-28。基线为d6bf22a，源码版本2.0.3；此功能分支不递增版本。
本记录描述源码与本机工程核验，市场事实和目标生产验收继续以
[验收矩阵](../../testing/PRODUCTION_ACCEPTANCE.md)及[任务准入](../../testing/TASK_ADMISSION.md)为准。

| 问题 | 实施与可验证行为 | 目标现场边界 |
| --- | --- | --- |
| 多release消费者耗尽Mongo连接许可 | 日期顺序只有一个活动reader；切换时完整总数/退出核验后释放许可，A/B/A重新打开A；额外总数或错query_id保留旧CSV | 本地管道模拟9个release、许可1/8及双消费者共享1，不代替真实Mongo负载 |
| 规则截止阻断原价日线状态维护 | 新state-only载入保留结构/阶段/公告检查，默认TryFrom仍严格；context_at不授予截止外规则；任务复核绑定实际四类输入SHA与日期 | 统一规则截止仍2026-10-22；未提供的真实公告/全市场名单不补造 |
| 固定旧Job无法按交易日补积压 | Python同步调度按独立日历/滞后/批准截止保留全队列，按批次生成完整日期；来源有效期/获取日/明确修订，执行前后重验 | 来源就绪与责任复核须由实际维护者提供；TCP获取日须为当天 |
| 健康汇总掩盖业务逾期或缺失实例 | 独立5分钟monitor重算全积压，0644原子textfile；Prometheus侧维护实例清单逐项判missing，daily_overdue_dates判逾期 | PromQL执行、Linux服务启动、Alertmanager送达与解除须现场验证 |
| 准入证据与真实执行输入未绑定，消费审计未一致备份 | 每Job核实际SHA/两二进制/锁/三事实/矩阵/平台/版本/完整范围和键；缺准入禁止recover/run且退出1；消费者共享审计锁、备份排他锁，成功/失败审计纳入库存与恢复 | 真实权限、ENOSPC、静止跨库恢复、账号角色及批准RPO/RTO须目标证据 |
| 超行数计划无法给出完整日期窗口 | plan完整枚举再判max_rows，与数据库预算共同只拆日期；单日超限仍拒绝，运行时行数限制不放宽 | 保存期/总硬配额/远端不可变存储继续要求批准，历史可达版本不自动删除 |
| 日线请求可能携带不支持字段 | required_fields非空唯一且仅允许规范日线CSV字段；分钟/快照/订阅无runner入口仍拒绝 | 历史样本不批准未来日期或其他任务 |

实现入口为runner的consumer/storage/ops/pipeline、CLI data-check/data-status、
`scripts/ops/daily.py`与`production_gate.py`。当前合同同步到APPLICATION_INTEGRATION、
DATA_MAINTENANCE、LIMIT_RULES、OBSERVABILITY、EVIDENCE_RETENTION和TASK_ADMISSION；
FR-02/14/15/16/18及文档核验日同步登记。

## 验证范围

当前恢复工作目录独立于主工作区；主工作区已有的两处无关修改未纳入本分支。
此前临时工作树的590/49通过统计属于历史过程，不能作为恢复后源码证据。
首次恢复后全工作区运行发现新增双消费者测试夹具未穿过子进程env_clear传递共享计数目录；
改为夹具JSON显式传递后，单项回归已通过，生产凭据隔离保持原行为。

最终参数检查复现了调度器向plan/run传`--no-tcp-startup`导致TCP来源被拒绝；
去掉该覆盖，保留配置自身的来源网络策略，管理recover仍显式禁用预热。
真实runner的TCP只读plan先失败、修复后通过，调度run参数回归保留来源网络；
8项日调度测试通过，未调用真实TCP采集或写数据库。

恢复后最终检查均退出0：

- `cargo fmt --all -- --check`及`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`。
- `RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --all-features --locked`：71个结果组，591通过、0失败、47按原标记ignored。
- 设置实际`RUSTDX_TEST_CLI`，`cargo test -p rustdx-runner --all-features --locked --test control_plane -- --include-ignored --skip real_databases_isolate_versions_reject_extra_keys_and_restore_local_evidence`：24通过、0失败，仅过滤真实两库1项。
- 设置实际`RUSTDX_TEST_RUNNER/RUSTDX_TEST_CLI`，unittest全发现：58通过、0跳过；包括真实local/TCP只读计划、调度run保留来源网络、CLI默认规则拒绝/state-only通过/公告篡改保旧输出。最终全发现只为已有Git辅助函数增加失败stderr输出，异常仍原样抛出，未跳过或修改测试。
- 文档导航53份/1173链接、生命周期36份/源码锚点1项、`versioning.py check --strict`通过，版本2.0.3；FR-01至FR-18保留。
- CI/告警/通知YAML及6份systemd INI、runtime JSON解析、`git diff --check`通过。

日志保存在交付工作区的`reports/production-fixes-20261028`，未把本机日志打包成真实目标证据。
首次失败日志另留`fresh-workspace-first-failure.log`，最终结果另存新日志，不改写冻结市场证据。
TCP修正后的全发现曾在既有版本测试的临时裸Git克隆处退出128；独立诊断和随后58项全发现均通过，
未确认该环境失败的稳定原因，失败日志`final-python-with-tcp-tests.log`与最终
`final-python-diagnostic-tests.log`分别保留；版本测试源码未改动。
真实两库、Linux/systemd、PromQL运行时、通知及持续负载未在本机执行，不能声明目标生产通过。
