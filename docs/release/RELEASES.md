# 源码与软件包交付

<!-- lifecycle:registry -->
读者：开发/发布；责任：发布维护者。源码核验：2026-10-27；每30日及CI、包发布或权限变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

当前源码版本：<!-- version:project -->2.0.4<!-- /version -->。数字由主线 CI 同步；注册表历史发布事实另列。

更新日期：2026-10-27。历史核心/CLI 2.0.0已发布到crates.io，runner仅交付源码与二进制。该次注册表发布来自提交`82c6de4e52dae907cb9055d4cd07bbb9444e0c47`；实际包摘要、核验结果和安装命令见[发布记录](../archive/release/PUBLICATION_20261025.md)。当前源码版本不证明同版本已经上传。

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
根目录`project-review-20261022/`，其他跟踪及非忽略新文件均计入；历史证据另行核验。
离线回归所需小样本在所属crate，来源/原摘要与派生范围见
[夹具清单](../../config/release/offline-fixtures.json)。工程归档不承诺完整历史演练。

审核未提交工作区可加`--snapshot --offline`，无需提交；
产物明确`publication_qualified=false`。完整干净本机检查仅设置`local_gate_passed`；
正式资格由下述同SHA远端门禁授予。`--skip-workspace-checks`只做包审核，
同样不满足正式门禁。工具不上传包、不生成提交；源码归档可重建内容身份，
不宣称二进制位级可复现或注册表同版本已更新。生成器要求输出在仓库外，
开始/结束及实际发布确认后重新核对源码，变动则拒绝继续。

`python3 scripts/release/check_consumers.py --output /outside/new-directory`验证
独立2.0调用方可编译运行，以及旧日历/财务/心跳返回类型调用方按声明失败。
实际包消费另核对两类IndicatorError别名及真实帧checked→owned JSON的市场、周期、单位和请求元数据。
该检查不是完整API diff，也不把预期失败解释为兼容。工具链/包版本/源码与
锁摘要及每个编译结果写入报告，可指定`--cargo`、`--toolchain`和`--offline`。
CI的contracts任务同时执行源码调用方和上述实际解包消费；MSRV、行为、
格式/Clippy及性能任务分别保留，各产物名称绑定实际GitHub SHA。
Actions须推送后实际核对结果，本机验证不能代替远端执行。

完整本机预检还执行Python、严格Rustdoc及CLI/runner发布构建，记录二进制摘要。
可用`--build-cache /external/cargo-target`复用编译缓存；审核crate和发布二进制复制到
新output，实际源码/包核对照常执行，缓存路径写入报告，不授予二进制位级可复现结论。
包选择使用只读Cargo metadata的准确包名/版本；旧版本缓存同存不构成错误，
缺少当前版本或当前路径为链接仍拒绝，不能用旧包代替当前请求。
`release_gate.py --preflight /review/preflight.json --output /review/release-manifest.json`
通过`gh api`读取[必需门禁](../../config/release/gates.json)，要求当前干净HEAD的
版本事件检查/既有push/手工运行、三平台、MSRV、质量、消费者/包、fuzz、性能和数据库任务全部成功。
重跑失败/取消/遗漏任一平台均拒绝；PR合并SHA不能代替当前提交。再下载当前SHA的
contracts制品，核对preflight身份、实际两crate字节摘要及清单。
离线`--snapshot-runs`只审核记录，始终不授予发布资格。

Action官方ref和运行时元信息固定在[工具清单](../../config/release/action-pins.json)，
Node24及最低runner要求需在目标Actions环境满足；2026-10-25历史2.0.0发布提交的9项远端门禁已通过，后续新提交仍需独立核验。
Mongo工具解压前按`deploy/acceptance/tool-archives.sha256`检查，数据库镜像digest
及上述配置摘要进入release manifest。manifest记录本地patch范围和注册表待验证状态。
数据库检查由PR或main版本工作流调用，覆盖每次推送，避免发布提交因仅修改文档而没有同SHA结果。

## 3. 源码交付与历史引用

任务使用独立分支/工作目录，经PR合入main。功能分支与本地提交不改号，主线push由CI先同步版本，再对同一提交执行全部质量检查，最后打v标签。规则、重跑与权限边界见[版本管理](VERSION_POLICY.md)。

质量成功后新增history/source-<版本>并按最新main提交祖先顺序保留三个history/*引用；原子推送带旧OID租约，竞态后回读收敛，旧事件重跑不复活已清理历史。现有v标签、数据版本、账本和备份不受此保留数影响。源码回退固定已检查提交/二进制，数据消费固定catalog_version，数据库恢复另行执行。

已有两个历史标签的原始交付见[旧交付报告](../archive/release/DELIVERY_20261022.md)。是否仍属于最近三个由主线成功CI判断，不修改旧报告的当时结论。

## 4. 正式软件包发布

1. 使用主线CI已经同步且检查通过的版本、核心依赖与锁；本地不得手工加号。更新CHANGELOG与行为迁移后经PR及新主线门禁交付。
2. 完成工程、同SHA远端门禁、实际包清单和核心注册表dry-run，确认源码与最终上传包一致。
3. 按scripts/release/publish.sh先核心后CLI，等待核心索引可见。
4. 核实注册表版本、CLI依赖、包内源码/许可证及校验和，记录发布结果。

发布脚本在确认前完成本机及远端门禁，确认后重新核对源码/审核包，并重查最新远端结果。
核心上传并等待索引可见后，CLI不使用本地patch执行注册表dry-run，再上传。
实际发布须有任务授权与凭据，runner不发布到crates.io。2026-10-25历史2.0.0发布已完成核心上传、
索引可见、CLI无本地patch的注册表dry-run和上传，并核验两个注册表下载包。
发布前门禁记录与发布后结果分别保存，详见[发布记录](../archive/release/PUBLICATION_20261025.md)。

## 5. 本次迁移交付

[当前目录迁移](STRUCTURE_MIGRATION.md)定义应用path依赖、install、测试/示例/fuzz新入口。逐文件迁移/删除原因和最终检查保存于docs/testing/evidence/project-restructure-20261024。
