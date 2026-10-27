# 2026-10-27实时遗留问题修复

用户要求“全部修复下当前问题”。本轮完成可控客户端修复：无被动推送时的显式持续刷新、三季报字段资格及冲突拒绝、日线参考并列消费、分钟边界差异诊断。服务端原值差异、被动推送能力及完整分钟保存期限保持未关闭，不能由客户端补数或放宽门禁代替取证。

基线为18cc661d1b0ff0169dcbfcaa571f48a5b96dcdf3，版本2.0.2。实施在独立分支`fix/live-market-legacy-20261027`及工作目录`/private/tmp/rustdx-live-market-fix-20261027`；没有提交、推送或发布。原主工作目录的两处用户修改保持原SHA-256，旧10月27日实时核验报告及冻结支持包也未改写。最终源码和新证据身份由[manifest](../../testing/evidence/live-market-legacy-fix-20261027/manifest.json)记录。

| 问题 | 已实施行为 | 真实结果/未关闭边界 |
| --- | --- | --- |
| 节点无被动推送时无法持续取得更新 | `QuoteSubscription::poll_update`先等待被动事件，空闲后请求刷新；等待基线时立即刷新，显式返回PassivePush或RequestedRefresh | 收盘前两节点各4次RequestedRefresh有行情和游标变化；未观察到被动推送 |
| 新三季报未命中旧默认证据 | 增加市场0/000001、20261024更新、2026-09-30报告期的17字段位模式证据 | 现金净变动源85.15亿元/公告58.72亿元冲突，标准及候选换算均拒绝，raw保持 |
| 052b历史量额与官网day存在差异 | `DayFileReference`校验原字节及摘要，重解TCP，返回原TCP、day标准值和逐字段差异 | 四证券27,237条有44条源差异；不能把参考值静默写回TCP或runner |
| 11:30分钟边界差异影响完整日期参照 | `MinuteSeries::compare_reference`对完整价格点及可选raw累计量结构化诊断，不改变日期资格 | 209点中119点价量差、208点量差；保留合成标签，拒绝改值/删点/平移规避全点门禁 |
| 保存窗口只有少量下界事实 | 新增两节点各11日期×3证券的有界原帧采样及独立回放 | 66请求均240点，000001共22组开末价对官网day；不证明每点实际日期、完整保存范围或稳定保留期限 |

## 财务独立依据与冲突处理

公告为平安银行2026年第三季度报告，原URL：`https://static.cninfo.com.cn/finalpage/2026-10-24/1225621667.PDF`，PDF SHA-256为`40257a9c9bedab90335b30c61e5c4812a18cfcdae5ead78ba475b676866dc31a`。PDF物理第4页披露19,405,918,198股，且期末至披露日没有股本变化；其他依据见第3、25、29、31、33页及[独立期望](../../../crates/rustdx-core/tests/fixtures/finance-q3/expected.json)。

17字段按公告值/单位倍率转binary32后逐位匹配，标准值保留源f32精度，不替换为公告十进制整数。EPS口径为本年前三季度基本/稀释，每股收益1.94元，不视为TTM或年化。旧H1的10项默认证据不变。

`FinanceCandidateRejection::EvidenceConflict { expected_raw_bits }`使适用profile登记但位模式冲突的字段同时拒绝`value()`和`candidate_value()`；`semantic_status`仍为BaselineCandidate，raw位模式/原值可读取。自定义profile也遵循此规则，未适用profile的候选仍按原单位基线处理。Q3为17个Verified、7个BaselineCandidate（含现金冲突）、7个Unknown。报告期、证券、更新日与其他字段资格分别检查；不能因部分命中就资格化整组消费。

## 日线与分钟的原值保全

日线参考来自`https://data.tdx.com.cn/vipdoc/hsjday.zip`，当轮官网归档止于20261026，解压四个day成员及052b页原字节作为就近回归夹具。sz000001 8,477条无差异；sz000988 6,319条中的20150701为TCP 40,905,300股/官网40,905,309股；sh600000 6,413条有38个金额差异，sh600519 6,028条有5个金额差异，均1ULP。合计27,237条、44条差异，核对全部页而非抽样。

