# 剩余验收第五阶段：RowBinary精度修复、Linux固定数据库与真实TCP对照

2026-10-24，延续[第四阶段](REMAINING_FIXES_STAGE4_20261024.md)。本轮修复数据库验收脚本的启动参数、隔离输入及容量测试，并修复稳定Linux ClickHouse的CSV导入浮点偏差，重新取得真实服务与真实协议样本证据。没有提交、推送、升版、注册表发布或生产部署；用户既有目录迁移保留。

支持包位于[acceptance-stage5-20261024](../../testing/evidence/acceptance-stage5-20261024)。实现、指定环境回归、真实样本与目标生产分别判定，不将模板、同版本号或非空响应当作生产通过。

## 修复与源码身份

三个脚本调用CLI时增加`--no-tcp-startup`：

- `scripts/bench/measure_database_readback.py`
- `scripts/validation/verify_database_faults.py`
- `scripts/validation/verify_real_day_database.py`

这些脚本使用本地文件及显式数据库服务；原调用会先测速公网行情节点，消耗故障注入的35秒任务预算，使测试可能在实际写入前失败。现在数据库验收时限用于本次数据库动作。依赖锁和版本不变；后续发现的导入正确性修复如下。

首次隔离编译发现两个测试直接引用docs中的3份编译期报文。现在`quote_precision/modern_commands`使用核心crate就近夹具，副本SHA与原证据完全一致；[副本来源](../../../crates/rustdx-core/tests/fixtures/protocol/stage5-provenance.json)另记，不修改旧报文或独立期望。其余运行期证据按测试实际读取范围加入隔离源码包，避免测试依赖未传入的工作区文件。macOS相关20项回归、fmt及全workspace/all-targets/all-features Clippy均退出0，见[本机检查](../../testing/evidence/acceptance-stage5-20261024/local-checks/results.json)。

首次505文件快照身份为`719783b57e3ddbfd6643a1b78c504264e618ba7501ba68f98083eb251bd57b8e`，[初始清单](../../testing/evidence/acceptance-stage5-20261024/source-manifest.json)与[归档](../../testing/evidence/acceptance-stage5-20261024/source.tar.gz)保留。第二份Linux输入共1,035文件，其中526份为运行期固定证据，身份为`55b7f0620f1bf40bd58b308bc31f03898e46edc3c7694798c4098e0863ff608a`；[清单](../../testing/evidence/acceptance-stage5-20261024/source-final-manifest.json)与[源码归档](../../testing/evidence/acceptance-stage5-20261024/source-final.tar.gz)保留。运行到98帧重放时发现动态拼接读取的12份退市日线JSON未加入清单，原工作区文件完整，隔离输入仍不完整。

补齐上述原字节后，输入共1,047文件，其中538份为运行期固定证据，身份为`cf3af420b5bf56878c1d92ed1f1f2a77a626352a9251f95b767a7cc3e9be888b`；[完整清单](../../testing/evidence/acceptance-stage5-20261024/source-complete-manifest.json)与[完整源码归档](../../testing/evidence/acceptance-stage5-20261024/source-complete.tar.gz)另存，不覆盖前两份快照。

Linux调试CLI为138,140,760字节，runner容量测试原来的48MiB空间会先在快照失败，未到数据库证据预算检查。修复后的测试先用实际CLI大小加原256MiB预算封存候选，注入目录提交失败保留sealed状态，再以独立48MiB数据库证据预算调用`database::prepare`。断言容量拒绝发生在凭据读取/网络请求/unknown目标登记之前，`targets/target_io/publications`均为空，数据库证据目录未创建。中间采用共享预算的尝试仍未测到该条件，[中间快照](../../testing/evidence/acceptance-stage5-20261024/source-tested-manifest.json)及失败日志另留；没有放宽生产容量规则。

容量修复后的输入为1,047文件、538份运行期证据，身份为`ea7a35a538580a717a31870b0311906aa379adca131371c41cd17d7361991d4f`；[清单](../../testing/evidence/acceptance-stage5-20261024/source-accepted-manifest.json)与[归档](../../testing/evidence/acceptance-stage5-20261024/source-accepted.tar.gz)独立保存。该输入通过Linux工程、大批量和故障检查，但真实历史的ClickHouse精确回读失败，不能作为最终通过输入。本机容量回归、fmt及Clippy见[补充检查](../../testing/evidence/acceptance-stage5-20261024/capacity-final-checks/results.json)。

