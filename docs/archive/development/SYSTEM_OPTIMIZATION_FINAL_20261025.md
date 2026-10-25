# A01–A12最终实施与验证

日期：2026-10-25。执行基线`47045cd554de2f3237a7f2f2821146b47807c43b`；当前为未提交工作区，基线HEAD不是新源码身份。Rust2024/MSRV1.89、同步I/O、有界线程保持不变。来源：[优化方案](../architecture/SYSTEM_OPTIMIZATION_PLAN_20261025.md)、[执行记录](../../development/SYSTEM_OPTIMIZATION_PROGRESS.md)。

## 1. 交付范围

| 项目 | 实际工程结果 | 剩余外部条件 |
| --- | --- | --- |
| A01–A04 | 确定性期限/同步取消、独立CI任务；实际父路径隔离与完整备份预算；查询只读/显式迁移；恢复逻辑路径与proof摘要核验 | 新提交各平台远端结果；更强身份隔离和目标恢复另验 |
| A05 | 五类事实维护、独立复核/升级角色、原来源摘要和复核期限巡检；未知模板拒绝 | 实际负责人、规则复核、2027官方日历、完整证券/点时事实 |
| A06 | 固定catalog只读；release会话复用、每日期全字段/末页及最终总数；消费审计独立于发布target_io | 新源码真实两库和长期审计配额/保留期 |
| A07–A09 | schema4事务汇总/可重建、两项查询索引；固定输入与动态计量分开、封存全树校验；离线应有键/初始并发/预算计划 | 生产长期规模、全市场吞吐和硬配额 |
| A10 | 工程scope1与历史证据分层；就近原字节夹具和独立来源；原件库存、获取校验/可选分发；有限文本可diff | 对象存储迁移、访问/退役承诺未实施；原Git历史及本地原件保留 |
| A11 | 最新同HEAD三平台/MSRV/质量/实际包/两库等门禁，确认前后读取远端；main/PR均触发两库；官方Action SHA/Node24与Mongo归档SHA固定 | 新提交远端结果、核心上传/索引可见/CLI注册表验证和真实发布 |
| A12 | 四种任务独立机器矩阵；同身份目标smoke/字段/键/资源/权限/故障/恢复/持续负载封套；Unknown拒绝 | 实际生产环境和批准SLO；分钟/快照/0547仍无runner任务/发布入口 |

前两阶段的管理修复与性能证据分别见[阶段一](SYSTEM_OPTIMIZATION_STAGE1_20261025.md)、[阶段二](SYSTEM_OPTIMIZATION_STAGE2_20261025.md)。本轮没有添加异步运行时、改写冻结历史、提交/推送、发布包或部署生产。

## 2. 工程归档与保全

工程scope1排除`docs/testing/evidence/`及旧`project-review-20261022/`，其他工程输入按Git清单与非忽略新文件绑定。当前完整检查输入25,283,831字节，确定顺序/时间的gzip归档7,129,771字节；审核身份为`1036ff0e6d633a1c74dc61e662bd12f7e794805d7ed1bcd1673b06607d053897`。归档及清单在[支持包](../../testing/evidence/system-optimization-final-20261025)，收尾新增文档后的身份另记录在[source-identity.json](../../testing/evidence/system-optimization-final-20261025/source-identity.json)。

必需报文/独立期望复制到所属crate，原始副本和摘要不变。98帧回放的原大型日线JSON仅生成已断言字段的紧凑新夹具；原独立期望保留。505份新夹具共8,487,099字节，来源/原SHA/新SHA见[夹具清单](../../../config/release/offline-fixtures.json)；发行人PDF也按原SHA复制。

历史库存24,920份、2,386,717,373字节，包含12份本地保留collector。每份流式SHA核对，已提交原件同时核对HEAD的Git blob及模式；完整verify通过。原件清单、核验结果和独立review分发包保存在支持包；新最终支持资料按自己的库存核验。它们证明字节身份，不认证来源真实性或扩大业务资格。

普通Git检出仍缺12份超限历史collector；取得原路径字节后按[原清单](../../testing/evidence/optimization-source-delivery-20261025/local-binaries.json)核验，不能声称普通检出可完整重放第六阶段。没有上传到对象存储或删除历史。

## 3. 实际验证

