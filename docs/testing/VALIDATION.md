# 测试与验证方法

更新日期：2026-10-24。目标是验证[需求FR-01至FR-18](../requirements/REQUIREMENTS.md)的可观察行为及故障边界。工程检查、真实源样本、数据库子集和目标生产验收分别记录，不累加重复执行的测试数量。

## 1. 常规门禁

从仓库根执行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
```

cargo test包含文档测试；all-targets编译由Clippy覆盖。外部skip变量按存在性判定，设0也会跳过；本地TCP/HTTP模拟仍执行。提前return并显示ok的外部用例不计为在线验收。ignored用例按明确前置条件单独执行。

## 2. 需求验证矩阵

| 需求 | 有意义的验证 | 不能由此推导 |
| --- | --- | --- |
| FR-01/02 日历与规则 | 两交易所逐日对照、导航越界、生效日两侧、未知证券状态 | 全市场事实、2027数据及未来政策 |
| FR-03 语义 | 市场/单位、财务资格、目录精度、事件分类、独立字段期望 | 未核实品种单位或聚合成交精确金额 |
| FR-04/12 网络与缓存 | 独立进程许可、deadline/取消、池排队/释放、启动CRUD/09:00、缓存容量/并发 | 公共源SLA、跨主机协调 |
| FR-05/06 文件与导入 | 旧输出保护、确定性、跨块状态、输入重叠、全键全字段回读、确认丢失 | 目标整库完整或跨库事务 |
| FR-07 协议 | 独立请求/正文/字段、截断/错身份/非法值、26命令注册表及98帧重放 | 26同码等同全部增强契约 |
| FR-08/09 名单和指标 | 分页/总量/重复、独立公式、预热/缺口、缩放不变量 | 官方源实时可用性或其他软件默认口径相同 |
| FR-10/11 工程交付 | MSRV、fmt、Clippy、路径/链接、许可证、包清单、证据摘要 | 远端CI、注册表发布 |
| FR-13/14 留证和观测 | 压缩/截断/篡改、传输前限额、阶段并集、失败释放、新鲜度范围 | 摘要证明来源真实性或自然日年龄证明缺少交易日 |
| FR-15/16 控制层 | Job去重、缺键/Unknown、全免行情、封存/目录CAS、固定版本消费 | 真实全市场事实或多机协调 |
| FR-17/18 数据库/恢复 | 唯一release、额外键/重复拒绝、unknown不重写、WAL备份/隔离恢复 | 数据库服务端灾难恢复、目标Linux与长期SLO |

测试位置见[开发说明](../development/DEVELOPMENT.md)。当前结论见[验收矩阵](PRODUCTION_ACCEPTANCE.md)，固定样本管理见[证据规则](EVIDENCE.md)。

## 3. 固定样本与独立重解

[核心夹具](../../crates/rustdx-core/tests/fixtures/README.md)包括day/gbbq、pytdx参考及7709命令报文。原始请求、header、wire、payload和独立期望保持摘要；合成样本明确标记。

```sh
cargo test -p jelven-rustdx-complete --test eltdx_330_commands --locked
cargo test -p jelven-rustdx-complete --test protocol_decoders --locked
cargo test -p jelven-rustdx-complete --test response_identity --locked
python3 scripts/validation/freeze_recent_minutes_fixtures.py \
  --output crates/rustdx-core/tests/fixtures/recent_minutes_nonempty --check
```

重新采样使用新目录。pytdx重解须先取得固定提交/依赖并核对源码摘要，对临时夹具副本执行；历史冻结的源码摘要不因当前重构而更新，必要时检出当时提交复现。

响应身份专项使用合法正文配错前缀、opcode或消息号，核对连接失效、禁止自动重试、池释放、握手/探测、心跳、统计、截断与离线重放。实现及本轮范围见[响应正确性修复](../archive/development/RESPONSE_IDENTITY_FIX_20261022.md)。

## 4. 外部和数据库验收

在线行情按业务选择示例和有效交易窗口，保存时间、节点、请求、返回及独立对照；`production_smoke`不代替全部26命令验收。[0feb编码修复](../archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)已完成两节点日期对照；通用日期资格、保存窗口和0547真实盘中主动推送仍单独取证。

数据库按[隔离验收说明](DATABASE_ACCEPTANCE.md)准备固定版本、工具、私有命名空间，执行小表、实际规模、重复写入及故障恢复。runner真实数据库用例还需对应RUSTDX_TEST_CH_*及RUSTDX_TEST_MONGO_*；不得因--include-ignored把缺前置条件的案例当成通过。

Unix本地CLI/故障回归可使用CI中的明确过滤命令。Linux/macOS CI检查完整工作区，Windows排除Unix runner。配置已提交或本机通过不证明远端实际执行。

## 5. fuzz和性能

```sh
cd tools/fuzz
cargo +nightly fuzz run checked_parsers -- -max_total_time=30 -max_len=4096
```

需要nightly及cargo-fuzz。ASan/覆盖率运行与普通目标执行分别记录；短时无崩溃不证明穷尽。异常保存最小复现并补有效回归。

```sh
cargo bench -p jelven-rustdx-complete --bench indicators --locked
python3 scripts/bench/measure_day_memory.py --help
```

窗口扩展观测与真实历史RSS对比须固定输入、参数、二进制摘要和环境；完整进程树与主进程分别测量，不把本机数据变成生产承诺。

## 6. 本次结构迁移验收

本轮重新执行工程检查，核对manifest、源码引用、CI/脚本、当前Markdown链接、crate清单及保留证据/夹具/参考日历字节。没有重新运行真实行情、数据库、远端CI或插桩fuzz。

最终命令、退出码、测试统计、文件保全与打包限制保存在[本次验证记录](evidence/project-restructure-20261024/verification.json)。历次具体统计与在线范围见[历史验证](../archive/validation/VALIDATION_THROUGH_20261024.md)。
