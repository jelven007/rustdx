# 证据分发、运行留存与取回

<!-- lifecycle:registry -->
读者：运维/数据及发布维护者；责任：证据维护者。源码核验2026-10-28，每30日及留存、配额或取回入口变化复核。现场存储及恢复另验，见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)。
<!-- /lifecycle -->

冻结证据保存原始字节、来源、摘要及历史结论。`evidence_manifest.py inventory/verify/bundle/restore`
分别登记库存、验证工作树、生成固定归档和向全新目录取回。归档摘要及每文件摘要同时核对，
缺失、篡改、额外/重复条目、链接、损坏压缩或已有目标均拒绝；不合并覆盖旧证据。
离线本地归档取回不依赖网络，远端归档缺失时必须失败，不能重新计算摘要冒充原件。

`--include-local-binaries`另外核对已登记的12份历史collector。Git不含这些二进制，
需要完整Linux重放的任务必须显式取得原字节并核对登记SHA；普通源码包不可宣称已携带全部
本地二进制。新证据用新目录登记；路径迁移映射和新结果单独记录，旧manifest保持原字节。
详情见[证据边界](../testing/EVIDENCE.md)。

| 留存类 | 保留依据与边界 | 独立容量/备份输入 |
| --- | --- | --- |
| release | 所有登记catalog可达版本、原输入、固定manifest和历史摘要均受保护 | release硬配额与最短保存期；不得因年龄解除引用保护 |
| consumer_audit | 成功与失败数据库消费者的原请求/响应，独立目录纳入静止控制层backup | 独立硬配额、最短保存期及备份周期；共享审计锁阻止与排他备份并发，库存/预算/隔离恢复验证覆盖原字节 |
| database_evidence | 发布/恢复证明及全部数据库I/O，按release/target保留 | 独立硬配额及保存期；不能与consumer-audit预算相抵 |
| backup | 本地一致控制层备份与数据库原生备份分别登记 | 硬配额、最短保存期、备份周期与目标RPO/RTO；取回至新目录核对固定版本 |

[留存输入模板](../../config/maintenance/retention.example.json)故意保留null：
实际owner、独立reviewer、每类批准值及不可变存储位置尚未提供。
`audit_retention_policy.py`对缺输入返回attention_required/退出1；inputs_complete只表示输入齐全，
不证明硬配额实施、远端对象可取或生产恢复达标。此轮不擅定生产保存期限。

`retention_preview.py`在共享控制锁下只读核对全部历史目录和显式备份，记录逻辑/分配字节、
依赖及增长趋势。全部登记版本/证据受保护，无引用且达到预览年龄的对象只标记candidate_review，
没有自动删除、账本裁剪或远端清理入口。跨版本数据库恢复按[部署恢复](../../deploy/production/runner/README.md)
与[本地演练入口](../../scripts/validation/verify_runner_local.py)另留范围证据。

远端不可变存储目前未配置；Git历史和原证据继续保留。普通开发可按需使用sparse checkout，
需要完整证据的CI必须明确取得所需库存并运行verify，不能把缺原件的测试记为通过。
未迁走历史Git对象前，不承诺冷checkout或压缩包缩减；字节核验也不授予市场语义资格。