RowBinary修复后的输入为1,048文件，仍含538份运行期证据，身份为`64588321c2d8e944c3a10a9cacd187eb0cef71d6992b92aee2fd227551fd925d`；[清单](../../testing/evidence/acceptance-stage5-20261024/source-rowbinary-final-manifest.json)与[归档](../../testing/evidence/acceptance-stage5-20261024/source-rowbinary-final.tar.gz)独立保存。新增适配器位于`apps/rustdx-cli/src/io/rowbinary.rs`；两处HTTP错误模拟夹具补齐完整数值列，避免被列校验提前拒绝。

该轮Linux发现分页间隔测试的40毫秒预算可能先被TCP延迟确认耗尽，错误为WouldBlock而非测试预期的TimedOut。测试现在为首帧保留250毫秒，设置1秒分页间隔，继续严格要求TimedOut，并断言750毫秒内结束、只发送第一页；没有放宽生产期限或错误类型断言。失败日志另存`evidence/pre-pacing-failure`。

最终执行输入仍为1,048文件/538份运行期证据，身份为`d080d6c1ed153150d10637f6dabbff2674baa8091b581f19da30bc64986f68cf`；[最终清单](../../testing/evidence/acceptance-stage5-20261024/source-pacing-final-manifest.json)与[最终归档](../../testing/evidence/acceptance-stage5-20261024/source-pacing-final.tar.gz)独立保存。中间RowBinary快照和本机编译/夹具失败记录也保留。Git HEAD不能单独标识包含未提交迁移及修复的工作区，二进制摘要分别记录；本轮文档另行核验。

## 稳定ClickHouse浮点精度修复

779,200行CSV的独立Python解码及1/4线程摘要一致后，ClickHouse25.8.33.6导入`2005-01-13/sh600000`时把low=6.94存为`4619499763780875714`，期望Float64位值为`4619499763780875715`。开启`precise_float_parsing=1`仍有相同偏差。原报告正确停在failed/acknowledged，CSV保留，未标记verified；失败现场保留在Linux支持包的`evidence/pre-rowbinary-failure`。

独立Python按Date UInt16、String VarUInt及六个小端Float64构造59字节RowBinary，真实服务精确返回原low位值，并生成非零纳秒version。探针原始响应、位值及payload见Linux支持包的`evidence/float-probe`。

CLI现在逐行将已发布CSV转为RowBinary，SQL显式列出实际字段，保留query_id、完整回执检查及表内默认版本。适配器在CSV分配前限制单行1024字节，只保留当前行和最多75字节编码；日期越出Date UInt16范围、未知/重复/缺字段、非法证券及非有限数值均拒绝。没有改变CSV字节、旧证据或精确回读标准。

本机22项I/O测试的Rust统计全部通过，其中真实ClickHouse旧入口因显式跳过变量未连接服务；四项新增测试实际执行，覆盖独立固定hex、1/2/7字节读取、6/8数值字段与列重排、负零位值、日期边界、坏输入及取消。fmt和全workspace/all-targets/all-features Clippy退出0，见[RowBinary检查](../../testing/evidence/acceptance-stage5-20261024/rowbinary-checks/results.json)。真实数据库结论以下述Linux复验为准。

## macOS本轮初始源码真实服务

使用独立目录与28123/29000/28018端口，实验ClickHouse26.11.1.548、MongoDB8.0.16、mongoimport100.12.2、mongosh2.5.10、Rust1.89。

| 检查 | 结果 |
| --- | --- |
| CLI真实小表 | 1通过，80.38秒；重复/更新、6/8数值字段、复权、CSV留存/删除及不兼容目标拒绝 |
| runner真实版本隔离 | 1通过，55.98秒；600只合成证券、1,200行、跨512行批次、两release、目录失败后恢复及旧版本导出 |
| 篡改与控制层恢复 | 同一runner案例核对额外键/重复拒绝、旧输出保护、控制账本及请求证据备份恢复 |
| 清理 | 本轮创建的两个服务进程均已停止 |

