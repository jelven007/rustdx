# 后续优化与交叉验证

任务日期：2026-10-23。基线 `9a09ca0`，保留Rust2024/MSRV1.89与同步有界架构；本轮未发布新crate。

## 第一步：数据更新与证券状态证据

新增 `rustdx data-check` 与 `SecurityHistory`。更新前逐日比较独立日历，证券状态校验区间、上市阶段和公告原始字节摘要；可选原子发布。未知区间返回None。详细格式及来源见[维护说明](DATA_MAINTENANCE.md)。

本轮读取沪深交易所官方2026日历，分别整理两份数据。命令逐日核对通过，共242个交易日；回归将全年365天同时与内置日历比较。错误参考、范围不一致、状态重叠、未知日期及多日IPO状态回归通过。未取得2027完整数据及全市场证券生命周期数据，未伪造或扩大覆盖。单一交易所当前规则复查不替代全部板块续期。

离线workspace/all-features回归通过（核心157、CLI库4、命令23及独立日历1，另有既有集成/属性/52项文档用例）；Clippy拒绝警告及格式检查通过。外部集成使用跳过变量，本段不计为在线验收。

## 第二步：当前源码与实际服务验收

`production_smoke`改为UTC+8最近收盘日的动态区间，检查日线最新边界及OHLC合法性。本次2026-10-23凌晨实测：股票/指数日线各38根、股票边界2026-08-24至2026-10-22，前复权192根；当日/历史分钟各240、当前/历史逐笔各20、除权45、概念记录41369、F10类别/内容各16。证券目录分别24462与27697条。以上是接口与所列边界/数值约束验收，未独立审计所有财务或目录字段。

东财HTTPS收盘快照2只、total=5563，日期2026-10-22，CLI清单complete。保留实际输出[样本](../rustdx-cmd/tests/fixtures/east-20261022.csv)，SHA-256=`0318b6b583e38a5f16aefb624f6029fe517baa9805847b26a7fa31d87d0cf10c`。新增`source_crosscheck`，对同日同股独立拉取通达信日线：两股OHLC精确一致，成交量分别差1/0手，成交额分别差18.09/9.37元。量额容差为2个f32相对ULP、至少1手/元，用于声明32位量额编码和来源口径的比较界限；数据库回读仍精确比较，不使用该容差。两只股票不能代表全市场验收。

隔离服务只监听本机非默认端口：ClickHouse HTTP18123/TCP19000、MongoDB27018，独立临时数据目录。实际ClickHouse为官方macOS master实验构建26.11.1.548，MongoDB8.0.16，Database Tools100.12.2，mongosh2.5.10。新增opt-in `database_acceptance`，小表1040键、3批，验证两个目标的重复导入、更新覆盖、六个普通数值字段及八个含因子字段、清理/保留文件、ClickHouse FINAL/实际合并、MongoDB唯一索引；错排序键/已有重复数据保留旧产物并拒绝写入。实际测试通过。未重跑2981万行全量导入，也不替代目标生产版本/配置验收。

复现入口（在已配置的隔离服务上）：

```sh
RUSTDX_DB_ACCEPTANCE=1 cargo test -p jelven-rustdx-cli --test database_acceptance -- --nocapture
cargo run --example production_smoke
cargo run -p jelven-rustdx-cli --example source_crosscheck -- rustdx-cmd/tests/fixtures/east-20261022.csv
```

数据库测试要求明确CLICKHOUSE_URL/MONGODB_URI及PATH中的MongoDB工具；只使用进程唯一的`rustdx_acceptance_*`数据库。它不是日常离线测试的外部依赖。

## 第三步：独立回读与写入协调

增加512键上限的流式验收，逐键精确比对全部导出字段，schema3清单新增database_verification。状态先unknown、回执后acknowledged、独立查询通过后verified；acknowledged但回读失败保留CSV。导入前再次校验发布摘要，清理函数拒绝任何未verified状态。MongoDB先建立/核实完整唯一索引、simple排序规则，启用stopOnError；空集合初始化由实际验收发现并修正。JSON使用float_roundtrip避免解析引入舍入差异。同机同目标任务在解析/行情采集与前因子读取至验收完成期间互斥，跨工作目录有效。

回读结果包含行数、逻辑键、边界、批次、数值字段列表及独立返回值摘要，范围仅为本次输出键。缺键、重复键、数值变化、迟到HTTP异常、查询失败、摘要变化、非零子进程和超时回归通过。Rust1.89 workspace/all-features离线测试、Clippy全目标拒绝警告通过；最后修改的相关检查继续随后续步骤执行。细节和跨主机/外部写入限制见[IMPORTS.md](IMPORTS.md)。

本次临时日志：`/tmp/rustdx-next-production-smoke-current.log`、`/tmp/rustdx-next-source-crosscheck.log`、`/tmp/rustdx-next-database-acceptance.log`、`/tmp/rustdx-next-step3-workspace.log`、`/tmp/rustdx-next-step3-checks.log`。日志不是长期备份，本文件和仓库样本是持久验证记录。

