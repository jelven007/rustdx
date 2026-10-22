# 数据语义与迁移

## 仓库源码优化（2026-10-22，尚未发布新crate版本）

源码提交7cd02e7、5fb5b70、401265c、1f9f739已进入main。核心库/CLI的Cargo版本保持1.12.2/1.8.2，crates.io上的同版本仍是10月11日发布内容；使用本节新行为须从源码构建。完整文档见[索引](README.md)，分支交付与两个历史快照见[RELEASES.md](RELEASES.md)。

`day/east` 默认写运行清单和run_id归档，`--report`可更改路径；schema_version升级为2，增加摘要、输出验证和数据库确认状态。数据库HTTP及mongoimport现在有300秒上限，失败保留CSV；详见[IMPORTS.md](IMPORTS.md)。文件构造器拒绝非法日期及数值，新 `try_from_bytes/try_from_chunk/try_parse/try_to_vec` 面向不可信数据；旧单条解析入口继续保留。

布林带使用双栈聚合的中心化总体方差，KDJ使用单调队列；正常序列总耗时O(n)，预热及缺口恢复口径不变。方差舍入误差可能带来末位变化，避免通过逐位浮点相等评判布林带回归。

历史涨跌幅现在由 `RuleContext` 的日期、板块、当日ST状态及上市阶段决定。`rule_at/prices_at/status_at` 能区分无幅度限制和非法输入；详细范围、IPO及例外见 [LIMIT_RULES.md](LIMIT_RULES.md)。静态涨跌停接口仍保留，但主板ST幅度按2026-07-06生效的规则改为10%；历史5%必须使用日期接口。

`CalendarProvider` 返回版本、来源和覆盖范围。严格前后交易日、最近N日和市场交易日接口在搜索越界时返回错误。`day/east --calendar calendar.json` 接受完整休市日列表（周末自动休市），报告记录使用的日历。输入格式为：

```json
{
  "version": "sse-2025-new-year",
  "source": "https://www.sse.com.cn/",
  "start": "2025-01-01",
  "end": "2025-01-06",
  "holidays": ["2025-01-01"]
}
```

示例只覆盖明确的局部区间。版本及来源不能为空；休市日期不能重复或越界。填入2027等新年份前，应从交易所公告取得**该覆盖区间所有休市日期**，不能通过扩大start/end或仅判断星期一至五来声称完整覆盖。库校验格式，数据真实性与完整性由提供者负责。

`CanonicalBar` 附带 `Period`、`BarSource` 和 `PriceBasis`；lc源记录仍保留旧类型，标准适配器要求提供带市场的Symbol。`PriceMultiplier` 与 `ReturnIndex` 是两个独立类型，防止收益指数误用为价格乘数。`Bar::from_kline` 只隐式转换A股股票的手单位，非A股用 `from_kline_with_unit` 显式传入单位。北交所不会隐式映射到深圳TCP市场。`KlineBuilder::symbol` 是带市场的构建入口，旧code自动推断行为保留。

交易所股票名单请求使用HTTPS、总时限和16MiB响应限制；上交所不再发送历史硬编码会话Cookie。接口鉴权或反爬变化会显式返回错误。`docsrs` feature可在稳定Rust下开启；自动文档标记由docs.rs的 `cfg(docsrs)` 单独启用。

所有受管Tcp共享默认8条进程预算（含主连接、建连及空闲池），首次使用或读取policy/stats前通过 `configure_network` 配置一次。批次完成释放连接；批次有界排队，全局配额不足时有界等待。连接、完整帧、分页及批次增加总体时限；服务器自动选择使用轮换起点和故障冷却。原先可能无限等待的调用现在会提前返回超时/WouldBlock；默认值、统计及裸socket兼容边界见[NETWORK.md](NETWORK.md)。day整条命令仍没有统一总deadline。

同一证券多个day来源的日期区间重叠现在拒绝发布，即使区间内实际缺口互补；需先整理源文件。CSV发布前回读并检查行数、来源区间和日期边界，东财JSON由验证后的记录直接序列化。运行清单schema2消费者需兼容新增字段和中间状态；project_version仍是Cargo版本，应另存Git提交区分本轮源码。run_version为客户端运行标识，不替代ClickHouse服务端行version。

## jelven-rustdx-complete 1.12.2 / jelven-rustdx-cli 1.8.2（2026-10-11，已发布 crates.io）

