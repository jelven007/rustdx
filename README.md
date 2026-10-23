# rustdx

Rust行情库、数据采集/导入工具及单机生产控制层，采用同步I/O和有界线程。核心库提供通达信TCP行情、本地文件解析、复权、指标、日历/日期规则、连接池和缓存；CLI提供日线历史、东财收盘快照、CSV/JSON和数据库导入；`rustdx-runner`提供持久任务、覆盖校验、版本发布和恢复。

最低Rust1.89，edition2024。核心包`jelven-rustdx-complete`、导入名`rustdx_complete`；CLI包`jelven-rustdx-cli`、库名`rustdx_cli`、二进制`rustdx`。Cargo版本仍为1.12.2/1.8.2，crates.io同版本是2026-10-11发布内容；后续源码优化尚未发新包，需要当前功能请从源码构建。

当前源码包括严格日历/日期规则、TCP/HTTP共享预算、整任务取消、4096条分块导出、checked协议请求、东财及TCP留证重放、schema6清单和数据库精确回读。兼容CLI已有779,200行真实历史子集及四项故障证据；新增控制层另通过16项回归，真实本机两库使用600只明确模拟证券/1,200行。范围分别见[第三轮记录](docs/history/OPTIMIZATION_ROUND3.md)、[组件实施](docs/development/PRODUCTION_IMPLEMENTATION.md)。

[完整需求](docs/requirements/REQUIREMENTS.md) · [项目目录](docs/architecture/PROJECT_STRUCTURE.md) · [技术架构](docs/architecture/PROJECT_ANALYSIS.md) · [文档索引](docs/README.md)

## 构建与安装

```sh
cargo build --workspace --locked
cargo install --path rustdx-cmd --locked
rustdx show -v
```

应用使用本轮核心库可配置源码路径：

```toml
[dependencies]
rustdx-complete = { package = "jelven-rustdx-complete", path = "/path/to/rustdx" }
```

安装已发布版本：`cargo add jelven-rustdx-complete --rename rustdx-complete`或`cargo install jelven-rustdx-cli --locked`；同版本号不含后续源码功能。源码安装可使用本地路径，或已授权推送版本的Git引用。发布/历史快照见[版本交付](docs/release/RELEASES.md)。

## 行情API

```rust,no_run
use rustdx_complete::tcp::{TcpConfig, stock::Client};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TcpConfig {
        timeout: Duration::from_secs(5),
        auto_reconnect: 2,
        retry_delay_ms: 200,
        recheck_empty: true,
        ..Default::default()
    };
    let mut client = Client::with_config(&config)?;
    let quotes = client.quotes(&[(1, "600000"), (0, "000001")])?;
    let bars = client.k(1, "600000", Some(20260901), Some(20260930))?;
    for row in client.k_batch(&[(1, "600000"), (0, "000001")], None, None, 2)? {
        let bars = row.result?;
        println!("{}: {} bars", row.code, bars.len());
    }
    Ok(())
}
```

| 能力 | 入口 |
| --- | --- |
| 快照/五档盘口、股票/指数K线 | Client::quotes、bars、index_bars |
| 区间/批次 | k、bars_range、index_bars_range、k_batch、bars_batch |
| 价格前/后复权 | k_adjusted、Adj |
| 当日/历史分时、逐笔 | minute、history_minute、transaction、history_transaction |
| 财务、除权、证券目录 | finance、xdxr、stocks |
| F10、原始板块 | f10、f10_categories、block |

TCP市场0深圳、1上海；股票/指数须使用对应解码器。标准Symbol支持sh/sz/bj，普通TCP支持范围须另核实。优先使用Client及checked不可变请求；旧parse与公开字段对象仍有兼容限制，见[协议说明](docs/architecture/PROTOCOL_REQUESTS.md)。

受管主连接/池/重连共享默认8个进程许可，批次排队、批次内有界并行，结束释放连接；首次初始化前configure_network配置策略。CLI以同目录OS锁共享同机TCP/HTTP上限，裸socket、不同目录和跨主机不在此保证内，见[网络预算](docs/architecture/NETWORK.md)。

## 数据口径

标准行情价格/金额为元、量为股、时间为UTC+8，CanonicalBar带来源/周期/价格口径。day/lc源量为股，day/东财CSV保留手；TCP源量按品种/周期显式核实，旧Bar::from_kline只兼容A股日线手→股。

