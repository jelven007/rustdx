# 生产工程目录迁移记录

日期：2026-10-24。范围为工作区目录、生命周期文档、构建/测试/脚本引用和有依据精简；保留产品API、包/二进制名及同步有界设计。

## 1. 目录映射

| 原位置 | 当前位置 |
| --- | --- |
| 根Cargo.toml的核心package | crates/rustdx-core/Cargo.toml；根变为虚拟workspace |
| src、tests、examples、benches | crates/rustdx-core下同名目录 |
| rustdx-cmd | apps/rustdx-cli |
| rustdx-runner | apps/rustdx-runner |
| tests-integration | tests/integration |
| fuzz | tools/fuzz；独立workspace |
| docs/validation | docs/testing；原始evidence一并迁移 |
| docs/history及日期化专项报告 | docs/archive，按原工作领域归类 |
| 日期化26命令手册 | docs/reference/COMMAND_API.md及原snapshot |
| docs/operations/RUNBOOK.md | docs/runbooks/RUNBOOK.md |

配置、部署、工程脚本与运行数据约定保留根职责。完整文件级映射、旧大小及SHA-256见[migration.json](../testing/evidence/project-restructure-20261024/migration.json)。

## 2. 调用方升级

库源码依赖改为/path/to/rustdx/crates/rustdx-core；cargo install使用--path apps/rustdx-cli。已发布包依赖和Rust导入名保持兼容，当前新接口仍须固定所需源码提交。

从根运行示例/测试/基准时显式指定-p jelven-rustdx-complete；完整检查使用--workspace。fuzz从tools/fuzz运行。测试引用使用crate目录或CARGO_MANIFEST_DIR，外部脚本仍从自身位置定位根。

当前接口签名和调用规则见[完整参考](../reference/COMMAND_API.md)与[应用接入](../development/APPLICATION_INTEGRATION.md)。协议语义变化按[MIGRATION](MIGRATION.md)执行，不能将路径迁移当成数据单位变更。

## 3. 文档更新与删除依据

需求保持FR-01至FR-18，版本升为3.0。当前架构、技术方案、开发、验证、验收矩阵、运营、运维及发布使用稳定入口；集成六类规则和完整26命令参考继续保留。

已实施的九命令、财务精度和成交额修复计划删除，实施报告及证据保留。重复需求跳转页删除。只依赖本机/tmp并覆盖历史审计结果的一次性冻结生成脚本删除，原生成JSON及采样/核验工具保留。Finder元数据删除。

历史架构/实施/验收报告进入archive，保留日期和范围。原始JSON、报文、fixture、来源快照、日历、必要许可证和运行数据不改写；历史旧路径通过迁移清单解析。

## 4. 验证和适用限制

本轮重新执行Rust1.89离线工程检查、包清单/核心包验证、脚本/config语法、路径/当前链接、证据和夹具摘要核验。路径遗漏或检查失败修复后重新验证，最终命令/退出码及统计见[verification.json](../testing/evidence/project-restructure-20261024/verification.json)。

历史实盘、数据库子集、runner模拟事实和fuzz结果保持当时归属；本次没有重新执行实盘、数据库、远端CI或插桩fuzz。目标Linux、真实全市场、0feb日期、0547盘中推送、长期SLO及数据库服务端恢复仍按[验收矩阵](../testing/PRODUCTION_ACCEPTANCE.md)管理。
