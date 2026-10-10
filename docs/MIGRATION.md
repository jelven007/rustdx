# 数据语义与迁移

## 源码1.12.1 / CLI1.8.1

crates.io核心库仍为1.12.0。本轮改动需使用workspace路径或Git源码；未执行新注册表发布。

同一 Client 的批量调用现在排队，单个批次内部仍并行，可能增加重叠调用的等待时间；避免30个任务分别创建Client并开独立池。每个Client含1条主连接及最多max_parallel条批量连接，进程总预算应由上层统一管理，建议从4–8条开始观测。连接池 `get_connection` 仍在饱和时返回WouldBlock；需要有界等待时使用 `get_connection_timeout`，其等待时限不包含建连。

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

HTTP导入及SQL启用wait_end_of_query=1并读完整响应，HTTP错误、正文异常和截断都返回失败；10秒限制仅用于建连，全量导入不使用东财15秒总时限。辅助边界查询失败仍仅告警。

旧 `ReplacingMergeTree()` 表不会被自动改写。先备份，创建新表（例如 `rustdx.bars_v2`，CLI 指定 `-t rustdx.bars_v2` 会建表），再按列迁移：

```sql
INSERT INTO rustdx.bars_v2
    (date, code, open, high, low, close, amount, vol, preclose, factor)
SELECT date, code, open, high, low, close, amount, vol, preclose, factor
FROM rustdx.old_bars FINAL;
```

无 factor 的旧表去掉相应两列。迁移后核对 `count()`、代码数量、`min(date)`、`max(date)` 以及抽样值，再切换调用方。版本语义为最后导入优先，回灌前须确认源数据质量。禁止通过直接重跑掩盖未知导入状态。
