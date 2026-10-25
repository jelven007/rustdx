# 生产工程与生命周期文档整理

任务日期：2026-10-25。基线为696e80040abc935a32ab2c12bacc86c40bfa6a36，独立分支codex/production-lifecycle-20261025；合入此前版本政策分支，保留最新Windows/HTTP修复及2.0.0实际注册表发布事实。

| 用户任务 | 实施结果 | 核验入口 |
| --- | --- | --- |
| 生产系统目录 | 45示例用途分组并保持Cargo名称；生产/验收部署及runtime模板分开，完成阶段报告归档 | PROJECT_STRUCTURE、ADR-0001、STRUCTURE_MIGRATION |
| 全生命周期文档 | 需求/架构/开发/测试/运营/运维/发布入口更新，FR-01至18关系和34份文档责任/周期登记 | TRACEABILITY、DOCUMENTATION_GOVERNANCE及CI检查 |
| 推送合并与版本 | 功能分支版本不变，main每push一次由CI同步并全检查后打v标签 | VERSION_POLICY、main-version.yml、versioning.py |
| 最近三个历史版本 | 只维护history/*源码引用，按main祖先顺序、原子租约及回读处理竞态/旧重跑 | 私有Git故障回归；实际远端结果另核 |
| 证据保全 | 移动报告/原始证据保持字节，旧清单通过映射核验/打包且保留成员名 | migration.json及evidence_manifest回归 |

本机Rust1.89全workspace格式、Clippy、全目标构建、测试及严格Rustdoc通过；测试585通过、28忽略、0失败。Python35项通过；Unix runner另执行21项本地故障/恢复用例，全部通过，真实数据库用例单独排除。文档链接/FR、34项生命周期、版本及Cargo示例元数据检查通过。真实行情与外部测试跳过不计在线验收。

独立fuzz锁初检发现缺少基线核心Windows依赖；补齐windows-sys0.61.2，保留既有第三方版本后locked metadata通过。原失败输出与最终复验分别保存。此次本机没有执行插桩fuzz，远端由CI执行。

实际核心/CLI包、安全解包、独立新/旧签名调用方检查通过；snapshot仅用于本次审核，不授予注册表上传资格，CLI解包检查使用本地核心patch。二进制逐字节可复现未宣称。

原始本机命令、退出码与摘要在[新证据目录](../../testing/evidence/production-lifecycle-20261025)；迁移72项中的冻结报告和审查材料均保持SHA。全量证据inventory核验25045项，清单放外部/tmp避免递归纳入。

远端PR及main版本事件须实际确认；不以本机或旧主线成功代替新提交检查。既有固定Linux静止两库恢复保留限定结论，目标账户/systemd/硬配额、真实全市场、未来事实、0547主动推送、长期SLO及生产RPO/RTO仍按当前矩阵验收。私有仓库服务器PR保护API的Pro权限403边界保持原记录。
