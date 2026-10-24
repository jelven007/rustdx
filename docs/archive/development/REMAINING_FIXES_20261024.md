# 剩余优化修复记录

2026-10-24，继续 [20项项目评审](../architecture/CURRENT_PROJECT_REVIEW_20261022.md) 的响应身份修复后续工作。保留已有未提交迁移、冻结协议字节和历史验收摘要。使用Rust2024、MSRV1.89、同步I/O及有界线程，不引入异步运行时。

## 第一阶段：日期资格与数据库发布

| 事项 | 本轮实现 | 仍需验证 |
| --- | --- | --- |
| O02 / P0 | 0feb提供不可直接改写的日期资格；`require_trading_date()`拒绝日期依赖发布；公开兼容日期字段不能提升资格 | 真实交易日、休市日、窗口外日期、多节点与终端交叉对照 |
| O06 / P1 | ClickHouse和MongoDB使用完整键分页，索引顺序一致；单日读取直接过滤；每目标复用mongosh，query_id配对，EOF和退出码确认，取消/超时终止进程组 | 新源码真实两库、不同规模P95/吞吐/RSS |
| O07 / P1 | 传输前按封存行数和JSON字节预测I/O、证据字节/条目、32MiB证明文件及磁盘空间；多目标共同预算；预留一次恢复和一次逐分区完整消费；另存capacity证据 | 固定Linux数据库配置的容量实测 |
| O08 / P1（部分） | 数据库会话复用已校验配置；请求/响应增量计量；开始和结束完整对账 | worker监督扫描和多次消费/证明验证仍有重复扫描 |
| O12 / P1 | 数据库CI预建runner隔离版本化表和唯一索引，设置全部RUSTDX_TEST_*，显式运行include-ignored的control_plane验收及JS协议回归 | workflow尚未推送/运行，不宣称远端通过 |
| O15 / P1 | mongosh通过环境读取URI；mongoimport通过0600临时配置文件读取URI；成功/失败后清理 | 固定100.12.2工具的真实服务运行 |

日期资格始终为 `Unverified { reason: ServerDoesNotEchoTradingDate }`，请求参数、非空、点数齐全或selector相同均不能证明日期。当前不提供无证据的资格提升入口。原始结果仍可用于留证和研究，日期消费合同见 [APPLICATION_INTEGRATION](../../development/APPLICATION_INTEGRATION.md)。

数据库分页的相等前缀为dataset/release，排序后缀为 `(contract_version,date,market,code,instrument_class)`。ClickHouse游标显式将日期转换为Date；MongoDB使用simple collation和同顺序字典序条件。逐批全字段精确比较、总数量及终止空查询仍有效，未知写入不自动重试。

容量预测基于封存数据，而非配置max_rows。设全量批次数为B、逐分区批次合计为P、分区数为D，预留逻辑操作数为 `3B + P + 2D + 8`，包括首次插入、发布回读、一次恢复和逐分区消费的固定查询。预测为保守上界，证明文件与证据限额也会先行拒绝，不能把50,000次简单理解为可写入行数。多次额外恢复/读取继续受剩余额度约束。

传输前请求原字节和SQLite意图仍先持久化；MongoDB管道线程仅允许一个未完成请求，逐块保留有界响应。半帧、超长、错query_id、异常退出、超时或取消均不能变成成功数据。会话边界重新扫描静止证据树；目标文件系统配额仍承担硬限制。

## 本机验证范围

机器证据见 [remaining-fixes-20261024.json](../../testing/evidence/remaining-fixes-20261024.json)。全workspace格式、all-targets/all-features Clippy、454项测试与66项doctest通过；6项ignored测试和18项ignored doctest未在该全量命令执行。之后新增凭据工具替身回归通过，最终按影响范围重复检查runner及CLI。

显式执行了5项原ignored控制层测试：真实CLI发布/修订与恢复、写确认丢失、容量不足先拒绝、封存后恢复、TCP启动池和离线参数传递。HTTP故障节点和行情启动节点是本地模拟服务；不算真实数据库或公共行情采样。

新增回归包括：完整键跨512行及日期边界、单日分区查询、完整响应和EOF、异常退出、截断及超长留证、堵塞管道下的取消/超时及后代清理；JavaScript使用离线MongoDB替身验证持久连接、字典序、date过滤、query_id和批次上限；CLI进程替身验证URI不在argv、凭据文件0600及成功/失败清理。

本机无Docker、mongosh或数据库监听。JavaScript检查使用本机Electron自带Node22.21.1执行；没有向真实数据库写入，没有运行新远端CI，没有提交/推送/发布。0feb实盘日期、0547盘中推送、2027官方日历、点时事实和目标Linux生产矩阵保持待验收。

## 后续事项

本阶段没有关闭O03/O04的实盘与生产事项，也没有形成O05正式发布提交。O09批量流式/字节预算、O10来源元数据、O11区间完成状态、O13 TCP Job、O14探测协调与O16–O20仍需继续实现或验证；后续结果另记，不能由本阶段通过推导全部20项完成。