factor是累计收益指数，原始OHLC不改写；价格复权乘数是另一种口径。增量需要有效前因子及日期衔接，跨停牌事件按顺序处理。价格复权按后端可用历史首/末日锚定，未必是上市日。

内置日历仅覆盖2024–2026，严格接口校验整个导航范围，外部日历以`--calendar`显式使用；旧工作日估算不能证明完整性。历史涨跌停须输入当日板块/ST/上市阶段，规则核实截至2026-10-22；主板ST自2026-07-06起为10%。未知证券状态不得猜测，旧静态概念/行业名称表已弃用。详细规则见[日期规则](docs/requirements/LIMIT_RULES.md)、[迁移说明](docs/release/MIGRATION.md)。

## CLI采集

```sh
# 目录、官网完整包、指定URL或本地ZIP
rustdx day ./vipdoc/sh/lday ./vipdoc/sz/lday \
  -o stocks.csv --report day-run.json --workers 4
rustdx day
rustdx day https://data.tdx.com.cn/vipdoc/hsjday.zip
rustdx day ./hsjday.zip

# 严格质量、显式名单、Excel第0列
rustdx day ./lday -o stocks.csv --strict
rustdx day ./lday -l sz000001,sh600000
rustdx day ./lday -l stocks.xlsx -x 0 -e sz

# 收益接续；使用实际采集版本的gbbq
rustdx day ./lday -g ./gbbq -o returns.csv
rustdx day ./incremental_lday -g ./gbbq -p ./factor.csv -o incremental.csv

# 当前行情日收盘快照及上一交易日收益接续
rustdx east -o east.csv --report east-run.json
rustdx east -j -o east.json
rustdx east -p factor.csv -o incremental.csv

# 全局参数置于子命令前
rustdx --timeout 10800 --network-limit 8 --cancel-file stop.flag \
  day ./lday -o stocks.csv
rustdx day --help
```

day默认最多8线程、可选1–64，单文件32MiB；4096条块跨块承接质量/因子，固定目录参数/文件名/日期输出顺序，切换线程数CSV字节相同。A股重复/逆序日期拒绝，非A股兼容异常默认告警并排序/保留末源同日记录；strict拒绝告警。跨输入同证券区间重叠另行拒绝。全空集合与坏输入不覆盖旧成功输出，路径别名/输入覆盖及输出锁在相关采集前检查。

day/east默认保留`<output>.manifest.json`和每run_id归档，schema6包含来源/输出摘要、阶段、数据库确认、覆盖/新鲜度及资源观测。CSV发布前回读后原子替换，JSON由已校验记录原子导出。退出0且complete是流程完成，运营还须核实应有证券及行情日；恢复见[导入说明](docs/operations/IMPORTS.md)。

下载缓存按来源隔离并持锁至解析结束，续传核实版本/字节边界；HEAD最多30秒、单下载最多2小时并受整任务deadline限制。east仅当前已收盘且字段完整的证券可发布，无收盘状态未知；前因子须属于上一交易日，漏跑先补历史。公共源限流可能失败，收盘后单任务调度由部署环境安排。

```sh
rustdx data-status --report coverage.json
rustdx data-check calendar config/calendars/sse-2026.json \
  --reference config/calendars/szse-2026.json
cargo run -p jelven-rustdx-cli --example east_replay -- run.json replay.csv
```

覆盖巡检默认提前90自然日告警，strict保留JSON并非零退出；不会自动延长证据范围。重放检查原始页摘要、请求/版本/页序，默认factor=0。更新和留证见[数据维护](docs/operations/DATA_MAINTENANCE.md)、[东财重放](docs/operations/EAST_REPLAY.md)。

## 数据库

```sh
# 按部署说明创建服务并在进程环境注入实际凭据
export CLICKHOUSE_URL=http://127.0.0.1:8123
export CLICKHOUSE_USER=rustdx
rustdx day ./lday -o clickhouse -t rustdx.bars_v2 -k
rustdx east -p clickhouse -o clickhouse -t rustdx.bars_v2

# 系统需mongoimport/mongosh
export MONGODB_URI=mongodb://127.0.0.1:27017
rustdx day ./lday -o mongodb -t rustdx.bars -k
```

