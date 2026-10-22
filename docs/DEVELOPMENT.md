# rustdx 开发说明

更新日期：2026-10-23；本轮分步结果见[NEXT_OPTIMIZATION.md](NEXT_OPTIMIZATION.md)。需求与变更追踪见[需求说明](REQUIREMENTS.md)、[文档索引](README.md)，模块关系见[架构评估](PROJECT_ANALYSIS.md)。

## 构建与验证

使用Rust1.89及以上，edition2024。核心crate为 `jelven-rustdx-complete`，导入名 `rustdx_complete`；CLI crate为 `jelven-rustdx-cli`，库名 `rustdx_cli`，二进制名 `rustdx`。保持阻塞I/O和有界线程，fuzz是独立workspace。

```sh
cargo build --workspace --all-targets --all-features --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked
cargo install --path rustdx-cmd --locked
cargo run --example local_data -- assets/sz000001.day sz000001
```

外部案例由环境变量跳过，本地模拟服务仍执行。本机验证环境通过忽略的 `.cargo` 安装工具链；常规开发使用已有Cargo/Rust即可，不依赖该本机目录。完整验证口径、fuzz及性能复现见[VALIDATION.md](VALIDATION.md)。

## 源码导航

| 模块/文件 | 职责 |
| --- | --- |
| `src/tcp/mod.rs`、`reader.rs` | 受管socket、握手、完整帧、重试及有界解码 |
| `src/tcp/governance.rs` | 进程配额、Deadline、端点轮换/冷却及网络统计 |
| `src/pool.rs` | 连接借还、闲置连接和关闭；与TCP共享配额 |
| `src/tcp/stock/client.rs` | 高层行情API，组装请求和结果 |
| `src/tcp/stock/kline_request.rs`、`tests/kline_decoders.rs` | 私有参数的不可变请求、股票/指数独立checked解码、固定pytdx参考样本 |
| `client/batch.rs`、`pagination.rs`、`adjustment.rs` | 批次调度、通用分页、价格复权 |
| `src/data.rs`、`data/model.rs` | 市场/代码、单位/时间、CanonicalBar及口径类型 |
| `src/calendar.rs`、`calendar/provider.rs` | 兼容日历、严格Provider及外部数据集 |
| `src/limit.rs`、`limit/rules.rs` | 静态兼容接口、按日期选择规则和价格舍入 |
| `src/file/day`、`file/lc`、`file/gbbq` | 文件解析、累计收益、checked入口和gbbq解密 |
| `src/indicators.rs`、`cache.rs`、`atomic_file.rs` | 指标、可选缓存及同目录原子发布 |
| `rustdx-cmd/src/cmd/day.rs`、`cmd/east.rs` | 参数、路径保护、来源获取和任务入口 |
| `rustdx-cmd/src/io.rs`、`io/sinks.rs`、`io/schema.rs` | 有界解析管道、前因子、数据库写入/清理、ClickHouse表校验 |
| `rustdx-cmd/src/io/streaming.rs`、`tests/day_streaming.rs` | 4096条分块、私有磁盘快照、日期归并、确定性及取消回归 |
| `rustdx-cmd/src/quality.rs`、`paths.rs` | 质量报告、来源/输出证据、清单检查点及文件锁 |
| `rustdx-cmd/src/download.rs`、`fetch_code.rs`、`eastmoney/mod.rs` | ZIP续传/缓存、交易所名单及快照分页 |
| `tests/network_budget.rs`、`properties.rs`、`reliability.rs` | 预算/时限、属性及协议可靠性回归 |
| `rustdx-cmd/tests/cli.rs`、`fuzz/fuzz_targets/checked_parsers.rs` | CLI端到端及不可信解析fuzz |

## 接口与实现约束

### 行情与网络

优先使用Client或KlineBuilder::build_request；TCP市场0为深圳、1为上海。KlineRequest私有字段保证编码与参数一致，返回股票/指数不同类型的独立解码函数拒绝截断、尾随字节、超数量和非法日期/时间。Client单页、分页、批次及复权拉取使用新路径，避免请求对象预填结果及响应/结果复制。旧build返回Kline，旧Kline/IndexKline字段和parse保留，try_parse共用新解码；旧公开字段仍可能与发送字节不一致，直接构造仍可能panic。标准Symbol支持北交所不代表普通TCP协议支持北交所，禁止自动映射成深圳。

`configure_network`只允许首次初始化前调用；读取policy/stats也会初始化全局策略。主连接占用配额，批量查询需要额外容量。重连在旧许可内先关闭旧socket；批次结束释放批量池。不得用裸socket绕过预算后声称仍受进程上限保护。参数默认值、同步闭包不可抢占的限制见[NETWORK.md](NETWORK.md)。