以下为相对1.12.1 / 1.8.1的行为变化；核心库导入名仍为 `rustdx_complete`，CLI二进制名仍为 `rustdx`。

`k_adjusted` 的后复权锚点是服务器可用历史的第一根日K，不保证是上市首日；首日及更早事件已体现在首日价格中，不再重复应用。停牌期间多次事件以前次参考价继续计算。前复权仍锚定可用历史末日，区间过滤不改变锚点。非法事件、价格及乘数返回InvalidData；旧 `ex_right_reference` 单独调用仍保留非法输入返回原昨收的兼容行为。

涨跌停报价以分为单位，用整数百分比四舍五入；例如主板昨收1.15元，涨停价为1.27元。非法、非分单位或超出精确计算范围的昨收返回None。主板代码仅匹配600/601/603/605和000/001/002/003前缀；按代码推断仍不能区分同码的指数与股票。历史制度和新股特殊阶段仍需调用方提供规则。

证券列表遇到未达到总数的空页、重复证券或超出总数时返回错误，不交付部分列表。`market_of` 要求六位数字。重连握手失败消耗一次尝试，剩余尝试仍会执行；同时使用显式 `retry` 和自动重试会形成嵌套预算，不应将其当作任务总deadline。

旧 `get_industry_name/get_province_name/get_industry_info` 已弃用，分别返回“未知行业”“未知省份”及原编码/未知行业/未分类；原静态表无可靠来源，不能继续用于选股或归因。使用财务接口的原始 `industry/province` 编码并配合同源、带版本字典；通达信概念板块文件不替代财务分类字典。

`day/east` 在执行采集前检查所有输出与输入路径。相对/绝对路径、`..`、符号链接别名也会判定冲突；报告不能覆盖day、gbbq、前因子或名单文件，也不能与输出、保留CSV、factor.csv重名。输出锁覆盖整个命令，冲突任务立即失败；下载来源持有缓存锁直到解析结束。系统临时目录中的锁文件保留以避免锁竞争，文件存在不代表仍被占用。

从ClickHouse读取的前因子现在与CSV一样验证日期、代码、正有限因子/收盘价、重复代码及空集；不再接受未计算的factor=0。`--keep-factor` 的factor.csv只在数据校验通过后原子替换。HEAD总时限30秒；每次ZIP下载建连15秒、响应头30秒、总时限2小时，超时可通过原有续传恢复。

EMA周期0返回NaN，非有限输入返回NaN并重置，下一有效价格重新初始化。MACD非法周期返回全None，缺口处三条线均为None并保留原序列位置。布林带周期小于2或倍数非法时全None。SMA使用补偿求和，窗口总和溢出时返回None并在后续窗口恢复。KDJ要求窗口内每根收盘位于自身高低区间；保留 `alpha=2/(d_period+1)`、首个有效RSV初始化K/D的口径，不能直接等同于alpha=1/3、初值50的其他实现。

## jelven-rustdx-complete 1.12.1 / jelven-rustdx-cli 1.8.1

均已发布到 crates.io。核心库安装 `cargo add jelven-rustdx-complete --rename rustdx-complete`；CLI 安装 `cargo install jelven-rustdx-cli --locked`（二进制 `rustdx`）。CLI 更名因 crates.io 上 `rustdx-cli` 归属他人，Rust 导入名仍为 `rustdx_cli`。

该已发布版本加入同一Client批量调用有界排队、批次内并行。进程总配额、端点冷却及连接/帧/分页/批次总体时限是2026-10-22源码新增行为，见本文首节；不将其视为已发布1.12.1/1.8.1的能力。

旧静态概念查询已弃用：`get_concept_stocks/get_concept_info` 返回None，`get_concept_names` 返回空列表，表示本库不再提供未核实样例。迁移到 `Client::block("block_gn.dat")`，按源板块名筛选并保存采集时间。该接口返回代码而非证券名称，名称可从证券列表匹配；不同来源板块不能直接等同。

`Symbol` 继续支持六位字母数字代码，但字母代码不再判为A股。`limit_status` 对非有限、非正或high低于close的行情返回None。`k_adjusted` 的非法日期区间提前返回InvalidInput。

标准接口 `data::Symbol` 显式携带 sh/sz/bj 市场，保留前导零与字母代码。`data::Bar` 的价格和成交额单位为元、成交量为股、时间为 UTC+8 本地时间。通达信股票 KlineData.vol 为手，标准适配器转换成股；指数及非股票品种须核实源单位。现有 day/东财 CSV 的 vol 仍为手（100股），保持文件兼容。

