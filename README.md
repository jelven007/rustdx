# rustdx

Rust 行情库与数据导入工具。最新发布版本：jelven-rustdx-complete 1.12.2、jelven-rustdx-cli 1.8.2，最低 Rust 1.89，二进制名为 `rustdx`。本版包含复权、涨跌停舍入、指标、证券列表完整性及文件保护修复，详见[变更记录](CHANGELOG.md)。

核心库实现通达信 TCP 行情协议、本地 day/lc/gbbq 文件解析、价格复权、技术指标、交易日历、连接池和缓存。CLI 提供日线下载/解析、东方财富收盘快照、CSV/JSON 输出及数据库导入。采用同步 I/O 和有界线程并发。

## 本地构建

```sh
cargo build --workspace --locked
cargo install --path rustdx-cmd --locked
rustdx show -v
```

使用本轮源码时可配置路径依赖：

```toml
[dependencies]
rustdx-complete = { package = "jelven-rustdx-complete", path = "/path/to/rustdx" }
```

从 [crates.io](https://crates.io/crates/jelven-rustdx-complete) 安装核心库，同时保留代码中的 `rustdx_complete` 导入名：

```sh
cargo add jelven-rustdx-complete --rename rustdx-complete
```

CLI 从 crates.io 安装：`cargo install jelven-rustdx-cli --locked`（二进制名为 `rustdx`）；也可从源码安装：`cargo install --git https://github.com/jelven007/rustdx jelven-rustdx-cli --locked`。

## 行情 API

```rust,no_run
use rustdx_complete::tcp::{TcpConfig, stock::Client};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TcpConfig {
        timeout: Duration::from_secs(5),
        auto_reconnect: 2, // 首次失败之外最多重试2次
        retry_delay_ms: 200,
        recheck_empty: true,
        ..Default::default()
    };
    let mut client = Client::with_config(&config)?;
    let quotes = client.quotes(&[(1, "600000"), (0, "000001")])?;
    let bars = client.k(1, "600000", Some(20260901), Some(20260930))?;
    for row in client.k_batch(&[(1, "600000"), (0, "000001")], None, None, 2)? {
        let bars = row.result?; // 单项错误在该证券结果内
        println!("{}: {} bars", row.code, bars.len());
    }
    Ok(())
}
```

TCP 市场参数为0深圳、1上海。请显式指定市场；指数代码与股票可能相同。标准 `data::Symbol` 支持 sh/sz/bj，但标准类型支持北交所不代表普通行情协议支持该市场。

| 能力 | API |
| --- | --- |
| 实时行情、五档盘口 | `Client::quotes` |
| 股票/指数K线 | `bars`、`index_bars` |
| 自动分页区间查询 | `k`、`bars_range`、`index_bars_range` |
| 批量查询 | `k_batch`、`bars_batch` |
| 前/后复权价格 | `k_adjusted`、`Adj` |
| 当日/历史分时 | `minute`、`history_minute` |
| 当日/历史逐笔 | `transaction`、`history_transaction` |
| 财务、除权、证券列表 | `finance`、`xdxr`、`stocks` |
| F10、板块文件 | `f10`、`f10_categories`、`block` |

单连接上的请求顺序执行，批量连接池沿用客户端配置并复用连接。同一 Client 的 `k_batch` / `bars_batch` 排队执行，每个批次内部并行；调整并行数不会关闭其他在用批次的池。多个 Client 的连接数仍需调用方统一限制：建议从进程总计4–8条行情连接开始观测，包含每个 Client 的主连接和批量池。`max_parallel=0` 按CPU核心数自动，不是进程限流器；实时行情合并到1–2条长连接、每次不超过60只，是保守起点，须按出口和服务端观测调整。

协议正文截断、解压失败、非法分页进展返回错误。股票和指数使用相同请求命令，但响应结构不同，指数须使用指数解析器。`Tdx::try_parse` 是返回错误的解码入口，旧 `parse` 保留兼容；旧低层构造函数可能因非法代码 panic，优先使用经过验证的 Client 或 KlineBuilder。

概念成分使用 `Client::block("block_gn.dat")` 获取通达信源板块，并记录采集日期。旧 `get_concept_stocks/get_concept_names/get_concept_info` 的错误静态样例已移除，函数已弃用并返回无数据；不同来源的同名板块不保证一致。

行业、省份保留财务接口原始编码；无来源的静态名称表已移除，旧名称查询返回未知并弃用。名称解析需要同源、带版本字典。

空数据无法证明停牌或无交易，调用方应核实交易时间和证券状态。证券列表提前空页、重复或超出预期数量会返回错误。服务器地址、协议响应和限流策略可能变化；历史在线验收及本轮本地回归分别记录于[验收报告](docs/PRODUCTION_ACCEPTANCE.md)，目标机器仍须运行 `production_smoke`。

## 数据单位与日期

`data::Bar` 的价格/金额为元，成交量为股，时间为 UTC+8 本地时间。通达信股票 `KlineData.vol` 为手（100股）；`Bar::from_kline` 转为股，非A股需通过 `from_kline_with_unit` 显式指定源单位。`CanonicalBar` 添加周期、来源与价格口径，分钟文件适配要求显式 `Symbol`；`KlineBuilder::symbol` 可区分sh000001和sz000001。day文件内部成交量为股，现有 day/东财 CSV 的 vol 保留手的格式。

文件/东财 `factor` 是累计收益指数，原始 OHLC 保持原价；`k_adjusted` 的价格缩放乘数是另一种口径。增量输入需要正确的前因子和日期衔接，详见[迁移说明](docs/MIGRATION.md)。

`k_adjusted` 以后端可用历史首/末日锚定后/前复权，先计算完整可用历史再筛选区间；停牌期间连续事件逐次应用。后端历史可能不包含上市首日。涨跌停计算要求未复权的分单位昨收，使用整数舍入。静态接口采用2026-07-06规则（主板含ST均为10%）；历史及IPO使用 `limit::rule_at/prices_at/status_at`，规则范围及输入要求见[日期规则](docs/LIMIT_RULES.md)。

内置休市数据仅覆盖2024–2026。严格 `try_*` 日历接口在查询和前后搜索越界时返回错误。`CalendarProvider` 可替换，外部 `HolidayCalendar` 声明版本、来源及完整覆盖区间；CLI `day/east --calendar calendar.json` 使用已核实休市表，格式见[迁移说明](docs/MIGRATION.md)。未内置推测的2027休市日。旧接口保留工作日估算。交易时间包含9:30–11:30和13:00–15:00，排除午休。

## CLI

```sh
# 本地目录输出，记录来源/日期边界/质量问题
rustdx day ./vipdoc/sh/lday ./vipdoc/sz/lday -o stocks.csv \
  --report collection.json --workers 4

# 交易日缺口与日历覆盖不足也视为失败
rustdx day ./vipdoc/sh/lday -o stocks.csv --strict --report collection.json

# 官网完整包 / 指定URL / 本地ZIP
rustdx day
rustdx day https://data.tdx.com.cn/vipdoc/hsjday.zip
rustdx day ./hsjday.zip

# 筛选代码；Excel列索引从0开始
rustdx day ./lday -l sz000001,sh600000
rustdx day ./lday -l stocks.xlsx -x 0 -e sz

# 累计收益序列，gbbq为二进制文件
rustdx day ./lday -g ./gbbq -o returns.csv
rustdx day ./incremental_lday -g ./gbbq -p ./factor.csv -o incremental.csv

# 东方财富完整收盘快照；JSON输出默认eastmoney.json
rustdx east -o east.csv --report east-run.json
rustdx east -j -o east.json
rustdx east -p factor.csv -o incremental.csv

rustdx day --help
rustdx day -h output
```

非法数值、坏文件、重复或逆序日期返回非零状态；成功后原子替换输出。全空文件集合失败并保留上次输出；混合集合中的空文件仍记为警告。质量报告在文件输出及数据库导入结束后写入最终状态，解析或导入失败记录运行错误。缺口标记原因待核实，不会填补伪日线。默认最多8个解析线程，`--workers` 支持1–64，单个day文件上限32MiB。

输出、报告及保留文件会检查路径别名和输入覆盖；同一输出的并发任务立即失败。报告路径冲突在采集前拒绝，不写入失败报告。文件与数据库前因子统一验证，空集、重复代码及非法因子拒绝接续。

下载缓存位于系统临时目录，按来源隔离，文件锁持续到解析完成。`.part` 续传必须通过版本/字节边界校验，远端缓存需 ETag 或 Last-Modified 确认；无法核实远端时可显式传本地ZIP。HEAD最多30秒，单次下载最多2小时；非交互环境下载重试耗尽后直接失败。

东财输出日期来自源时间戳并确认已收盘，已成交证券必须属于当前行情日；无收盘数据的证券状态记为未知。增量前因子须来自上一交易日，漏跑时需先补历史；单日快照无法恢复跨日累计收益。`factor=0` 表示未计算，旧 `-i` 忽略校验参数已停用。

## 数据库导入

```sh
cp deploy/clickhouse/.env.example deploy/clickhouse/.env
# 修改.env中的密码后启动；端口仅绑定本机回环地址
docker compose --env-file deploy/clickhouse/.env \
  -f deploy/clickhouse/compose.yml up -d

export CLICKHOUSE_URL=http://127.0.0.1:8123
export CLICKHOUSE_USER=rustdx
export CLICKHOUSE_PASSWORD='与deploy/clickhouse/.env一致'
rustdx day ./lday -o clickhouse -t rustdx.bars_v2 -k
rustdx east -p clickhouse -o clickhouse -t rustdx.bars_v2
```

ClickHouse 统一使用HTTP配置。新表是 `ReplacingMergeTree(version)`，以(date,code)为逻辑键；查询即时去重结果须加 `FINAL`。已有表必须通过字段、精度、排序键、分区键和版本默认值检查；不兼容表需按[迁移说明](docs/MIGRATION.md)建立新表并核验后切换。HTTP请求设置 `wait_end_of_query=1`，完整读取响应并传播错误正文或截断，连接超时10秒；全量上传没有15秒总时限。本地 factor 输入只读，辅助边界查询失败不会把已成功导入报告为写入失败。数据库批次可能部分落库，失败时保留中间CSV供核验。

MongoDB 路径需要系统 `mongoimport`，以(date,code)执行upsert，日期以字符串保存；批次失败可能已写入部分行。ClickHouse 25.8 已完成2981万行本地验收；MongoDB 未做真实服务验收。

## 缓存、计算与示例

- `cache::MemoryCache::with_capacity` 限制条数/字节数；默认4096条/64MiB。
- `FileCache::try_new/try_get/try_set` 返回明确I/O错误，原子写入并校验文件键；旧缓存视为未命中。
- `Cache::get_or_fetch` 合并同键并发请求，64个固定锁碰撞时可能串行；fetch闭包不能递归调用同一缓存。
- SMA/RSI 为简单窗口口径，SMA支持补偿求和的 `RollingSma` 流式更新；横盘RSI为50，预热/非有限窗口为None。
- EMA遇非有限值输出NaN并重置；MACD在对应位置输出None，下一有效段重新起算。KDJ采用 `alpha=2/(d_period+1)` 的EMA平滑，以首个有效RSV初始化；缺口重置，不能直接等同于其他软件的默认口径。
- 缓存是可选模块，Client不会自动缓存请求。调用方的键应包含市场、代码、周期、起止范围及版本。

```sh
# 无网络本地文件例子
cargo run --example local_data -- assets/sz000001.day sz000001
# 需要通达信网络的例子
cargo run --example reliable_client
# 快速检查本项目握手和心跳，失败返回非零状态
bash scripts/diagnose_network.sh
# 部署前完整在线验收
cargo run --release --example production_smoke
```

## 验证与发布

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo +1.89.0 check --workspace --all-targets --locked
cargo bench --bench indicators
# 测试脚本：默认离线，report保存真实日志，live显式启用外部行情
bash scripts/run_comprehensive_test.sh report
```

默认CI在Linux/macOS/Windows上跳过外部网络案例，另有最低版本检查；本地TCP/HTTP模拟和文件回归正常执行。移除跳过变量会启用网络案例，需要可用行情/HTTP/数据库环境。`Tcp` 包含平台相关的 `TcpStream`，结构尺寸测试使用受控范围，协议数据结构仍保留精确快照。

两个 crate 通过 `scripts/publish.sh` 发布到 crates.io；脚本依次执行格式、测试、Clippy 和发布预检，经人工确认后发布。先发核心库 `jelven-rustdx-complete 1.12.2`，待索引可见后再发 CLI `jelven-rustdx-cli 1.8.2`（CLI 的 `rustdx-complete` 依赖同时写了注册表版本与 path）。仓库不再通过 tag 自动构建 CLI 二进制或创建 GitHub Release。发布前可用下面的联合打包在临时注册表核验：

```sh
cargo package -p jelven-rustdx-complete -p jelven-rustdx-cli --offline
```

东财公共接口会按出口和节点限流。CLI 使用 HTTPS、多节点、15秒单请求总超时和低频分页，并可通过
`RUSTDX_EASTMONEY_IPS=ip1,ip2` 覆盖备用节点；生产调度仍应限制为收盘后单任务运行。

[生产验收报告](docs/PRODUCTION_ACCEPTANCE.md) · [分阶段交付记录](docs/ITERATION.md) · [架构评估](docs/PROJECT_ANALYSIS.md) · [变更记录](CHANGELOG.md)

MIT许可证，见[LICENSE](LICENSE)。
