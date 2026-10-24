# rustdx 生产技术方案

版本：3.0；2026-10-24。方案对应当前单机实现，按[需求](../requirements/REQUIREMENTS.md)追踪。拟扩展能力集中在第8节，不将设计目标表述为已验收结果。

## 1. 系统职责与工程组织

| 层 | 当前职责 | 依赖与交付 |
| --- | --- | --- |
| 核心库 | TCP治理、26命令、文件解析、语义适配、规则/指标/缓存 | crates/rustdx-core；保持rustdx_complete公开路径 |
| 采集应用 | 来源准备、分块处理、质量/清单、CSV与兼容数据库导入 | apps/rustdx-cli；rustdx二进制 |
| 生产控制层 | 严格Job、账本、覆盖、封存、版本发布/消费、恢复 | apps/rustdx-runner；SQLite WAL与本地持久根目录 |
| 部署环境 | 事实维护、调度、凭据、数据库预建、配额、监控与备份 | config/deploy；外部systemd等触发单次Job |

核心不承担Job账本或数据库发布事务；控制层复用采集CLI，在私有目录中执行。保留同步I/O、有界线程及子进程模型。

## 2. 业务契约与事实

当前runner固定契约：A股、日线、原价、人民币元、整数股。逻辑键保留日期、市场、代码、品种及契约维度。day兼容CSV量为手；控制层核实后转为股，TCP日线schema8已为股。原始OHLC不通过收益因子缩放。

Job显式指定dataset/contract、policy_revision、source_revision、planned_date、source_as_of、日期范围、CLI身份、输入事实和资源预算。相同身份重复执行去重；相同身份配置不同拒绝，改变业务修订登记新Job。

应有键由冻结的版本化日历、点时名单与证券区间产生：Trading必须有行情；已核实停牌/退市等保存免行情原因；Unknown、缺键、意外键拒绝发布。公告摘要只证明原字节一致性，维护者仍负责公告语义和全市场完整性。

全部应有键均已明确免行情时，允许空分区并记录collector_executed=false。行情空页不能用于生成停牌事实。日历、规则截止与证券事实覆盖分别审核。

## 3. 采集与质量

```mermaid
flowchart LR
    Job["校验Job / 登记Attempt"] --> Freeze["冻结源、事实、CLI"]
    Freeze --> Worker["私有worker / 有界day或TCP日线采集"]
    Worker --> Verify["独立重放 / 全部应有键"]
    Verify --> Seal["rename + fsync / 封存release"]
    Seal --> DB["可选目标写入 / 完整验证"]
    DB --> Catalog["SQLite事务 / 目录CAS"]
    Catalog --> Reader["消费者固定catalog版本"]
```

worker使用剩余deadline，并为封存/发布预留预算。采集正常取消后有宽限期；超时终止独立进程组。源文件、记录数、工作空间及空闲磁盘受配置限制；周期扫描可能遇到暂存文件消失，封存时再次计量。

day使用同字节快照完成摘要和解析，每块4096条，质量和收益跨块连续。A股乱序/重复拒绝，非A股兼容路径最多8路磁盘归并。输出顺序确定，切换线程数不改变CSV。

runner流式回读CSV，独立重放冻结day的范围内全部记录，精确核对源字段，再通过SQLite临时表核对应有/意外键。所有检查通过后生成按日期和证券顺序确定的分区。

