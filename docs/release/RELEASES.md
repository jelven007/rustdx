# 源码与软件包交付

更新日期：2026-10-25。当前项目版本为 <!-- version:project -->2.0.0<!-- /version -->（未发布），runner 为 <!-- version:runner -->0.1.0<!-- /version -->；已发布1.x是历史内容。加号流程见[版本规则](VERSION_POLICY.md)。本地包验证不构成注册表发布。

## 1. 交付单位与身份

| 单位 | 必须记录 |
| --- | --- |
| 源码 | Git提交、工作区状态、Cargo.lock、实际检查及文档版本 |
| 二进制 | 同一提交构建的rustdx/runner、平台、Rust版本、SHA-256 |
| 核心/CLI包 | 新版本、CLI依赖版本、许可证、实际包清单与验证 |
| 部署 | 配置修订、数据库/工具版本、事实版本、权限/资源及目标验收 |
| 数据版本 | dataset、契约、catalog_version、release、source_revision/source_as_of |

源码版本、包版本和数据目录版本分别管理。同版本号不能证明同一源码；history标签不能限制运行归档或备份数量。

## 2. 工程门禁

执行[开发检查](../development/DEVELOPMENT.md)及[验证方法](../testing/VALIDATION.md)：fmt、all-targets/all-features Clippy、Rust1.89 workspace/文档测试，变更所需的独立样本与故障回归。

核对当前文档链接、配置/脚本路径、需求映射及许可证，检查两个发布包清单和解包编译。发布包排除工程样本与运行产物，不能排除src/data等实际源码。离线环境若缺注册表索引，明确区分本地源码编译与上传包验证。

`python3 scripts/release/preflight.py --output /outside/repository/new-directory`
默认要求干净源码，执行Rust1.89格式、全目标/特性Clippy、测试/文档测试，
生成两个`.crate`、逐个核对src与许可证，解包核心并编译最小下游和CLI。
CLI验证使用解包核心的本地patch；正式CLI注册表验证在核心索引可见后执行。
离线CLI打包也在隔离工作区使用该patch并生成审核锁文件；源Cargo.lock不改动。
源码文件清单、HEAD/dirty/lock摘要、固定顺序/时间的源码归档与包/检查摘要
写入schema2的`preflight.json`。工程scope1排除`docs/testing/evidence/`和
`project-review-20261022/`，其他跟踪及非忽略新文件均计入；历史证据另行核验。
离线回归所需小样本在所属crate，来源/原摘要与派生范围见
[夹具清单](../../config/release/offline-fixtures.json)。工程归档不承诺完整历史演练。

审核未提交工作区可加`--snapshot --offline`，无需提交；
产物明确`publication_qualified=false`。完整干净本机检查仅设置`local_gate_passed`；
正式资格由下述同SHA远端门禁授予。`--skip-workspace-checks`只做包审核，
同样不满足正式门禁。工具不上传包、不生成提交；源码归档可重建内容身份，
不宣称二进制位级可复现或注册表同版本已更新。生成器要求输出在仓库外，
开始/结束及实际发布确认后重新核对源码，变动则拒绝继续。

`python3 scripts/release/check_consumers.py --output /outside/new-directory`验证
独立当前调用方可编译运行，以及旧日历/财务/心跳返回类型调用方按声明失败。
该检查不是完整API diff，也不把预期失败解释为兼容。工具链/包版本/源码与
锁摘要及每个编译结果写入报告，可指定`--cargo`、`--toolchain`和`--offline`。
CI的contracts任务同时执行源码调用方和上述实际解包消费；MSRV、行为、
格式/Clippy及性能任务分别保留，各产物名称绑定实际GitHub SHA。
Actions须推送后实际核对结果，本机验证不能代替远端执行。

完整本机预检还执行Python、严格Rustdoc及CLI/runner发布构建，记录二进制摘要。
可用`--build-cache /external/cargo-target`复用编译缓存；审核crate和发布二进制复制到
新output，实际源码/包核对照常执行，缓存路径写入报告，不授予二进制位级可复现结论。
`release_gate.py --preflight /review/preflight.json --output /review/release-manifest.json`
通过`gh api`读取[必需门禁](../../config/release/gates.json)，要求当前干净HEAD的
对应 Main version 推送运行、三平台、MSRV、质量、消费者/包、fuzz、性能、数据库和标签任务全部成功。
用当前提交引入的加号记录定位原始 push SHA 和 run_id，检查最新运行尝试；
重跑失败/取消/遗漏任一平台均拒绝。再下载当前检查提交 SHA 的
contracts制品，核对preflight身份、实际两crate字节摘要及清单。
离线`--snapshot-runs`只审核记录，始终不授予发布资格。

Action官方ref和运行时元信息固定在[工具清单](../../config/release/action-pins.json)，
Node24及最低runner要求需在目标Actions环境满足；新SHA远端执行尚待取得。
Mongo工具解压前按`deploy/acceptance/tool-archives.sha256`检查，数据库镜像digest
及上述配置摘要进入release manifest。manifest记录本地patch范围和注册表待验证状态。
数据库检查由统一 CI 调用，覆盖所有 main 推送/PR，包括仅修改文档的提交。

## 3. 源码交付与历史引用

提交应只包含本次已核实工作，不回滚已有未提交修改。每项任务使用独立分支和工作目录，经 PR 合入 main；本地开发、提交、合并和功能分支推送不加号。远端 CI 先同步版本后检查，成功后打标签；结果须实际核实，不由本机结果推导。

已有history/source-20261022和history/release-1.12.2-1.8.2记录保留在[历史交付报告](../archive/release/DELIVERY_20261022.md)。它们属于当时目录结构；检出旧提交后按对应文档构建。标签解引用后的提交才是源码身份。

源码回退不会回滚数据库、文件或清单。数据消费回退固定旧catalog_version；数据库灾难恢复执行独立恢复流程。

## 4. 正式软件包发布

1. 使用主线 CI 已同步并通过检查、打标签的版本，更新CHANGELOG和[行为迁移](MIGRATION.md)状态也须经 PR，由 CI 递增。
2. 完成工程、同SHA远端门禁、实际包清单和核心注册表dry-run，确认源码与最终上传包一致。
3. 按scripts/release/publish.sh先核心后CLI，等待核心索引可见。
4. 核实注册表版本、CLI依赖、包内源码/许可证及校验和，记录发布结果。

发布脚本在确认前完成本机及远端门禁，确认后重新核对源码/审核包，并重查最新远端结果。
核心上传并等待索引可见后，CLI不使用本地patch执行注册表dry-run，再上传。
实际发布仍须另有任务授权与凭据，runner不发布到crates.io；本轮没有运行实际publish。

## 5. 本次迁移交付

[目录迁移记录](PROJECT_RESTRUCTURE.md)定义应用path依赖、install、测试/示例/fuzz新入口。逐文件迁移/删除原因和最终检查保存于docs/testing/evidence/project-restructure-20261024。
