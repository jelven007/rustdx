# 日历、规则与证券历史维护

<!-- lifecycle:registry -->
读者：数据运营；责任：事实维护者。源码核验：2026-10-28；每30日及日历、证券事实或公告变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

更新日期：2026-10-28。数据格式验证不能证明公告内容和证券状态事实；维护者须保存原始证据并复核语义。版本化参考文件在config/calendars，核心实现位于crates/rustdx-core；当前维护数据与固定测试证据分别管理。

本轮核实上交所[年度休市页](https://www.sse.com.cn/disclosure/dealinstruc/closed/)及深交所[英文交易日历](https://www.szse.cn/English/services/trading/calendar/index.html)，分别整理为[沪市参考日历](../../config/calendars/sse-2026.json)和[深市参考日历](../../config/calendars/szse-2026.json)。工作日休市19天，交易日242天；调休周末仍休市。自动回归逐日比较两份数据与`trade_date_a/2026.0`，涵盖365天。

上述页面当前提供2026安排。未取得2027完整交易所安排，内置覆盖仍截至2026-12-31，不能仅扩展覆盖日期。

覆盖巡检入口：

```sh
rustdx --no-tcp-startup data-status --warn-days 90 --report data-coverage.json
rustdx --no-tcp-startup data-status --as-of 2026-10-24 --calendar config/calendars/sse-2026.json --strict
```

默认使用UTC+8今日，按自然日提前90天告警。`covered`表示核对日在声明范围内且离截止大于阈值；`due`表示到达提前告警窗口，含截止日；`unverified`表示超出证据范围，规则项不表示法律规则已经失效。未提供`--security`时为`missing`，提供后还逐证券检查核对日是否有状态，空表或缺口为`incomplete`；统计只涵盖文件中的证券，不声称覆盖全市场。`--strict`遇到任一告警返回非零，但仍打印并原子保存JSON，供CI/运维读取。

2026-10-24的内置日历剩68天（due），规则核实截止落后2天（unverified），证券历史未提供（missing）。当前schema7保留同口径`data_coverage`。`.github/workflows/data-coverage.yml`提供每周一及手动巡检，严格失败并保留报告，更新仍走下述人工核实流程；本地提交不代表远端任务已执行。该工作流会持续告警，直至所需证据补齐；不会自动续期。

更新过程：

1. 分别从交易所完整年度公告整理主数据与独立参考数据，保存公告原始字节、URL和核实日期。
2. 执行 `rustdx data-check calendar candidate.json --reference reference.json`。覆盖区间必须完全一致，来源须不同，任意日期分歧拒绝通过。
3. 复核公告完整性后执行同一命令加 `--output calendars/current.json`，通过后原子发布。CLI `day --calendar calendars/current.json`记录实际使用文件的摘要。检查只发布显式目标，不自动覆盖内置日历。

日期规则版本和核实截止分别为 `limit::RULE_VERSION`、`RULE_CHECKED_THROUGH`。核实日期续期必须复查沪深京全部适用规则和实施公告，再修改日期并运行板块、ST、IPO和退市例外边界回归。[上交所2026修订规则](https://www.sse.com.cn/lawandrules/sselawsrules2025/stocks/exchange/c/c_20260424_10816482.shtml)明确2026-07-06生效且当前有效；本轮不将单一交易所复查当作所有板块的续期证据，统一核实截止仍为2026-10-22。

证券状态采用 `limit::SecurityHistoryDataset`，每条记录含完整证券代码、起止日期闭区间、board、st、stage、state及announcement。announcement包含HTTPS URL、公告原始字节SHA-256、published和checked日期；数据集含version和checked_through。TradingState为Trading/Suspended/Delisted。上市序号和AnnouncedUnrestricted必须逐日声明；区间重叠、错误板块、非法阶段或超出数据核实截止拒绝载入。默认规则载入另要求日期处于统一规则覆盖内。

执行 `rustdx data-check security history.json --evidence-dir evidence/`，其中公告文件名为其小写SHA-256值；核验格式与实际公告文件摘要，可加 `--output` 原子发布。原价日线状态维护可显式加`--state-only`，走`from_state_dataset`，保留结构/公告/阶段检查而不要求规则截止覆盖；calendar拒绝该开关。`data-status`分别报告状态与规则覆盖。库的`state_at`在缺口返回None，`context_at`仅在Trading且`rule_at`核实通过时生成RuleContext；状态可用不授予涨跌幅计算资格。记录是调用方核实的事实，项目未附带推测的全市场ST/停牌历史。测试中的example.org来源仅为夹具。

新增只读计划巡检`scripts/validation/audit_point_in_time_facts.py --config job.json --calendar-reference reference.json --output new-audit.json`：按job应有证券和交易日检查点时名单、状态缺口、重叠区间、数据截止、公告原文件摘要及独立日历逐日一致。Unknown使结果incomplete并非零退出；输出只证明声明范围的完整性和字节一致，名单的全市场真实性与公告语义仍由维护者复核。它不下载公告、不续期日历、不替换配置，也不覆盖旧输出。

维护闭环依次为取得完整公告并存摘要命名原文件、整理候选日历/名单/上市退市与ST/停复牌区间、独立来源复核、执行data-check及job巡检、由新source/policy revision发布、最后固定版本回读。每次保留来源截止、核实日期、证据版本和巡检结果；新增年度公告未取得前保持OutOfRange。2027接入继续使用交易所年度公告与独立交易所日历对照，不能以周一至周五或国务院调休表代替交易安排。

责任与复核巡检使用`audit_fact_maintenance.py --registry REGISTRY --evidence-root ORIGINALS --output NEW.json`。
登记规则、年度日历、上市/退市、ST/停复牌和点时名单五类；每类指定维护角色、独立复核角色、
升级责任角色、声明范围、核实覆盖和下次复核日。证据条目包含相对路径、HTTPS来源、
published/checked与SHA-256；来源不足、摘要变化、复核到期及覆盖不足返回attention_required并退出1。
角色须由运营方落实到实际负责人，脚本不向他人发送通知，也不续期或发布事实。
仓库的[登记示例](../../config/maintenance/facts.example.json)明确未核实、无证据，不作为真实全市场材料；
现有覆盖工作流保存缺口报告，直到部署方填写并独立核对资料。它不替代data-check与应有键审计。

实际任务巡检同时传`--config JOB --calendar-reference REFERENCE`。四类消费记录的
`binding.dataset_id`须匹配任务，`binding.inputs`保存实际文件SHA：calendar绑定calendar与
calendar_reference，listing_delisting/st_suspension分别绑定security_history，universe绑定universe；
`checked_from/checked_through`须覆盖任务日期，责任和独立复核未到期。巡检同时逐键核对事实与
独立日历；`job_status`和五类总体`status`分别输出，CLI仅在两者均covered时退出0。
日调度器按四类`job_status`阻断原价日线，规则复核到期仍独立告警，统一规则截止保持2026-10-22。

2026-10-27已取得沪深京2026规则、实施说明及沪暂缓条款的7份原件，
局部范围对照及待补历史/公告在[本轮事实复查](../archive/development/FACT_SCOPE_REVIEW_20261027.md)。
该阅读未完成全适用范围的独立审核，统一截止保持2026-10-22；当日规则落后5天，
日历剩65天，证券点时事实仍由批准业务范围提供。复查原件的取得不自动续期或补齐名单。