## 第四步：任务时限、取消与共享连接资源

新增OperationContext绝对截止时间/取消令牌，CLI顶层`--timeout`默认10800秒，`--cancel-file`和Ctrl-C/SIGTERM协作取消。覆盖下载/解压/名单/因子/解析/CSV回读/数据库导入与独立验收；重试不能延长。终端备选输入在任务模式改为要求下次显式提供。失败清单仍持久化，未知或未验收数据库写入仍保留CSV。TCP批次及解析线程显式继承上下文，HTTP读取每200ms检查，健康长正文不会因轮询被截断。

TCP/HTTP共用进程许可，可配置同机共享OS文件锁，CLI默认共享8个槽位。独立进程、不同工作目录的饱和回归通过，强制终止后许可释放，锁文件保留；配置容量不一致拒绝。Mongo子进程预约一个任务许可，其驱动内部socket数不在逐连接预算承诺内。保留同步I/O与有界线程，不引入异步运行时。

清单resources增加峰值连接、等待总量、请求/失败/空响应、最近最多1024样本的最近秩P95及样本数。慢HTTP正文、静默正文取消、完整CLI下载截止/取消及旧CSV保留、TCP静默响应取消、数据库子进程任务deadline、嵌套上下文、P95窗口与配置冲突回归通过。Rust1.89 workspace/all-features通过（核心160、CLI库4、命令26、52项文档及其余集成/属性用例）；Clippy全目标拒绝警告、格式检查通过。外部测试跳过不计作在线验收。

再次实测东财HTTPS两只股票，日期2026-10-22，CSV摘要与第二步保留样本完全一致，清单complete；资源观测active=0、peak=1、requests=1、failures=0，单样本耗时455533微秒。一个样本不能代表生产P95。详细边界及配置见[NETWORK.md](NETWORK.md)。临时日志：`/tmp/rustdx-next-step4-validation.log`、`/tmp/rustdx-next-step4-final.log`。

## 第五步：有界分块、确定性输出与实际内存

day用128KiB缓冲建立并摘要私有磁盘快照，每块4096条。QualityAccumulator跨块检查日期并限制每类64条详情，省略数量单独记录。累计收益承接上块末日收盘价和因子，除权事件按前基准日期过滤。A股严格升序可直接暂存CSV；非A股固定块排序、最多8路多轮磁盘归并，跨块重复仍保留源最后一条。结果通道每线程容量1，传磁盘产物及元数据；主线程按目录参数、文件名、日期顺序消费。错误退出先释放接收端，协作取消无发送死锁，旧输出保持。

回归覆盖1/4/8线程CSV逐字节一致、反向创建文件及目录参数顺序、9块多轮归并的末源重复胜出、跨4096条边界日期缺口/非法日期、取消及暂存文件清理。完整与实际分块管道的累计收益CSV在上市基准和前因子两种模式下逐字节一致，除权事件横跨停牌与块边界。Rust1.89 workspace/all-features测试通过（核心160、CLI库4、命令27、新增端到端2项、52项文档及其余集成/属性回归），Clippy全目标拒绝警告及格式检查通过；外部skip项不计在线验收。

正式对比基线0892427与新源码的不同摘要release二进制。macOS arm64、Rust1.89、opt-level3/LTO，4个合成A股day文件，升序日期、固定OHLC量额，不含gbbq/previous/数据库；每组仅一次。`/usr/bin/time -l`测整命令峰值RSS，包含发布前回读和清单。

| 每文件记录 / 总记录 | 线程 | 基线RSS MiB | 新版RSS MiB | 基线 / 新版耗时秒 |
| --- | --- | --- | --- | --- |
| 20,000 / 80,000 | 1 | 19.6 | 8.2 | 0.267 / 0.739 |
| 20,000 / 80,000 | 4 | 25.9 | 10.4 | 0.266 / 0.240 |
| 200,000 / 800,000 | 1 | 192.7 | 8.6 | 1.501 / 1.297 |
| 200,000 / 800,000 | 4 | 219.5 | 10.9 | 1.469 / 0.900 |

大样本4线程RSS下降约95%，输入10倍时新RSS增加约5%；单次时间有缓存/系统噪声，小样本1线程本次变慢，不据此承诺所有场景提速。每个样本新旧逐证券字段摘要一致，新版线程数切换的整体CSV摘要一致。脚本先拒绝相同二进制摘要，避免共享Cargo缓存混淆版本。原始数值、两二进制和CSV摘要保存在[JSON证据](fixtures/day-memory-20261023.json)，复现脚本为[scripts/measure_day_memory.py](../scripts/measure_day_memory.py)。分别构建基线和新版到独立target目录，再传`--baseline <旧二进制> --current <新二进制> --output <结果JSON>`。

