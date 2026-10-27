# rustdx 数据运营说明

<!-- lifecycle:registry -->
读者：数据运营；责任：数据运营维护者。源码核验：2026-10-25；每30日及业务事实、补数或成功条件变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

当前源码版本：<!-- version:project -->2.0.3<!-- /version -->。数字由主线 CI 同步；注册表历史发布事实另列。

更新日期：2026-10-25；适用当前仓库源码。运营任务是确认行情日期、业务覆盖、计算口径及下游可用版本，并在源故障或漏跑时安排补数。[需求说明](../requirements/REQUIREMENTS.md)定义产品行为，[运维手册](../runbooks/RUNBOOK.md)定义部署和恢复操作。

## 外部应用消费

应用直接调用核心库时，遵循[外部应用调用约定](../development/APPLICATION_INTEGRATION.md)：保留市场/代码身份与来源，按接口解释单位和None，检查分页完成与批量独立错误，按订阅状态消费，并共享受管连接和总体预算。具体接口、字段及上限见[26命令手册](../reference/COMMAND_API.md)，此处的数据任务清单不替代TCP结果状态检查。

只有明确标准化后的模型采用本页统一口径。原始TCP DTO、财务候选、资金流向原始单位、合成分钟标签和请求日期不得自动升级为已核实业务值；失败或不完整结果应保留原因，不能发布成完整历史或补造零数据。

## 数据任务

| 任务 | 用途与输入 | 成功后的判断 |
| --- | --- | --- |
| `day`目录/官网包 | 日线历史、全量或漏日回补；按业务需要筛选市场/名单 | 清单complete、退出0；核对日期边界、证券数、有效行及警告 |
| `day -g gbbq` | 基于历史日线计算累计收益指数 | 来源gbbq已记录；若为增量，前因子有效并衔接 |
| 数据库输出 | 将已经验证的CSV写入ClickHouse/MongoDB | 检查verified、database_verification及文件状态；核对本次选取范围 |
| runner版本发布 | 已核实日历、点时名单、证券状态、原字节及本地day或预算明确的052b日线 | coverage通过、产物封存、全部配置目标verified、账本released及目录版本已推进 |
| 固定版本消费 | 按dataset、catalog_version和范围导出文件或数据库 | 固定版本、全部选定分区验证通过，输出原子发布；不在中途追随最新版本 |

示例命令：

```sh
rustdx --no-tcp-startup day ./vipdoc/sh/lday ./vipdoc/sz/lday \
  -o stocks.csv --report day-run.json --workers 4
rustdx --no-tcp-startup day ./lday -l sz000001,sh600000 \
  -o subset.csv --report subset-run.json
rustdx --no-tcp-startup day ./incremental_lday -g ./gbbq -p factor.csv \
  -o incremental.csv --report incremental-run.json
```

数据采集任务的实际调度由部署环境执行；当日IP池自动刷新是连接管理，不是数据采集调度。独立任务使用独立工作目录，避免数据库中间文件 `clickhouse/mongodb`、factor.csv及报告路径冲突。

## 成功与质量判断

命令退出0且清单complete是流程成功；没有证明所有应有证券都在输出中。运营人员应将清单与本次选取的证券名单、预期行情日和历史基线对照。行数变化或空数据需核实证券状态、筛选参数和源响应，不能直接认定停牌。

日线默认告警涵盖日历覆盖外、缺口及允许保留的非A股源异常。`--strict`会拒绝这类警告；覆盖1990年以来的全量包时，内置2024–2026日历不足以支撑全历史严格验收。是否接纳告警取决于实际使用范围，不能通过删除告警声称数据完整。

A股文件重复/逆序日期拒绝发布；非A股告警在默认模式允许保留时，输出再排序并对同日保留最后一条，strict模式拒绝告警。跨输入的同证券日期区间重叠也拒绝。即使区间内缺口互补，也应先整理成一个可核实的输入，不能直接叠加重跑。

最新清单会更新，每次run_id归档提供追溯。运营审查保存原始归档，不修改状态或错误字段；异常终止的running和数据库unknown交给运维核实。状态定义及摘要范围见[IMPORTS.md](IMPORTS.md)。

## 统一口径

| 项目 | 使用规则 |
| --- | --- |
| 证券 | 保留sh/sz/bj与前导零；指数与股票同数字代码不混用 |
| 时间 | UTC+8行情日期；日历、源时间戳和收盘状态分别核对 |
| 价格/金额 | 元；历史涨跌停使用未复权分单位昨收 |
| 成交量 | 标准Bar为股；day CSV为手（100股）；TCP源单位按品种/周期和请求参数核实 |
| factor | 累计收益指数；0表示未计算；不用于直接缩放OHLC |
| 价格复权 | 后/前复权锚定后端可用历史首/末日，可能不是上市日 |
| 指标 | RSI横盘50；KDJ使用本库EMA系数与初始化，缺口按文档处理 |
| 概念/行业 | 通达信板块按源名及采集日期保存；行业/省份编码须配合同源版本字典 |

