# TCP日线整数股数修复

2026-10-24。关联FR-03、FR-04、FR-13及FR-18。新CLI/runner日线采集已改用052b整数股数；两个真实主站、三只在市沪深A股、2026-10-19至22共24条日线的日期、代码、OHLC、金额和成交股数全部与已留存官网day文件精确一致。

## 原因与修复

052d category9在所测样本中返回整数手数，股尾数在源字段中已经丢失。category4改用binary32股量仍有量化：同样24条中16条不能与官网整数股数精确一致。四舍五入、乘100或改容差都不能恢复这些尾数。独立原帧诊断见[precision-analysis.json](../../testing/evidence/tcp-day-volume-fix-20261024/precision-analysis.json)。

052b虽沿用旧SDK的“退市日线”名称，但所测两个节点也返回在市股票。它的成交量是非负变长整数，按u32股数解析后可精确扩宽为f64。新入口`Client::daily_bars_range_with_options`复用既有分页预算、来源连续性、空页复查和下界停止规则；`DailyBar/DailyBarsRequest/DailyBarsPage/decode_daily_bars`是旧`Delisted*`名称的通用别名。052b报文没有category槽，返回集合的category=9只表示日线分类。

接入同时修复了052b金额解码：原指数算法遗漏IEEE754隐含首位，新解析按非负有限binary32人民币元扩宽。例如sh600000的2026-10-22金额原码1309654326，正确值602,885,504元，旧算法得到66,014,592元。零、低指数和非有限/负数拒绝均有回归。与官网day金额精确相等只确认两份报告值一致，不代表成交额具有逐笔分币精度。

CLI/runner新输入合同为`a-share/tcp-052b/day/raw/cny/integer-shares/v2`，请求及逐证券收据schema=2；CSV报告仍为schema8、`volume_unit=shares`。旧052d来源v1请求及收据只保留原字段离线重放，新采集拒绝旧合同。库存、日期、身份、预算及字段复查继续执行；重叠记录比较绝对价格、量、金额原码和未知尾槽，页内价格差值及页首前收不参与跨页相等判断。

## 真实核对

| 项目 | 结果 |
| --- | --- |
| 节点 | 115.238.56.198:7709、180.153.18.170:7709 |
| 证券 | sh600000、sh600519、sz000001 |
| 比较日期 | 2026-10-19至2026-10-22，每节点12条 |
| 新采集 | 每证券一页12条，过滤后4条；全部越过下界停止 |
| 独立核对 | 标准库Python直接解压、解析原帧，再比收据、CSV及官网day；8字段精确相等，零容差 |
| CSV摘要 | 两节点均为2ecd33d4ea44f4cf2185275149b2d112913774905a3462c4f0ff087c558dd0ae |
| 最终二进制 | Rust1.89.0 / macOS arm64；SHA-256 b0ed87fef9af5887cf1076a547b117226e28506929b7eee1d58f3b66f8f4b951 |

sh600000的修复前后股数示例：

| 日期 | 旧052d转换股数 | 新052b/官网股数 |
| --- | --- | --- |
| 2026-10-19 | 54,074,700 | 54,074,699 |
| 2026-10-20 | 60,513,200 | 60,513,255 |
| 2026-10-21 | 46,256,200 | 46,256,249 |
| 2026-10-22 | 62,529,800 | 62,529,869 |

首轮[24条核对](../../testing/evidence/tcp-day-volume-fix-20261024/independent.json)用于生成六组就近Rust夹具。完成检查后又用已记录摘要的固定二进制[复采24条](../../testing/evidence/tcp-day-volume-fix-20261024/final-independent.json)，结果及CSV与首轮相同。最终采集UTC为2026-10-24 14:11:51–14:11:52。115节点最新日为10月22日，180节点为10月23日；按请求范围过滤后相同，未改写端点实际日期。

官网参考沿用[第五阶段支持包](../../testing/evidence/acceptance-stage5-20261024)，保留原始来源与各day字节摘要。本轮没有重新下载官网包。原052d的−94至+1股差异、原052b历史错误金额JSON、旧期望及来源摘要不改写；当前验证另行留证。

## 工程检查

Rust2024 / MSRV1.89，保持同步I/O。完整命令、耗时、日志及退出码见[checks.json](../../testing/evidence/tcp-day-volume-fix-20261024/checks.json)：

- `cargo +1.89.0 fmt --all -- --check`通过。
- `cargo +1.89.0 clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`通过。
- `RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo +1.89.0 test --workspace --all-features --locked --offline`通过，统计547 passed、27 ignored。部分联网/数据库测试通过环境守卫提前返回，不能把这个统计当作全部真实服务已执行。
- 新`daily_integer_shares`夹具专项1项通过，核对六份真实响应中的24条原始官网记录。
- runner的日线发布、修订、缺键拒绝与封存后离线恢复专项1项通过；报告摘要同步修改仍不能绕过原帧复查的伪造CSV专项1项通过。这两项原为ignored，本轮显式执行；其余数据库及外部环境ignored项未强行执行。
- CLI的5项日线回归包括非整手股数、金额、压缩短页/空页、重复边界、篡改/孤儿文件、预算拒绝及冻结v1归档重放；原98帧现代命令重放回归通过。

原帧、收据、事实材料、源码摘要、日志、六组夹具及逐文件库存见[支持包说明](../../testing/evidence/tcp-day-volume-fix-20261024/README.md)。本轮没有执行数据库服务端恢复、远端CI、Git提交、推送或发布。

## 范围

已关闭的是所列日线采集路径中的成交股数精度问题及052b金额解码错误。单页u32字段不会丢失整数表示精度，但任意证券/节点的业务值仍须按实际范围核实。全市场、其他品种、当天bar、五分钟、市场2及交易所完整历史未由这24条样本验收；来源枚举完成与runner事实覆盖仍分别判定。

跨接口消费约定见[APPLICATION_INTEGRATION](../../development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)；当前状态见[验收矩阵](../../testing/PRODUCTION_ACCEPTANCE.md)。