TCP日线选择独立052b整数股数来源合同，有总预算schedule时最多10000只沪深A股、
8线程，旧无schedule请求为32只串行；目标日期早于实际采集日。
单位材料、获取日和页/行/正文/报文总预算随请求冻结。worker三轮探测配置端点，
逐证券从握手完整留压缩帧，短页继续、空页同连接确认。发布者独立回读全部
库存与页配对/字段后仍执行应有键核对；来源枚举不等于业务覆盖。
恢复封存候选不联网重采。详见[来源消费合同](../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。

## 4. 账本与不可变发布

一个根目录同时只有一个控制进程；SQLite使用WAL，账本schema为3，增加持久worker/controller观测。Job/Attempt、追加事件、release和目录指针分别持久化。worker启动前先登记Attempt，文件封存后再登记release。

封存包含源、事实、契约和产物库存；私有rename、文件/目录fsync后只读。目录保存全部日期分区映射，未变化分区继续引用已有release。源修订必须增加，source_as_of不得降低，完整目录不留交易日分区缺口。

一个SQLite事务校验上一catalog_version并更新目录指针、Job终态和事件。CAS冲突或事件失败保持上一正式目录。只有已验证且目录事务完成才是released。

消费者先固定catalog_version，再读取所选分区；文件和数据库核验完成后才原子替换输出，不在导出中途重新读取最新指针。

## 5. 数据库模型与恢复

| 项目 | CLI兼容路径 | runner版本路径 |
| --- | --- | --- |
| 目标 | 既有平面表/集合 | DBA预建的新命名空间 |
| CH | ReplacingMergeTree，读取FINAL | 普通MergeTree，唯一release insert-only |
| Mongo | 完整(date,code)唯一索引，upsert | 完整契约键唯一索引，insert-only |
| 验证范围 | 本次全部CSV键及导出字段 | 全release字段、计数、重复及额外键 |
| 发布 | schema7清单 | 全部目标verified后目录事务提交 |

runner每批512行，传输/spawn前fsync请求并事务登记intent。完整限额内响应持久后确认，再精确回读；请求/响应上限各8MiB，证据受工作空间预算约束。CH保存query_id，Mongo保存逻辑请求和完整响应。

unknown、确认丢失或目标尚未开始时，recover不自动INSERT。核查服务器在途请求、原请求/响应和账本后，可以明确隔离未发布候选，用全新release重做；晚到旧请求只作用于旧release。已经verified的原凭据保持不变，恢复核验另存证据。

多目标写入不构成跨库事务。某目标失败时，其他目标可能已有数据，但目录保持旧版本。普通INSERT权限无法阻止凭据持有者向封存release追加，部署须约束外部写入；数据库导出仍检查完整范围。

CLI恢复状态以[导入约定](../operations/IMPORTS.md)为准；runner现场步骤以[运维手册](../runbooks/RUNBOOK.md)及[部署恢复](../../deploy/runner/README.md)为准。

## 6. 资源、观测和部署

网络配置先于首次初始化；默认8许可，主连接、池、订阅与HTTP共用。本地文件和运维默认不启动行情TCP，--tcp-warmup显式预热通用当日池，TCP来源核验自身冻结节点。连接/完整帧/分页/批量期限由绝对任务deadline约束；同步系统调用不可硬实时抢占。

状态和Prometheus textfile记录任务、未结束Attempt、事件、当前版本时点、刷新时点和余量。CLI阶段累计耗时与活跃时间并集分别表示，freshness自然日年龄不证明缺少交易日。

部署使用支持锁、SQLite WAL、rename和目录fsync的本地Unix持久盘。配置JSON不携带凭据，publisher环境单独注入；worker清空继承环境，当前仍是同UID。systemd模板管理单次Job及进程树，事实维护流程负责原子更新下一次Job。

硬磁盘配额、账户/ACL、数据库权限、告警阈值及实际RPO/RTO由部署环境落实并验收。模板初始资源值不是生产容量承诺。

## 7. 备份和版本回退

控制锁下的备份拒绝活动/不明确worker，使用SQLite在线备份包含已提交WAL，复制attempt/release/catalog和数据库请求证据，最后写backup.json。

恢复到全新隔离目录，检查全部摘要、SQLite完整性/外键、所有release/历史目录和数据库证据，再写restore-validated.json。恢复不会切换生产目录；账本/文件备份不包含数据库服务端数据卷和日志。

源码回退、消费旧catalog_version、数据库灾难恢复分别执行。服务端数据通过数据库一致性备份恢复或在新命名空间重建后精确验收。

## 8. 扩展与验收门禁

| 门禁 | 当前证据 | 关闭条件 |
| --- | --- | --- |
| G1 工程正确性 | 离线回归、固定报文、现有本机组件测试 | 每次变更执行fmt/Clippy/相关测试与证据完整性核对 |
| G2 实际业务范围 | 公共源有限样本、两个数据库本机子集 | 所选证券/日期/单位/完整性独立验收；0feb/0547分别关闭边界 |
| G3 目标单机生产 | Linux模板已提供，真实全市场/长期目标未验收 | 目标平台、权限、硬配额、真实全量、调度、告警及恢复演练 |
| G4 新契约/来源 | 本地day与少量显式TCP日线闭环已有本地回归 | 五分钟/当天/全市场须另定契约、应有键、留证/预算与独立实盘验收 |
| G5 多主机 | 当前未实现 | 明确外部协调、失效写入隔离、故障模型后实施并验证 |

验收结果与限制统一维护在[当前验收矩阵](../testing/PRODUCTION_ACCEPTANCE.md)。目录落位见[项目结构](PROJECT_STRUCTURE.md)，历史组件实现证据见[实施记录](../archive/development/PRODUCTION_IMPLEMENTATION.md)。