结果与环境见[mac/results.json](../../testing/evidence/acceptance-stage5-20261024/mac/results.json)、[mac/environment.json](../../testing/evidence/acceptance-stage5-20261024/mac/environment.json)、[清理记录](../../testing/evidence/acceptance-stage5-20261024/mac/cleanup.json)。其源码身份为本轮初始`719783b5…`，尚不包含后续RowBinary修复；不能据此声称最终源码已在该实验服务通过。数据库是真实服务，证券名单、日历和状态是合成软件测试输入，不是全市场事实。上述恢复不包含数据库服务端卷或日志。

## 两个真实TCP主站

北京时间2026-10-24，从`115.238.56.198:7709`与`180.153.18.170:7709`采集`sh600000/sh600519/sz000001`的052d category9日线。请求范围2026-10-19至22，每节点得到12条选定记录；每证券返回800条原始行情，因已越过范围下界而停止，没有在此次真实采样中验证空页结束。

每节点保留从握手开始的请求与解压前帧、库存SHA、连接/接收时间、分页receipt、schema8报告及CSV。独立Python解码器核对身份、长度、压缩帧、请求证券/参数和全部CSV字段；两节点CSV字节完全一致。

独立对照使用2026-10-22留存的官网ZIP原始成员。原包SHA-256为`07aab2f1166a794cebfa380cda058417499c4f74049a5fe36fe029f5c803d400`，原包没有改写。

| 字段或条件 | 范围内结论 |
| --- | --- |
| 日期、OHLC、金额 | 选定12条记录的OHLC/金额与day文件逐值精确一致；日期按所列原始字段核对 |
| 原始帧至CSV | 两节点均通过独立解码与数值核对 |
| 成交股数 | 未通过跨源精确核验；TCP转换股数减day整数股数为−94至+1股 |
| 全品种单位、真实业务覆盖 | 未验收，不能从3只股票推广 |
| 同日bar、五分钟、0547推送 | 本轮没有执行；周六采样不能替代盘中门禁 |

