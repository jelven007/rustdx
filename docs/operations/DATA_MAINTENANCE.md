# 日历、规则与证券历史维护

更新日期：2026-10-23。数据格式验证不能证明公告内容和证券状态事实；维护者须保存原始证据并复核语义。

本轮核实上交所[年度休市页](https://www.sse.com.cn/disclosure/dealinstruc/closed/)及深交所[英文交易日历](https://www.szse.cn/English/services/trading/calendar/index.html)，分别整理为[沪市参考日历](../../config/calendars/sse-2026.json)和[深市参考日历](../../config/calendars/szse-2026.json)。工作日休市19天，交易日242天；调休周末仍休市。自动回归逐日比较两份数据与`trade_date_a/2026.0`，涵盖365天。

上述页面当前提供2026安排。未取得2027完整交易所安排，内置覆盖仍截至2026-12-31，不能仅扩展覆盖日期。

覆盖巡检入口：

```sh
rustdx data-status --warn-days 90 --report data-coverage.json
rustdx data-status --as-of 2026-10-23 --calendar config/calendars/sse-2026.json --strict
```

默认使用UTC+8今日，按自然日提前90天告警。`covered`表示核对日在声明范围内且离截止大于阈值；`due`表示到达提前告警窗口，含截止日；`unverified`表示超出证据范围，规则项不表示法律规则已经失效。未提供`--security`时为`missing`，提供后还逐证券检查核对日是否有状态，空表或缺口为`incomplete`；统计只涵盖文件中的证券，不声称覆盖全市场。`--strict`遇到任一告警返回非零，但仍打印并原子保存JSON，供CI/运维读取。

2026-10-23的内置日历剩69天（due），规则核实截止落后1天（unverified），证券历史未提供（missing）。采集清单schema4自动附加同口径`data_coverage`。`.github/workflows/data-coverage.yml`提供每周一及手动巡检，严格失败并保留报告，更新仍走下述人工核实流程；只提交在本地时不会启动远端定时任务。该工作流会持续告警，直至所需证据补齐；不会自动续期。

更新过程：

1. 分别从交易所完整年度公告整理主数据与独立参考数据，保存公告原始字节、URL和核实日期。
2. 执行 `rustdx data-check calendar candidate.json --reference reference.json`。覆盖区间必须完全一致，来源须不同，任意日期分歧拒绝通过。
3. 复核公告完整性后执行同一命令加 `--output calendars/current.json`，通过后原子发布。CLI `day/east --calendar calendars/current.json`记录实际使用文件的摘要。检查只发布显式目标，不自动覆盖内置日历。

日期规则版本和核实截止分别为 `limit::RULE_VERSION`、`RULE_CHECKED_THROUGH`。核实日期续期必须复查沪深京全部适用规则和实施公告，再修改日期并运行板块、ST、IPO和退市例外边界回归。[上交所2026修订规则](https://www.sse.com.cn/lawandrules/sselawsrules2025/stocks/exchange/c/c_20260424_10816482.shtml)明确2026-07-06生效且当前有效；本轮不将单一交易所复查当作所有板块的续期证据，统一核实截止仍为2026-10-22。

证券状态采用 `limit::SecurityHistoryDataset`，每条记录含完整证券代码、起止日期闭区间、board、st、stage、state及announcement。announcement包含HTTPS URL、公告原始字节SHA-256、published和checked日期；数据集含version和checked_through。TradingState为Trading/Suspended/Delisted。上市序号和AnnouncedUnrestricted必须逐日声明；区间重叠、错误板块、超出核实截止或不支持规则拒绝载入。

执行 `rustdx data-check security history.json --evidence-dir evidence/`，其中公告文件名为其小写SHA-256值；核验格式与实际公告文件摘要，可加 `--output` 原子发布。库的 `state_at` 在数据缺口返回None，`context_at`仅对Trading状态生成RuleContext；停牌、摘牌不被当作可计算的交易日。记录是调用方核实的事实，项目未附带推测的全市场ST/停牌历史。测试中的example.org来源仅为夹具。
