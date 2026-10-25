# 生产工程目录升级

<!-- lifecycle:registry -->
读者：开发/部署；责任：发布维护者。源码核验：2026-10-25；每90日及路径或兼容入口变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

源码核验：2026-10-25。读者为开发、部署与发布维护者；责任为发布维护者。权威输入是Git移动记录、Cargo路径及[逐文件映射](../testing/evidence/production-lifecycle-20261025/migration.json)。每次路径变更及每90日复核，失效路径或缺少配套检查即过期。

| 原位置 | 新位置 | 使用方动作 |
| --- | --- | --- |
| crates/rustdx-core/examples/*.rs | examples/{usage,capture,validation,diagnostics}/*.rs | cargo --example名称保持原值；直接脚本路径改为新分组 |
| deploy/clickhouse | deploy/production/clickhouse | Compose及本地env参数更新 |
| deploy/runner | deploy/production/runner | systemd、DBA脚本和告警模板路径更新 |
| config/runtime.env.example | config/runtime/env.example | 私有config/runtime.env仍可保留；shell显式导出 |
| config/runner/publisher.env.example | config/runtime/publisher.env.example | 服务端/etc/rustdx/publisher.env位置保持不变 |
| 已完成优化/交付/发布报告 | docs/archive/development、release | 从归档索引追溯，历史正文不改写 |
| 根审查报告与project-review-20261022 | docs/archive/development与docs/testing/evidence | 使用映射解析旧路径，原始SHA不变 |

根虚拟workspace、库公开路径、产品包和二进制名称不变。源码依赖使用crates/rustdx-core；CLI安装使用apps/rustdx-cli。不得用本次迁移更新运行中的数据库、账本、数据目录或事实版本。

升级时在独立分支/工作目录操作，保存现有私有env，更新外部脚本路径并执行Cargo全目标、Python、文档与包消费门禁。部署路径改变需目标机核实账号、配置和定时器；本机目录检查不证明systemd上线。

历史路径先检查现有文件，再使用两次迁移映射定位；外部绝对路径保持原值。证据核验与bundle仍验证原摘要，打包保持原清单中的成员名。映射不是改变来源或重新授予资格的工具。

2026-10-24根目录分层见[旧迁移报告](../archive/release/PROJECT_RESTRUCTURE_20261024.md)及[原映射](../testing/evidence/project-restructure-20261024/migration.json)。代码回退检出对应提交并按当时入口构建；数据恢复和固定目录消费执行独立运维流程。
