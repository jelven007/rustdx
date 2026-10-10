# 数据语义与迁移

标准接口 `data::Symbol` 显式携带 sh/sz/bj 市场，保留前导零与字母代码。`data::Bar` 的价格和成交额单位为元、成交量为股、时间为 UTC+8 本地时间。通达信股票 KlineData.vol 为手，标准适配器转换成股；指数及非股票品种须核实源单位。现有 day/东财 CSV 的 vol 仍为手（100股），保持文件兼容。

`factor` 是从基期开始的累计收益指数，不是前后复权价格乘数。原始 OHLC 未被改写。增量必须提供上一条有效 factor/收盘价，并只应用其日期之后的除权事件；停牌期间的多个事件按日期依次调整理论前收。首次导入以首条记录为基期。

内置交易日历仅有2024–2026休市数据。旧 `is_trading_day` 在范围外保留工作日估算；新 `try_is_trading_day` 和 `trading_days_between` 返回明确错误。质量报告对范围外记录标注未核实。时间判断排除11:30之后至13:00之前的午休，包含11:30和15:00端点。

## CLI 质量记录

```sh
rustdx day ./vipdoc/sh/lday ./vipdoc/sz/lday -o stocks.csv --report collection.json
rustdx day ./vipdoc/sh/lday -o stocks.csv --report collection.json --strict
```

报告记录版本、时间、来源文件、代码、行数、日期边界和质量问题。非法数值、重复/逆序日期、文件失败都会返回非零状态，输出通过临时文件原子替换，上次成功结果不会因本次失败被覆盖。缺少交易日不能直接认定为停牌，默认记录警告；`--strict` 将缺口和日历覆盖不足也视为失败。

## ClickHouse

新表使用 `ReplacingMergeTree(version)`，逻辑键是 `(date, code)`，`version UInt64 DEFAULT toUnixTimestamp64Nano(now64(9))` 为每次导入生成版本。重复导入会在合并时替换旧行，读取立即一致的结果须使用 `FINAL`。历史 CSV 仍可通过 `input_format_defaults_for_omitted_fields=1` 补足 version；未提供 preclose/factor 时默认为0（表示未计算）。

旧 `ReplacingMergeTree()` 表不会被自动改写。先备份，创建新表（例如 `rustdx.bars_v2`，CLI 指定 `-t rustdx.bars_v2` 会建表），再按列迁移：

```sql
INSERT INTO rustdx.bars_v2
    (date, code, open, high, low, close, amount, vol, preclose, factor)
SELECT date, code, open, high, low, close, amount, vol, preclose, factor
FROM rustdx.old_bars FINAL;
```

无 factor 的旧表去掉相应两列。迁移后核对 `count()`、代码数量、`min(date)`、`max(date)` 以及抽样值，再切换调用方。版本语义为最后导入优先，回灌前须确认源数据质量。禁止通过直接重跑掩盖未知导入状态。