### 行情语义与规则

`CanonicalBar::from_kline`显式要求市场、周期、成交量单位和价格口径；`from_day`为原始日线；`from_lc`要求Symbol且只支持1/5分钟。BarSource覆盖通达信TCP/day/lc及Eastmoney；CLI的Day::to_canonical_with_calendar返回EastCanonicalBar，含标准原价日线、源UTC+8时间戳、源手单位、前收和可选收益指数。标准日线时点15:00与实际快照时间分开，市场、日期、已收盘和完整字段独立检查；采集任务另检查当前行情日。既有文件/TCP类型和CSV保持兼容，枚举消费者需处理新增Eastmoney分支。

标准量为股，day/lc文件源量为股，TCP A股日线与CSV量为手。TCP分钟周期、指数和其他品种须核实源单位，不能仅凭股票代码推断；旧Bar::from_kline只兼容A股日线手→股，其余用显式单位入口。`ReturnIndex`与`PriceMultiplier`要求正有限值；CSV `factor=0`适配为None。东财advance_return_index只接受ReturnIndex，CLI校验上一交易日基准后更新旧CSV factor，原始OHLC不改写。

历史回测使用 `RuleContext`及 `rule_at/prices_at/status_at`，输入当日状态和上市交易日序号。日历覆盖与规则覆盖分别检查；填入新日历不自动扩展规则核实范围。静态规则、IPO例外和来源见[LIMIT_RULES.md](LIMIT_RULES.md)。

### 文件、证据与数据库

新不可信输入使用day/lc的 `try_from_bytes`、gbbq的 `Gbbq::try_from_chunk`及读取器 `try_to_vec`、协议 `Tdx::try_parse`。旧单条入口保留兼容，部分低层构造仍可能panic；checked边界不能因新增解析路径而绕过。

读取与摘要必须基于同一字节快照。单day32MiB、gbbq/前因子64MiB、日历1MiB；名单HTTP响应16MiB。day线程默认最多8、可选1–64，每线程结果通道容量1，只传磁盘CSV和元数据。day以4096条分块，质量检查及复权因子跨块连续；A股重复/逆序拒绝，非A股最多8路磁盘归并排序去重；同证券跨输入区间重叠拒绝发布。输出固定为目录参数顺序、文件名升序、文件内日期升序；切换线程数不改变CSV字节。质量详情每类64条，省略计数不改变校验结果。临时空间及元数据边界见[IMPORTS.md](IMPORTS.md)。

`CollectionReport::start`在采集前持久化，`stage_artifact/published/begin_database/finish`保存关键检查点。每次先更新run_id归档，再更新最新清单，两文件不是一个事务。CSV先回读再记录摘要并原子提交；东财JSON直接序列化已经验证的行，不执行CSV回读。

ClickHouse通过HTTP完整响应确认，INSERT附带query_id，单次操作300秒；版本由表默认服务端纳秒生成。MongoDB通过mongosh建立/核实唯一索引，mongoimport退出码确认，mongoexport独立回读，MONGODB_URI配置连接。子进程上限300秒并受整任务deadline约束，超时终止并回收。回执后每批512键精确比较全部导出字段，verified才允许清理CSV；同机同目标锁覆盖前基准读取至验收完成。不要将unknown自动改成not_started，也不要删除待核对CSV。字段和恢复语义以[IMPORTS.md](IMPORTS.md)为准。

### 指标与缓存

布林带采用双栈中心化矩聚合，每条数据入栈/转栈/出栈各一次；KDJ使用高低价单调队列。常规输入O(n)，窗口状态O(period)，输出O(n)。修改后对照独立公式和缺口/极端数值回归，不用逐位浮点相等验证方差。SMA极端累加溢出仍可能重算窗口。

KDJ采用 `alpha=2/(d_period+1)`、首个有效RSV初始化K/D；非有限缺口重置。缓存不由Client自动启用，缓存键应含市场、证券、周期、区间和版本；get_or_fetch闭包不能递归调用同一缓存。

## 修改与交付

新增行为同步更新需求、方案、开发、验证和受影响运营/运维文档；不在多个文件重复完整专项定义。新增日期规则须补原始依据和边界用例，新增数据库写入须记录不确定阶段与恢复证据。

本轮Cargo版本未提升，源码推送不等于crates.io发布；CLI path依赖在源码构建时读取本地优化核心库。分支合并、版本保留与实际发布门禁见[RELEASES.md](RELEASES.md)。
