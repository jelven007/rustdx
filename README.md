# rustdx

Rust 行情库与数据导入工具。当前源码版本：rustdx-complete 1.12.0、rustdx-cli 1.8.0，最低 Rust 1.89。

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
rustdx-complete = { path = "/path/to/rustdx" }
```

仓库源码版本不表示已发布到 crates.io。已有发布包可通过 `cargo add rustdx-complete` 或 `cargo install rustdx-cli` 安装，使用前核对版本。

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

单连接上的请求顺序执行，批量连接池沿用客户端配置并复用连接。协议正文截断、解压失败、非法分页进展返回错误。`Tdx::try_parse` 是返回错误的解码入口，旧 `parse` 保留兼容；旧低层构造函数可能因非法代码 panic，优先使用经过验证的 Client 或 KlineBuilder。

空数据无法证明停牌或无交易，调用方应核实交易时间和证券状态。服务器地址、协议响应和限流策略可能变化；本轮验证使用本地模拟服务与存量抓包，未访问真实行情服务器。

## 数据单位与日期

`data::Bar` 的价格/金额为元，成交量为股，时间为 UTC+8 本地时间。通达信股票 `KlineData.vol` 为手（100股）；`Bar::from_kline` 转为股，指数及其他品种需另行确认单位。day文件内部成交量为股，现有 day/东财 CSV 的 vol 保留手的格式。

文件/东财 `factor` 是累计收益指数，原始 OHLC 保持原价；`k_adjusted` 的价格缩放乘数是另一种口径。增量输入需要正确的前因子和日期衔接，详见[迁移说明](docs/MIGRATION.md)。

内置休市数据仅覆盖2024–2026。严格日历接口 `try_is_trading_day`、`trading_days_between` 对范围外日期返回错误。旧接口保留工作日估算。交易时间包含9:30–11:30和13:00–15:00，排除午休。

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

非法数值、坏文件、重复或逆序日期返回非零状态；成功后原子替换输出。缺口标记原因待核实，不会填补伪日线。默认最多8个解析线程，`--workers` 支持1–64，单个day文件上限32MiB。

下载缓存位于系统临时目录，按来源隔离并用文件锁保护。`.part` 续传必须通过版本/字节边界校验，远端缓存需 ETag 或 Last-Modified 确认；无法核实远端时可显式传本地ZIP。非交互环境下载重试耗尽后直接失败。

东财输出日期来自源时间戳并确认已收盘，已成交证券必须属于当前行情日；无收盘数据的证券状态记为未知。增量前因子须来自上一交易日，漏跑时需先补历史；单日快照无法恢复跨日累计收益。`factor=0` 表示未计算，旧 `-i` 忽略校验参数已停用。

## 数据库导入

```sh
export CLICKHOUSE_URL=http://127.0.0.1:8123
export CLICKHOUSE_USER=default
# 如需认证，在环境变量中设置CLICKHOUSE_PASSWORD
rustdx day ./lday -o clickhouse -t rustdx.bars_v2 -k
rustdx east -p clickhouse -o clickhouse -t rustdx.bars_v2
```

ClickHouse 统一使用HTTP配置。新表是 `ReplacingMergeTree(version)`，以(date,code)为逻辑键；查询即时去重结果须加 `FINAL`。旧表需按[迁移说明](docs/MIGRATION.md)建立新表并核验后切换。本地 factor 输入只读，辅助边界查询失败不会把已成功导入报告为写入失败。

MongoDB 路径需要系统 `mongoimport`，以(date,code)执行upsert，日期以字符串保存；批次失败可能已写入部分行。两种外部数据库本轮均未做真实服务验收。

## 缓存、计算与示例

- `cache::MemoryCache::with_capacity` 限制条数/字节数；默认4096条/64MiB。
- `FileCache::try_new/try_get/try_set` 返回明确I/O错误，原子写入并校验文件键；旧缓存视为未命中。
- `Cache::get_or_fetch` 合并同键并发请求，64个固定锁碰撞时可能串行；fetch闭包不能递归调用同一缓存。
- SMA/RSI 为简单窗口口径，SMA支持 `RollingSma` 流式更新；横盘RSI为50，预热/非有限窗口为None。KDJ采用EMA平滑，缺口重置状态。
- 缓存是可选模块，Client不会自动缓存请求。调用方的键应包含市场、代码、周期、起止范围及版本。

```sh
# 无网络本地文件例子
cargo run --example local_data -- assets/sz000001.day sz000001
# 需要通达信网络的例子
cargo run --example reliable_client
```

## 验证与发布

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo +1.89.0 check --workspace --all-targets --locked
cargo bench --bench indicators
```

默认CI在Linux/macOS/Windows上跳过外部网络案例，另有最低版本检查；本地TCP/HTTP模拟和文件回归正常执行。移除跳过变量会启用网络案例，需要可用行情/HTTP/数据库环境。tag `v*` 构建三平台CLI并生成带SHA256的release草稿，仍需人工发布。

[分阶段交付记录](docs/ITERATION.md) · [架构评估](docs/PROJECT_ANALYSIS.md) · [变更记录](CHANGELOG.md)

MIT许可证，见[LICENSE](LICENSE)。