ClickHouse新表ReplacingMergeTree(version)，逻辑键(date,code)，即时去重查询用FINAL；现有字段/精度/分区/排序/版本须兼容。Mongo先建立/核实完整唯一索引，再upsert。数据库完整回执后默认4096键一批、可配RUSTDX_VERIFY_BATCH_ROWS=1..16384，独立精确核对全部本次导出键及数值；verified后才清理CSV。

unknown表示可能部分写入，acknowledged只是成功回执；失败保留CSV、非零退出，不自动重试不确定写入。同机同目标锁覆盖前因子读取至验收结束，不提供跨主机互斥或跨库事务。详细状态见[导入说明](docs/operations/IMPORTS.md)，凭据/固定镜像/验收操作见[配置](config/README.md)、[部署](deploy/README.md)、[数据库验收](docs/validation/DATABASE_ACCEPTANCE.md)。

本机已验证779,200行真实子集两库首次/重复及四项恢复；实际ClickHouse为macOS实验26.11.1.548，MongoDB8.0.16。固定Linux镜像和数据库CI已配置但未执行，用户生产配置/全量负载/备份恢复仍待验收。

## 单机生产组件

```sh
cargo +1.89.0 build -p jelven-rustdx-cli -p rustdx-runner --locked
rustdx-runner run --config /etc/rustdx/job.json
rustdx-runner status --root /var/lib/rustdx
rustdx-runner recover --root /var/lib/rustdx
rustdx-runner export --root /var/lib/rustdx --dataset a-share-day --version 1 \
  --from 2026-10-22 --through 2026-10-22 --output /srv/exports/bars.csv
```

runner目前接收本地A股day、版本化日历/点时名单/证券事实及公告原字节，契约固定日线原价、人民币元/股。SQLite WAL保存Job/Attempt；Unknown或缺一个应有键拒绝发布。产物封存后，可选新命名空间CH/Mongo进行insert-only写入、全字段和release总范围回读；全部目标verified才CAS提交完整日期目录。消费者固定版本读取，恢复不重写unknown，在线备份包含控制账本和原字节归档。

根目录使用本地持久Unix盘；单机锁不提供跨机HA。模板及代码通过不能替代目标Linux、真实全市场事实、权限隔离、全量吞吐和长期SLO验收，MongoDB每请求启动mongosh的成本也须实测。命令/配置见[runner](rustdx-runner/README.md)，Linux调度、指标及恢复见[部署说明](deploy/runner/README.md)，设计与交付边界见[融合方案](docs/architecture/PRODUCTION_FUSION_PLAN.md)。

## 验证、缓存与示例

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked
cargo bench --bench indicators
cargo run --example local_data -- tests/fixtures/tdx/sz000001.day sz000001
bash scripts/dev/run_comprehensive_test.sh report
# 显式在线接口验收
bash scripts/ops/diagnose_network.sh
cargo run --release --example production_smoke
```

内存缓存默认4096条/64MiB，可显式调整；文件缓存checked入口返回I/O错误并原子写入，同键get_or_fetch合并并发获取，Client不自动缓存。指标保留本库预热/非有限值/缺口口径：RSI横盘50，KDJ为EMA平滑；布林带/KDJ常规输入O(n)，不保证第三方默认公式相同。见[开发说明](docs/development/DEVELOPMENT.md)。

本地模拟与固定夹具回归不因外部skip停止；跨平台CI仅按实际执行结果报告。fuzz/性能/历史验收的环境、规模与未覆盖项见[验证方法](docs/validation/VALIDATION.md)。测试/诊断/测量/发布工具按[脚本目录](scripts/README.md)使用，日志默认存系统临时目录，可指定RUSTDX_REPORT_DIR。

正式发包须提升版本及CLI注册表依赖，先核心再CLI；`scripts/release/publish.sh`执行门禁并请求实际发布确认。runner为独立不发布crate；本次组件仅本地交付，没有推送、crate发布或生产部署。源码快照与数据备份分别管理，见[版本交付](docs/release/RELEASES.md)。

[需求](docs/requirements/REQUIREMENTS.md) · [架构](docs/architecture/PROJECT_ANALYSIS.md) · [运营](docs/operations/OPERATIONS.md) · [运维](docs/operations/RUNBOOK.md) · [变更记录](CHANGELOG.md)

MIT许可证，见[LICENSE](LICENSE)。
