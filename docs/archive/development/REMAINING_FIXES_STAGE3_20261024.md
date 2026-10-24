# 剩余优化第三阶段

2026-10-24，延续[20项评审](../architecture/CURRENT_PROJECT_REVIEW_20261022.md)、
[第一阶段](REMAINING_FIXES_20261024.md)与[第二阶段](REMAINING_FIXES_STAGE2_20261024.md)。
本阶段保持版本、同步I/O和现有未提交迁移；没有提交、推送或上传注册表。

| 事项 | 实现 | 仍有边界 |
| --- | --- | --- |
| O05 / P0 | preflight生成固定内容清单/源码归档、HEAD/dirty/lock摘要；两个包源码核对、解包下游运行和CLI编译；publish要求干净树/MSRV检查，确认后重核源码 | 当前仍是未提交审核快照；版本升级、正式提交、注册表验证和发布未执行 |
| O14 / P1 | 所有独立池/手动诊断共享单独进程探测配额；共享业务策略衍生独立OS锁槽；可取消限时等待，资源失败不替换当日池；按端点/opcode保存最新传输/解码观察与过期筛选 | 被动观察不等于全面能力采样或业务新鲜度；未在多进程真实主站同时启动验收 |
| O08 / P1（部分） | worker存活/取消仍100ms轮询，目录软计量改为1秒，退出时完整核对；此前数据库热路径已增量计量 | 监督仍遍历目录，未实现文件事件增量追踪；硬限额仍依赖目标文件系统 |
| O16 / P2（部分） | runner取消/超时分开，Stage保留分类，CLI执行错误JSON含error_category；账本detail保留分类；组长回收后最多等待200ms核对后代消失 | 核心解码/业务资格与各层墙钟对账未统一为完整阶段指标 |
| O17 / P2 | EmaState/MacdState常量内存更新；checked参数，缺口重置、检查点重放、一倍/两倍柱状图显式口径；批量复用同一递推 | RSI/KDJ等没有新增增量状态；时间/复权/closed-bar资格仍由消费方维护 |
| O18 / P2（部分） | 17条现代解码fuzz路径及CI入口；五样本中位数JSON基准，覆盖1万/10万/100万行及不同窗口；同环境比较器可按阈值退出失败 | 未做订阅网络状态序列fuzz、长时间插桩、全市场/数据库吞吐及新旧RSS比较 |

`ProbeStage::Quota`在本批资源不足时终止刷新，避免把排队失败当作节点故障
发布空池。`capable_nodes`要求当前有效池、最新正文解码通过、未过期；普通
心跳或单纯身份通过不足以放行特殊命令。0547解码成功不能证明主动推送，
空正文也不证明证券/日期覆盖。共同合同见[应用接入](../../development/APPLICATION_INTEGRATION.md)。

preflight对当前工作区生成7740个源码/证据文件清单与固定顺序/时间源码归档。
实际核心包101文件、CLI包27文件，均核对源码与各自许可证。CLI离线打包在
隔离工作区重新生成锁文件并patch解包核心；不改仓库锁文件，不声称完成
注册表验证。审核使用`--snapshot --offline --skip-workspace-checks`，
完整工程检查另行执行并留证，`publication_qualified=false`。
本地交付目录为`reports/source-review-stage3-20261024`（忽略运行产物），包含
源码归档、原始预检JSON和两个crate；机器证据另留预检清单与全部摘要。
源码归档对应本阶段核验时点，后续文档/源码变更需要新的身份快照。

## 验证

机器证据：[remaining-fixes-stage3-20261024.json](../../testing/evidence/remaining-fixes-stage3-20261024.json)；
完整日志、包清单和基准在[支持目录](../../testing/evidence/remaining-fixes-stage3-20261024)。

| 检查 | 实际结果 |
| --- | --- |
| Rust1.89 workspace all-targets/all-features Clippy，-D warnings | 退出0 |
| workspace all-targets/all-features测试 | 474通过、0失败、7 ignored |
| workspace文档测试 | 66通过、0失败、18 ignored |
| fmt、严格rustdoc、独立fuzz工程检查 | 退出0 |
| 现代17路径 ASan fuzz | 302867次，31秒，退出0 |
| 原checked解析器 ASan fuzz | 279639次，31秒，退出0 |
| 两包生成、解包下游运行、解包CLI编译 | 退出0；CLI本地patch，未访问注册表 |
| 指标JSON基准 | 24个测量项，五样本中位数，退出0 |
| 归档确定性/中途来源变更拒绝、比较器通过/超阈值退出 | 通过 |

100万行MACD增量观察约4.56ms，仅是本机合成输入计时；批量测量还包含独立EMA，
不能据此计算新旧加速比或RSS收益。fuzz使用冻结实帧副本及合成空输入作种子，
原始夹具、manifest和历史摘要未改写。nightly因MSRV保留的fetch_update给出
弃用警告，Rust1.89 Clippy无警告；不为nightly推荐替换而提高MSRV。

回归曾发现并已处理：旧deadline断言需改为timeout；后代进程表消失存在短时
时序差；runner错误显示应从Debug改为结构化JSON。最终全部回归通过。框架
通过数包含提前跳过外部服务后返回的用例；ignored未运行。没有新增实盘、
真实数据库、Linux或远端CI结论。

最终补充将worker时限回归设为3秒，扣1秒预留后仍有整秒CLI预算，
确认实际spawn/终止而非在预留检查提前返回；control_plane再次9通过/6 ignored，
全Clippy与fmt再次通过。活跃文档620个相对链接0缺失，git diff --check退出0。
详见[补充证据](../../testing/evidence/remaining-fixes-stage3-20261024/final-checks.json)。

## 继续处理

O13 TCP采集与runner发布闭环仍未实现，继续以少量证券、独立来源合同推进，
不绕过日历、点时事实、收盘资格或冻结来源回读。O03/O04仍需实际交易窗口、
官方事实及目标生产环境。O05正式提交/新版本/注册表验收，以及O08/O16/O18/O20
剩余容量和兼容边界仍须按实际完成情况记录。
