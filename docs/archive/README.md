# 历史报告索引

本目录保存历次研究、实施与验收结果，日期和样本范围按原报告解释。当前需求、架构及生产状态从[当前文档](../README.md)进入；历史结论不随目录移动升级为当前全量或生产通过。

17项工程迭代收尾见[最终检查报告](development/OPTIMIZATION_FINAL_20261025.md)，另有[批量预算与目录查找](development/OPTIMIZATION_BATCH_METADATA_20261025.md)、[在线调度与留存预览](development/OPTIMIZATION_ACQUISITION_RETENTION_20261025.md)；真实范围与目标生产边界分别保留。

版本流程实施及本机/隔离验证见[版本规则实施记录](development/VERSION_POLICY_IMPLEMENTATION_20261025.md)；
当前有效规则见[版本管理](../release/VERSION_POLICY.md)。

最新专项：[通用资源与推送门禁修复](development/RESOURCE_CONTRACT_PUSH_ACCEPTANCE_FIX_20261024.md)、[板块完整MD5与严格解析修复](development/BLOCK_FILE_INTEGRITY_FIX_20261024.md)、[TCP日线整数股数修复](development/TCP_DAILY_INTEGER_SHARES_FIX_20261024.md)、[0feb日期编码修复与两节点日期对照](development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。旧差异和错位的原始记录保持不变，修复后范围另行取证。

## 按领域检索

| 目录 | 保存内容 | 常用报告 |
| --- | --- | --- |
| [architecture](architecture) | 协议研究、版本对比、架构评估 | [历次架构结果](architecture/REVIEWS_THROUGH_20261024.md)、[技术评审](architecture/TECHNICAL_ARCHITECTURE_REVIEW_20261024.md) |
| [development](development) | 命令、财务/价格、成交语义与分页实施 | [剩余优化第一阶段](development/REMAINING_FIXES_20261024.md)、[第二阶段](development/REMAINING_FIXES_STAGE2_20261024.md)、[第三阶段](development/REMAINING_FIXES_STAGE3_20261024.md)、[第四阶段/TCP闭环](development/REMAINING_FIXES_STAGE4_20261024.md)、[第五阶段/RowBinary与固定Linux](development/REMAINING_ACCEPTANCE_STAGE5_20261024.md)、[第六阶段/原生备份恢复](development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)、[runner组件](development/PRODUCTION_IMPLEMENTATION.md)、[财务与精度](development/FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)、[成交额](development/TRADE_AMOUNT_SEMANTICS_IMPLEMENTATION_20261024.md) |
| [validation](validation) | 分阶段协议、业务与工程验收 | [26命令](validation/ELTDX_3_3_0_26_COMMANDS_IMPLEMENTATION_20261024.md)、[历史工程检查](validation/VALIDATION_THROUGH_20261024.md)、[历史生产验收](validation/PRODUCTION_ACCEPTANCE_THROUGH_20261024.md) |
| [operations](operations) | 上游SDK接口、旧版本命令解释及调用分析 | [eltdx3.2.2手册](operations/ELTDX_3_2_2_COMMAND_GUIDE_20261024.md)、[上游33方法对照](operations/TDX_API_REFERENCE_20261024.md) |
| [history](history) | 各轮优化和交叉验证 | [第三轮](history/OPTIMIZATION_ROUND3.md)、[六步优化](history/NEXT_OPTIMIZATION.md) |
| [release](release) | 已执行源码交付与标签记录 | [2026-10-22交付](release/DELIVERY_20261022.md) |

## 留存与精简

已实施的重复修复计划已删除，保留实施报告与原始支持包。历史正文不改写当时的测试统计、采样路径或未验收判断；导航链接可以按新位置修复。

原始报文、JSON、参考源码和日志在[testing/evidence](../testing/evidence)，字节按原摘要保留。路径搬迁、删除原因及原文件摘要见[迁移清单](../testing/evidence/project-restructure-20261024/migration.json)，核验规则见[证据管理](../testing/EVIDENCE.md)。

报告中代码块的旧运行命令用于复现当时环境；当前命令入口见[开发说明](../development/DEVELOPMENT.md)。保留的第三方源码快照遵循其原许可证，仅作为协议与独立核验来源。