`factor` 是从基期开始的累计收益指数，不是前后复权价格乘数。原始 OHLC 未被改写。增量必须提供上一条有效 factor/收盘价，并只应用其日期之后的除权事件；停牌期间的多个事件按日期依次调整理论前收。首次导入以首条记录为基期。

内置交易日历仅有2024–2026休市数据。旧 `is_trading_day` 在范围外保留工作日估算；新 `try_is_trading_day` 和 `trading_days_between` 返回明确错误。质量报告对范围外记录标注未核实。时间判断排除11:30之后至13:00之前的午休，包含11:30和15:00端点。

## CLI 质量记录

```sh
rustdx day ./vipdoc/sh/lday ./vipdoc/sz/lday -o stocks.csv --report collection.json
rustdx day ./vipdoc/sh/lday -o stocks.csv --report collection.json --strict
```

报告记录版本、时间、来源文件、代码、行数、日期边界和质量问题。非法数值、重复/逆序日期、文件失败都会返回非零状态，输出通过临时文件原子替换，上次成功结果不会因本次失败被覆盖。缺少交易日不能直接认定为停牌，默认记录警告；`--strict` 将缺口和日历覆盖不足也视为失败。

全空day集合现在失败并保留旧输出；混合数据中的空文件默认只告警。报告在输出提交及数据库导入结束后确定complete/failed，导入失败也记录顶层errors。解析前因子、gbbq、目录或输出失败同样记录报告；输入参数或下载源解析失败发生在采集阶段之前。数据库可能已写入部分行，中间CSV保留供核验，不能把failed视为数据库事务回滚。

`east -j` 现在写入JSON数组，默认文件为eastmoney.json；不能同时使用 `-o clickhouse`。`east --report` 记录未输出的无收盘证券（状态未知）及运行错误。快照必须属于当前行情日，`-p` 前因子必须是上一交易日；跨日漏跑应先回补历史，禁止用单日快照直接计算跨日收益。旧 `-i` 绕过校验参数已停用。

gbbq输入是通达信二进制文件。`Fq::new/concat` 现在仅返回交易日行，停牌事件不额外生成伪交易日；`xdxr` 标记在后续实际交易日是否应用过事件。SMA支持周期1，RSI横盘为50，KDJ缺数据不再填0。文件缓存使用新格式和稳定文件键，旧缓存不会复用。

## ClickHouse

新表使用 `ReplacingMergeTree(version)`，逻辑键是 `(date, code)`，`version UInt64 DEFAULT toUnixTimestamp64Nano(now64(9))` 为每次导入生成版本。重复导入会在合并时替换旧行，读取立即一致的结果须使用 `FINAL`。历史 CSV 仍可通过 `input_format_defaults_for_omitted_fields=1` 补足 version；未提供 preclose/factor 时默认为0（表示未计算）。

CLI1.8.1同时检查已有表：date Date、code String、OHLC/amount/vol/preclose/factor Float64、version UInt64；preclose/factor须有DEFAULT，version使用上述纳秒表达式；排序键必须严格为date,code。允许不分区或按date/toYYYYMM(date)/toYear(date)分区，其他分区表达式需另行核实后迁移，避免重复键跨分区。引擎或键即使包含version也不能绕过检查。

HTTP导入及SQL启用wait_end_of_query=1并读完整响应，HTTP错误、正文异常和截断都返回失败；10秒限制用于建连，全量导入不使用东财15秒总时限。本轮源码为单次HTTP操作另加300秒总体上限和INSERT query_id。辅助边界查询失败仍仅告警。

旧 `ReplacingMergeTree()` 表不会被自动改写。先备份，创建新表（例如 `rustdx.bars_v2`，CLI 指定 `-t rustdx.bars_v2` 会建表），再按列迁移：

```sql
INSERT INTO rustdx.bars_v2
    (date, code, open, high, low, close, amount, vol, preclose, factor)
SELECT date, code, open, high, low, close, amount, vol, preclose, factor
FROM rustdx.old_bars FINAL;
```

无 factor 的旧表去掉相应两列。迁移后核对 `count()`、代码数量、`min(date)`、`max(date)` 以及抽样值，再切换调用方。版本语义为最后导入优先，回灌前须确认源数据质量。禁止通过直接重跑掩盖未知导入状态。