参考构造器限制非空、最多2MiB、32字节整除、实际SHA-256及递增唯一日期。比较器拒绝错证券/命令、参考缺日、公开字段或原始正文被改写；返回的`reference`有`TdxDayFile`来源，原`tcp`及双方来源、参考摘要一起保留。day文件没有证券身份槽，摘要也不认证传输；来源归属和原TCP请求真实性由调用方负责。没有静默替换Client/CLI/runner源值或将官网称作交易所真值。

分钟独立腾讯原JSON SHA-256为`d1472367ae328effb3adf8c011958b25c6f828d72345723a2e2b61f65f93b78a`，完整209点期望在[分钟夹具](../../../crates/rustdx-core/tests/fixtures/minute-boundary/expected.json)。119点标签11:30：TCP价格raw1146/参考1147，累计量913147/913222；208末点另有量差。首点标签09:31、raw量39200已包含此前会话量；不增加人工09:30、不整体移动标签，也不删除119点。比较器返回MorningClose/CapturedTail等诊断位置，不证明实际交易时刻或任一供应商量单位。价格不全部匹配时`with_date_reference`仍拒绝，默认`require_trading_date`仍为Unsupported。

## 收盘前持续更新与日期端点采样

2026-10-27 14:57:52–14:57:59 UTC+8，115.238.56.198:7709及180.153.18.170:7709各4次`poll_update`，订阅sz000001/sz159915/sh510300，均`RequestedRefresh`。独立Python按原请求、header/wire/payload解码，检查每次游标承接、累计行情及全证券最终同步、同连接/代次0，并观察到市场变化；各5帧含握手。无后台心跳、无自动重连。该结果证明客户端主动刷新可持续更新，不计为被动推送。

日期样本为20260105、20260401、20260506、20260601、20260701、20260803、20260810、20260814、20260817、20260901、20261026；相同三证券每节点33请求，合计66请求/15,840点。000001的两节点22组开末价对独立官网day；完整原帧与DTO逐字段回放一致。第一次在父操作预算内建连发生connection timed out，失败日志保留；成功轮次先完成当日池/连接核验，再进入120秒请求预算，未放宽日期或字段标准。

`live-replay.json`保存原轮次结果，新增[可迁移回放入口](../../testing/evidence/live-market-legacy-fix-20261027/replay_updates.py)独立复验8次更新及66请求。响应仍无日期回显，只有000001开末价独立对照；不能声明全点日期资格、全品种支持或完整保存期限。

## 回归与工程检查

窄回归为39项：Q3 2、旧H1 6、日线参考2、分钟诊断2、订阅27。财务已记录2项失败到8项成功；日线/分钟缺API失败到各2项成功；临时撤销主动刷新后无推送节点测试TimedOut，恢复后通过；取消提前退出未释放许可反例active=1/预期0，恢复后通过。错误/篡改/不完整参考不交付假合格结果。

工程检查使用macOS、Rust1.89.0、offline/locked。工作区输出539项单元/集成通过、10忽略，文档测试57通过、18忽略；合计596通过、28忽略，不累加窄回归或重复执行。真实CLI/本地故障21通过、1个真实数据库用例明确排除；Python 36通过；fmt、全目标/全特性Clippy（-D warnings）、Rust文档（-D warnings）、旧H1消费矩阵、新调用方及3个预期不兼容旧调用方、文档导航51份/1,107链接、34份生命周期及严格版本检查均通过。最终结果及精确命令见[verification.json](../../testing/evidence/live-market-legacy-fix-20261027/verification.json)。外部集成用例使用`RUSTDX_SKIP_INTEGRATION_TESTS=1`；其中提前return显示ok的外部用例只算测试输出，不算在线验收，ignored同样不算。本轮不重跑数据库实服务、fuzz、Linux/Windows/远端CI、生产部署或长期负载。

当前合同与迁移已经同步：[应用接入](../../development/APPLICATION_INTEGRATION.md)、[签名](../../reference/COMMAND_API.md)、[字段矩阵](../../reference/RESPONSE_FIELD_SEMANTICS.md)、[验收](../../testing/PRODUCTION_ACCEPTANCE.md)、[迁移](../../release/MIGRATION.md)、[FR-03/04/07/13追踪](../../requirements/TRACEABILITY.md)。源码修复不修改版本；未关闭的上游事实继续按各自证据范围验收。