历史策略调用日期化涨跌幅接口，提供当日ST/上市阶段；静态接口采用2026-07-06主板规则。覆盖截至2026-10-22，不包含完整证券状态历史。详细例外见[LIMIT_RULES.md](../requirements/LIMIT_RULES.md)。

## 漏跑、源失败与版本变化

官网日线包下载失败时停止本次发布，保留原成功数据并记录失败。使用已核实的本地ZIP或后续官网包补回缺失日期；逐证券核实后再接续收益，不得自动重试unknown数据库写入。runner可选本地day或有总预算的显式TCP日线来源；补采推进source_revision，已封存恢复只回读证据。TCP单位/获取日/覆盖门禁见[应用接入](../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。

日历更新必须有交易所公告支持的完整覆盖区间，并保留版本、来源及JSON；填入2027数据前核实该区间所有休市日，前因子导航也需覆盖前一交易日。日历更新不会扩展涨跌幅规则核实范围。

此前核心/CLI 2.0.0已发布，实际包身份见[发布记录](../archive/release/PUBLICATION_20261025.md)。本次工程调整描述当前源码；后续源码版本的注册表上传独立核实。对外说明保留源码提交、源码摘要、目录版本和日历/规则版本；历史验收不能替代当前范围。发布流程见[RELEASES.md](../release/RELEASES.md)。

## 日常运营审核

1. 发布前核实planned_date、source_as_of、完整日期范围、名单和事实覆盖；假期及免行情使用已核实事实，不能由空行情推断。
2. 运行后同时检查CLI清单与runner账本，确认没有缺键、意外键、Unknown或未结束Attempt。CLI complete不自动升级为业务released。
3. 全部数据库目标verified并目录提交后，向下游登记dataset、catalog_version、release、契约和来源修订。消费者固定该版本。
4. 失败时保存原清单和候选证据，按阶段交给运维处理；unknown不得重新INSERT。弃置未发布候选后使用新release。
5. 按运营记录核实告警原因、补数范围和收益衔接。修订已发布日期时增加source_revision，source_as_of不得降低。

runner目前处理本地A股原价day或预算明确的052b日线，完整市场事实由维护流程提供。0547盘中推送、0feb日期资格、目标Linux/全量和长期SLO见[验收矩阵](../testing/PRODUCTION_ACCEPTANCE.md)，未关闭范围不能纳入“已验收服务”。

## 运营记录

每次任务保留运行标识、源码提交、选取名单、行情日、输入来源/摘要、输出行数/日期边界、质量结论和故障处置。目标库核对结果与运行清单一起保存；产物摘要不能替代源文件备份。记录保存期限由部署方确定，三个源码历史引用不限制运行归档数量。

长期留存只读预览使用：

```sh
python3 scripts/ops/retention_preview.py --root /var/lib/rustdx \
  --backup /srv/backups/rustdx-fixed --retention-days 30 \
  --output /srv/reports/retention-new.json
python3 scripts/ops/retention_preview.py --root /var/lib/rustdx \
  --backup /srv/backups/rustdx-fixed --previous /srv/reports/retention-new.json \
  --output /srv/reports/retention-next.json
```

工具复用现有controller锁，以SQLite只读事务核对所有登记catalog/release、attempt和数据库回执；包含旧版本、失败采集原码及追加事件，不因超过天数自动变成候选。显式备份库存和摘要核验后参与引用保护，未声明外部副本保持未知。软链接、摘要变化、运行/模糊worker、扫描预算耗尽或控制锁冲突都拒绝预览。输出逻辑/分配字节、目录分组、账本行数及两次采样的线性容量趋势；趋势不是生产SLO或硬配额。仅过期且无登记引用的产物列为`candidate_review`，本工具没有删除或账本裁剪入口。要改变历史保留承诺，须另行设计版本退役与恢复验证。

## 生产运营交接与异常分流

| 时点 | 运营者确认 | 交付或升级条件 |
| --- | --- | --- |
| 发布前 | planned_date、source_as_of、完整名单、日历/状态及输入摘要 | Unknown或缺事实先补材料，不从空页猜停牌 |
| 发布后 | 应有键/全字段、全部DB目标、released和catalog_version | 固定dataset/contract/release与来源修订后登记下游 |
| 数据异常 | 与预期日/名单/源字段比对，保存原清单和报告 | 业务事实错误修订source_revision；技术恢复交运维 |
| 不确定写入 | query_id/请求证据、在途情况和较新数据 | 运维核查，不自动重写；新任务使用新release |
| 版本升级 | 经过CI核验的源码/二进制、迁移说明和操作记录 | 软件升级、旧数据消费和数据库恢复分别审核 |

每日Job之后复核业务日期和异常；每次补数、源/事实/契约改变重新审核覆盖。长期SLO、业务漏日阈值、实际保留期和人员值守由目标部署方确定，模板默认值不作为已批准指标。指标名称在OBSERVABILITY定义，恢复动作在RUNBOOK定义，消费合同在APPLICATION_INTEGRATION定义。

运营交接记录最少包含：任务/Attempt、源码与配置修订、事实/源摘要、覆盖范围、质量结论、DB验证、目录版本、未关闭风险和下一次核验责任。源码history只保留最近三个引用，不能据此删除数据版本、源证据、账本或备份。