该测量只验证记录相关内存。源文件列表/清单/区间索引仍O(文件数)，gbbq和前因子仍各64MiB有界内存快照，非A股归并与线程数也影响峰值。磁盘快照/暂存增加I/O和临时空间；强制终止不保证清理，见[IMPORTS.md](IMPORTS.md)。临时日志：`/tmp/rustdx-next-step5-release.log`、`/tmp/rustdx-next-step5-memory-final.log`。

## 第六步：东财标准适配与不可变K线请求

新增EastCanonicalBar/Day::to_canonical_with_calendar，标准日线携带Eastmoney来源、Day周期、Raw价格口径、UTC+8的15:00时点及股单位；同时保留源快照时间、源手单位、前收和可选ReturnIndex。市场、证券、源日期/交易日、已收盘及完整OHLC量额独立检查，CLI当前行情日验证和前因子日期约束保留。factor=0对应None，advance_return_index只接受有效收益类型，原OHLC不改写。现有CSV字段、手单位和JSON数组兼容；穷尽匹配BarSource的下游须处理新增Eastmoney分支。

新增KlineRequest私有参数和checked编码，股票/指数独立decode函数返回不同类型，不把发送字节、响应和结果存在同一对象。Client单页、区间、批次及复权拉取改用新路径，避免预填结果/响应持有/结果复制；KlineBuilder::build_request提供新入口，旧build/Kline/IndexKline/parse和预填data兼容。旧try_parse共用checked解码，拒绝截断、尾随字节、超数量和非法年月日/时分，失败清空旧结果；参数数量要求1–800。其他低层协议仍有旧兼容入口，未声称全部协议已迁移。

实际捕获股票日线、指数日线、股票5分钟线各3根，用独立pytdx提交2857fdad08534533610bd2aeca387f760c4baa42核对请求字节并生成期望CSV。Rust逐字段精确比较时间、OHLC、量额及指数涨跌家数通过；同时验证新旧兼容入口。参考源码摘要、请求/响应/期望摘要和数据保存在[协议夹具](../tests/fixtures/kline/manifest.json)，复现入口为[verify_kline_pytdx.py](../scripts/verify_kline_pytdx.py)及protocol_capture示例。生产无pytdx依赖。TCP量保持源数值，单位须按品种/周期核实；旧Bar::from_kline仅兼容A股日线手单位，分钟周期使用显式单位适配。

## 最终交叉验收

| 验收层 | 实际结果 | 覆盖限制 |
| --- | --- | --- |
| Rust1.89离线workspace/all-features | 核心160、CLI库5、命令27、新解码3、分块端到端2、54普通文档及1 compile_fail，其他属性/离线回归通过 | 外部skip项不计在线通过 |
| Clippy/格式/构建 | 全目标/全部features -D warnings、fmt及release构建通过 | 本机macOS arm64；远端CI需单独核验 |
| ASan/覆盖率fuzz | 新股票/指数日线及分钟解码纳入目标，627,152次/31秒，无崩溃 | 短时有界运行，非穷尽 |
| 当前源码通达信 | 股票/指数日线各38，股票边界2026-08-24至2026-10-22；前复权192、分时各240、逐笔各20、除权45、F10各16、板块41369，证券列表24462/27697 | 接口/边界/OHLC约束；未独立审计全部财务和目录字段 |
| 实际数据库 | ClickHouse26.11.1.548（官方macOS master实验构建）、MongoDB8.0.16；1040键、3批精确回读、重复、更新覆盖、约束及文件保留/清理通过 | 隔离小表，不替代生产版本、全量和跨主机验收 |
| 当前源码东财及跨源 | 两股、total5563、行情日2026-10-22，complete；CSV SHA与第二步夹具一致。与独立TDX同日OHLC精确一致，量分别差1/0手，额差18.09/9.37元，在声明编码容差内 | 两股不能代表全市场；数据库校验不用该容差 |
| 实际分块内存 | 大样本4线程219.5→10.9MiB，字段摘要一致；输入10倍新RSS约增5% | 第五步参数/二进制的单次测量，非全部场景性能承诺 |

最终release SHA-256=`e6c5241b2a480361a2f422768a0fa55019a951a25b11a4c578d52e5c1adf498f`。东财本次resources为active0、peak1、requests1、failures0、单样本412420微秒；一个样本不代表生产P95。实际命令/退出码、版本、东财原清单、日志摘要及局限保存在[最终验收JSON](fixtures/final-acceptance-20261023.json)。临时日志为`/tmp/rustdx-step6-final-checks.log`、`/tmp/rustdx-step6-services.log`、`/tmp/rustdx-step6-fuzz.log`。

六步实现及已能取得的交叉验证全部完成，架构、需求、开发、验证、导入、网络、迁移及历史验收文档已对齐。本轮只保存本地提交，未推送或发布新crate。临时数据库在验收结束后停止，数据目录保留。2027完整休市表、全市场证券状态、全部板块规则续期、目标生产配置/全量负载和长期网络P95仍需相应真实数据与环境，未虚构完成。
