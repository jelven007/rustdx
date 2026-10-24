# 剩余优化第四阶段：TCP日线runner闭环

2026-10-24，延续[20项评审](../architecture/CURRENT_PROJECT_REVIEW_20261022.md)、
[第一阶段](REMAINING_FIXES_20261024.md)、[第二阶段](REMAINING_FIXES_STAGE2_20261024.md)
与[第三阶段](REMAINING_FIXES_STAGE3_20261024.md)。保持Rust2024/MSRV1.89、同步I/O、
有界线程、版本与用户既有迁移；没有提交、推送、发包或生产部署。

## 本阶段实现

O13已接通小范围TCP日线的采集、归档、标准化、业务覆盖、封存、修订、固定版本消费
及离线恢复。`Config.tcp_day`和day目录互斥；相交名单最多32只排序唯一沪深A股，
当前逐证券同步。来源合同独立，endpoint与单位依据SHA参与Job身份。
旧day JSON省略新增字段仍兼容，Job哈希和量单位保持既有合同。

实际北京时间获取日绑定`planned_date=source_as_of=acquired_on`；
through必须早于它，分页及回读检查获取日不变。日线15:00为解码合成时间，
不会被用作当天收盘证明；今天补采的旧bar保留今天来源版本，不冒充历史时点数据。
单位材料SHA冻结到facts；哈希可证明材料一致，真实性与100股/手仍须维护者审核。

worker新增`tcp-day`，独立池对配置端点三轮协议探测后执行；
父进程和worker跳过通用预热，仍使用受管连接及探测配额。每证券从首次握手
完整归档请求、压缩帧或失败；关闭空闲心跳，保持单会话页配对。
短页按原始行数继续，空页同连接再确认。receipt记录连接、时间、来源、逐页参数/
偏移/数量/字节/重复和停止原因。独立回读一次加载库存，逐序号核查全部文件、
哈希、请求身份、页推进、重复内容和全部标准字段。

schema8 CSV量为股，报告声明独立来源合同及`business_coverage=unverified`；
旧day schema6/7仍从手换算股。股数必须为可精确表示的整数，不靠容差或四舍五入
掩盖TCP源差异。runner核对归档字段与CSV后复用全部事实键校验；空页完成
不能豁免missing。范围外TCP CSV也拒绝；所有键有明确免行情事实时不启动worker。

`max_source_bytes`先扣冻结facts，剩余预算约束所有wire/receipt，
`max_wire_records`为全部证券合计。传输前预留单证券receipt，成功封库存前预留库存；
CSV逐证券检查且最终流式计量，不在全量输出后一次性加载CSV到内存。
单证券仍保留有界区间记录，目录监督仍为软计量；目标硬配额未替代。
失败保留私有候选与日志，可能没有collection.json；没有封存登记的中断须隔离，
已封存恢复只重验字节及目录CAS，不重新联网采集。

完整消费约定统一在[应用接入](../../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)，
严格配置见[TCP示例](../../../config/runner/tcp-day.example.json)，
操作与兼容边界见[runner说明](../../../apps/rustdx-runner/README.md)及[迁移](../../release/MIGRATION.md)。

## 验证

本机macOS arm64、Rust1.89、offline/locked，显式跳过外部集成服务。
机器证据：[remaining-fixes-stage4-20261024.json](../../testing/evidence/remaining-fixes-stage4-20261024.json)；
完整日志：[支持目录](../../testing/evidence/remaining-fixes-stage4-20261024)。

| 检查 | 结果与实际范围 |
| --- | --- |
| TCP采集专项 | 4通过：压缩帧、短页重叠/空页确认、完整字段和股数、改receipt/报文/库存、日期/单位/页/行/字节/报文预算拒绝 |
| TCP runner专项 | 4通过：严格配置、事实免行情无联网、CSV和报告一起伪造仍拒绝、缺键/修订/固定版本与离线恢复 |
| Rust1.89 workspace all-targets/all-features Clippy，-D warnings | 退出0 |
| 格式与严格rustdoc | 均退出0 |
| workspace all-targets/all-features测试 | 480通过、0失败、9 ignored |
| workspace文档测试 | 66通过、0失败、18 ignored |
| 旧day受影响的显式ignored测试 | 5通过：发布/修订与恢复、容量拒绝、封存恢复、默认启动/离线转发、HTTP确认丢失 |

模拟协议peer、证券状态和单位材料均明确为合成测试输入，不是实盘证据。
压缩响应包含1379837手，标准股数137983700；短页偏移0/1/3/3，
第二页重复边界精确核对。全量测试通过数含主动跳过外部服务后返回的用例；
显式专项与全量重叠部分不重复累加。未执行真实数据库服务、公共行情采集、
目标Linux或远端CI。本轮没有新增fuzz/RSS/吞吐结论。

测试依赖加入tempfile/miniz_oxide，均已存在于workspace锁中。
Cargo重新生成锁时产生过无关版本更新，已从第三阶段固定源码归档恢复，
最终相对该快照仅新增CLI的两个测试依赖边，不引入注册表包或升级版本。
原始协议夹具和历史证据未改写。

全量回归后再补发布者对全部证券合计报文条数的独立预算核对；
最终fmt/全workspace Clippy再次退出0，runner全部本地用例
30通过、0失败、1个真实数据库案例filtered。该项不与前述全量数相加。
活跃文档673个相对路径0缺失，git diff --check退出0；锚点和外部URL未验。

## 四阶段后的剩余事项

| 事项 | 当前状态 |
| --- | --- |
| O01/O02 | 身份错配已拒绝；0feb日期依赖仍拒绝，不伪造Verified |
| O03/O04 | 实盘日期/主动推送、官方事实及目标Linux/真实数据库/全量负载仍待外部验收 |
| O05 | 本地源码/包预检工具已实现；正式版本、提交、注册表验收及发布未执行 |
| O06/O07/O09/O10/O11/O12/O14/O15/O17/O19 | 所列分页、容量、流式、来源、范围状态、CI入口、探测、URI、EMA/MACD及缓存修复已有本机证据；真实规模/目标环境边界仍有效 |
| O08/O16/O18/O20 | 目录扫描仍在、分层阶段指标未完整对账、订阅状态fuzz及RSS/吞吐基线未补、旧parse/API未整体弃用 |
| O13 | 少量TCP日线闭环已实现；五分钟、当天bar、全市场和真实业务资格仍待扩展/验收 |

下一次扩大采集应先具备实际证券/单位/日历/状态证据，以独立数据集验证，
再按真实页数、日期范围和资源计量提升规模。没有新的证据时不把上述外部条件
或产品范围扩展标记为完成。