逐日差值见[tcp-independent.json](../../testing/evidence/acceptance-stage5-20261024/tcp-independent.json)，原始现场在[tcp-live-0](../../testing/evidence/acceptance-stage5-20261024/tcp-live-0)及[tcp-live-1](../../testing/evidence/acceptance-stage5-20261024/tcp-live-1)。换算为整数只证明转换结果的表示，不恢复源量精度；应用规则已同步到[接入约定第9节](../../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。不修改原始量，不以容差关闭精确验收。

## Linux固定配置

隔离运行在Ubuntu24.04.5 x86_64的临时目录，使用独立Docker27.5.1 daemon、socket、数据和执行目录。没有修改系统iptables、bridge或IP转发；容器使用host network并仅监听loopback。ClickHouse限制2CPU/2GiB、MongoDB限制2CPU/1GiB，查询和缓存限额沿用验收配置；端口差异另存实际XML。

固定ClickHouse25.8.33.6 Alpine索引SHA为`87e0a5b72f5465b18eacca7c76850e7ff551c9795c50e451f5646299e5e24146`；MongoDB8.0.16索引SHA为`4b58ebcb1dc7a7b4e84cd8ce9098d48764ae4478876898ff9551acf2ac4a6a6d`。因公共入口超时或下载过慢，使用镜像入口取得同一digest，不能据此称Docker Hub的本轮下载已通过。

Rust1.89组件逐项匹配已核验的官方发行manifest。补齐Linux专有`linux-raw-sys0.12.1`时同样核对既有Cargo.lock checksum，锁文件不变。Mongo离线协议检查已通过：以mongosh2.5.10内嵌Node执行原`.cjs`，没有连接数据库，日志包含完整成功标记。

最终源码的Linux统计如下，原始统计见[linux/evidence/test-statistics.json](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/test-statistics.json)。

| 检查 | 结果 |
| --- | --- |
| workspace/all-targets/all-features | 484通过、0失败、9 ignored；36.81秒 |
| 文档测试 | 66通过、0失败、18 ignored |
| fmt、Clippy、构建、严格文档 | 均退出0；Clippy与rustdoc拒绝警告 |
| CLI真实小表 | workspace内实际执行，1,040键，重复/更新、6/8字段、复权、留存/删除及错误目标拒绝 |
| runner完整复验 | 31通过、0失败、0 ignored；其中19项control_plane，真实两库600只合成证券/1,200行跨512批次及两release恢复 |
| 65,536行重复导入 | 两库各两次，全部键及字段verified；脚本21.59秒 |
| 四项中断恢复 | 两库确认丢失/SIGKILL，同表重跑8,192键精确核对，CSV摘要及逻辑行数不变；脚本8.82秒 |
| 779,200行真实历史 | 独立Python逐行解码、1/4线程CSV一致；两库各两次全部键与字段verified、重复后总行数不增加；脚本211.78秒 |
| 临时systemd smoke | 在NoNewPrivileges/128MiB/30秒限额下运行CLI `--no-tcp-startup show --version`，退出0 |

workspace环境设置`RUSTDX_DB_ACCEPTANCE=1`使CLI两库案例实际执行；同时设置`RUSTDX_SKIP_INTEGRATION_TESTS=1`跳过部分公网/旧服务入口，Rust的passed计数包含这些提前返回，不能写作484项全部联网执行。runner另用`--include-ignored --test-threads=1`和完整`RUSTDX_TEST_*`执行真实数据库案例，没有缺环境提前跳过。独立TCP事实见前节。

全部18条最终命令退出0，见[结果](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/results.json)。真实历史使用从原官网ZIP保留的128个原字节成员，日期/OHLC/金额/量全部独立核对，六次输出CSV的SHA-256均为`80f1b74671bcc1d8d3aa5bf7693e5dbacdcf04c60f40300255f23694fffa46e5`。ClickHouse两次分别30.65/37.21秒，MongoDB两次68.35/65.25秒；每次精确回读全部779,200键，结束删除本轮私有数据库，见[真实历史结果](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/real-history.json)。

[环境记录](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/environment.json)保存最终源码、Cargo.lock、两个debug二进制、Rust工具及实际系统身份；[容器](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/containers.json)与[镜像](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/images.json)保存固定digest、限额及实际配置。执行后重新核对源码全部1,048个文件。临时systemd命令只验证受所列限额管理的CLI可执行，未安装或运行生产runner/metrics模板。

[清理审计](../../testing/evidence/acceptance-stage5-20261024/linux/evidence/cleanup-audit.json)确认自建daemon已无容器，清除其24个私有匿名卷后为空，回收约1.85GB，再按命令行所属路径核对并停止本轮daemon。下载缓存、源码、二进制及原始scratch/日志留存，没有修改共享系统配置。Linux支持包共345个文件，归档905,577字节、SHA-256为`c871c69ba046bc01e163bfcdc3365fcee9d7138ad1d00f35504b52037547fe60`；逐文件打包摘要见[清单](../../testing/evidence/acceptance-stage5-20261024/linux/packaged-files.json)。

最终本机核验见[final-verification.json](../../testing/evidence/acceptance-stage5-20261024/final-verification.json)：各轮源码归档、自身逐文件身份、最终工作区一致、原3份编译期报文副本、128个ZIP成员、Linux打包文件及传输摘要、实际命令退出码、清理审计和当前文档本地链接。复验脚本保存在[reproduce](../../testing/evidence/acceptance-stage5-20261024/reproduce)；旧失败材料不覆盖为新通过结果。

## 仍须独立关闭的门禁

本轮工程样本不关闭以下范围：用户生产账号/ACL和同UID治理、硬磁盘配额、完整进程树容量与长期SLO、真实全市场点时名单/状态与全量数据、数据库服务端备份恢复及业务批准RPO/RTO、2027官方日历、0feb交易日冲突、0547实际盘中主动推送、全部品种量单位及正式版本发布。

CI工作流配置未推送，本轮远端命令不是GitHub Actions运行。Linux基础执行或临时systemd命令也不等于已安装生产服务模板、权限和调度配置。
