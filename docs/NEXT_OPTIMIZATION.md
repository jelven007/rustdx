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

## 剩余步骤

5. 有界分块写入、跨线程确定性输出及内存观测。
6. 东财标准行情适配与低层不可变请求/独立解码兼容入口。

此文件随步骤更新，最终证据以实际测试/服务结果为准。
