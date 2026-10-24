# 运行数据约定

本目录为本地运行数据入口，仅此README进入版本控制。版本化参考日历已迁至[config/calendars](../config/calendars)，固定测试数据在[tests/fixtures](../crates/rustdx-core/tests/fixtures/README.md)，持久验收指标在[docs/validation/evidence](../docs/testing/evidence)。

生产任务应在持久存储上选择独立工作目录，按业务显式传入源、前因子、日历和报告路径。CLI不会自动在本目录创建完整数据层次，以下子目录由部署方按需要创建：

| 建议位置 | 保存内容 | 保留规则 |
| --- | --- | --- |
| sources/ | 原始ZIP、day、gbbq及来源元数据 | 摘要不能替代源备份；gbbq按实际采集版本管理 |
| reference/ | 已核实当前日历、证券历史、公告原始字节 | data-check后显式发布，保留版本/来源/完整范围 |
| outputs/ | 有效CSV/JSON及待核查数据库中间CSV | unknown/acknowledged失败产物核实前保留 |
| manifests/ | 最新运行清单、run_id归档、原始响应证据 | 与输出/证据一并归档，按部署保留期管理 |
| backups/ | 导出备份及恢复材料 | 数据库仍须使用自身一致性备份机制 |

默认输出、factor.csv、报告和数据库中间文件与当前工作目录及参数相关，不会自动遵循上述建议。清单原始响应证据包含绝对路径，归档搬迁后需重新定位；迁移不能只复制CSV。下载/解压缓存和同机许可锁默认在系统临时目录，清理前核实活跃任务。

运行目录不进入Git或crate源码包。部署数据路径、备份目标、归档期限和磁盘余量由实际业务环境确定，见[运营说明](../docs/operations/OPERATIONS.md)、[导入状态](../docs/operations/IMPORTS.md)和[运维手册](../docs/runbooks/RUNBOOK.md)。