所有Cargo命令使用1.89.0及locked/offline，`RUSTDX_SKIP_INTEGRATION_TESTS=1`；外部用例提前返回/忽略不计在线通过。日志、命令、退出码、环境、二进制及包摘要见[summary.json](../../testing/evidence/system-optimization-final-20261025/summary.json)及原始日志。

| 检查 | 结果 |
| --- | --- |
| 全workspace all-targets/all-features测试 | 534通过、10忽略 |
| 全workspace all-features文档测试 | 57通过、18忽略，包含compile_fail |
| fmt / 全workspace Clippy `-D warnings` / 严格Rustdoc | 全部通过 |
| Python，包括事实、留存、发布/库存边界和任务准入 | 最终22项通过 |
| 实际CLI/control_plane，串行执行 | 21通过；唯一真实两库案例明确排除 |
| 独立工程归档，不含历史证据，独立Cargo target | 534项Rust通过；补齐配置后两处22项Python通过 |
| 核心/CLI实际crate，解包下游及新旧签名消费 | 通过；新签名运行、旧签名按声明编译失败；CLI使用解包核心patch |
| CLI/runner优化发布构建 | 通过；本机二进制摘要及平台已记录 |
| 当前文档导航/FR-01至FR-18 | 本地链接及锚点核验通过；最终结果见支持包 |
| 历史库存、读取核验、review分发归档 | 24,920份核验通过，分发前后输入身份一致 |
| 默认准入/未提交源码正式发布门禁 | 均退出1，保留拒绝报告；没有调用真实上传 |

工作区、独立归档及CLI组有重叠，不把这些数量相加成独立测试总数。包内源码/README/许可证再次与当前工程核对，核心100个、CLI27个必需文件一致；原成功preflight及消费者记录保留，新工程输入和文档变化另记录，`publication_qualified=false`。

核心crate SHA为`c67d191d6002e54b40645958e88576b3639a6c2a1d9ff0a4e7d1cd31e7b09979`，CLI为`ae72f9ecebc84324b7e2da0806fd9d4a38cb114d87f80b1fca215c31dc184605`。这是本地审核包，不是crates.io下载或发布成功。

## 4. 验证中发现并处理的边界

首次紧凑夹具遗漏大型JSON中同时存在的分页信息，98帧测试拒绝；补回所需页请求和已断言字段，新夹具摘要另记，原件未改。全量启动测试仍假设status自动建账本，改为在测试内显式migrate，再完整验证默认离线、显式预热、取消和期限。

隔离Rust534项通过后，Python发现原`acceptance/`忽略规则误排除新增验收矩阵/模板及Mongo工具SHA文件。规则收窄为根目录`/acceptance/`，三份工程配置进入清单，新增回归保护；Rust代码/夹具/锁字节不变，补齐后的根目录和隔离归档Python各22项通过。没有把原缺文件失败改成通过。

并行冷编译期间40证券CLI成功用例触发原50秒worker期限，该组20通过/1失败。编译结束后，同源码/CLI/期限的21项串行通过，42.98秒是整组耗时，不是单任务SLO。没有调宽生产期限、删断言或加自动重试；失败日志保留，也不据此声称任意资源负载下都能按时完成。

官方Action ref及action元信息核对30处使用，均为固定SHA；Node24元信息验证不等于新工作流已经在GitHub执行。Mongo归档在解压前核对批准SHA，镜像及这些配置摘要进入正式交付清单。

## 5. 性能与生产边界

A06同一两日1,200行CSV SHA不变，8→7请求、364,404→363,430证据字节，但单次墙钟320.527→358.046ms；不能宣称总体吞吐提高。A07的20,000条历史扫描51,510µs/汇总69µs，当前metrics CLI中位20.812ms；A08的2,000固定文件100次计量813,412µs→304µs。它们只证明所列固定工作减少，不替代采集进程树、真实数据库或全市场性能。

默认未来日历、规则复核及全市场事实仍不完整。真实盘中推送、通用分钟量/时间资格、目标权限/硬配额、持续负载和业务批准RPO/RTO没有因工具完成而通过。按[任务准入](../../testing/TASK_ADMISSION.md)、[维护流程](../../operations/DATA_MAINTENANCE.md)及[当前验收矩阵](../../testing/PRODUCTION_ACCEPTANCE.md)取得新实际证据；新结果另留证。
