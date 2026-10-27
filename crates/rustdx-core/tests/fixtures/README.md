# 测试夹具与来源

本目录只保存可随源码分发的固定回归材料，运行下载包、全量CSV和本次任务清单不放入这里。

| 目录 | 内容 | 使用与证据边界 |
| --- | --- | --- |
| tdx/ | 原assets中的sz000001.day和gbbq二进制 | 本地解析/收益回归；保留原字节，不能作为当前证券事实或每日生产gbbq |
| kline/ | 股票日线、指数日线、股票5分钟各3条请求/响应/期望CSV | [manifest](kline/manifest.json)记录固定pytdx参考及摘要；9条样本不代表全市场 |
| protocol/ | 两证券快照、财务及当日/历史逐笔原始夹具；新财务语义和多精度价格期望JSON | [manifest](protocol/manifest.json)保留历史摘要；财务新期望来自官方中报，报价新期望来自独立腾讯行情 |
| recent_minutes_nonempty/ | 两主站、六证券、两请求日期的24组非空0feb完整帧及5760条独立期望 | [来源与摘要](recent_minutes_nonempty/README.md)；真实非空字段回归，响应没有回显日期，不能据此证明请求日期或保存窗口 |
| recent_minutes_date_selector/ | 日期schema2的两主站六证券六交易日72组非空帧及17,280条独立期望 | [来源与摘要](recent_minutes_date_selector/README.md)；同日0fb4/日线对照，旧8月注释夹具原字节保留，实际selector为6月；通用日期资格未升级 |
| finance-q3/ | 20261024更新的平安银行原0010正文、Q3公告PDF及逐页文本、18项独立预期 | [expected.json](finance-q3/expected.json)；17字段匹配，现金净变动冲突控制，原值不替换 |
| finance-cash-basis/ | 固定Q3合并现金流三活动/汇率/净增加的独立整数操作数及公告摘要 | [expected.json](finance-cash-basis/expected.json)；85.15亿元三活动合计减26.43亿元汇率影响为58.72亿元，原净变动字段仍拒绝不同口径 |
| daily-reference/ | 官网四证券day原字节及052b完整页正文、27,237条逐日对照 | [manifest.json](daily-reference/manifest.json)；保留1条股数差9股/43条金额1ULP差，不授予全市场权威性 |
| minute-boundary/ | 10月27日209点0feb正文及独立腾讯分时原JSON/完整价量期望 | [expected.json](minute-boundary/expected.json)；119点价量差、208点量差不通过平移/删点资格化日期 |
| apps/rustdx-cli/tests/（仓库根） | CLI端到端与本地HTTP/文件回归 | [CLI测试](../../../../apps/rustdx-cli/tests)就近生成隔离样本 |

独立参考期望CSV进入版本控制，`.gitignore`只对这两组`*.expected.csv`开例外；不要通过全局放开CSV把运行数据入库。protocol四份CSV已按既有manifest摘要核对后补入，解决干净检出缺少预期结果的问题。

`finance.expected.csv`保留旧pytdx单位与名称，仅由显式legacy入口验证；`finance.semantic.expected.json`固定官方报告的10项、主体偏移及float32/披露容差。`quotes.precision.expected.json`固定独立闭市价格，原始实帧在[本轮支持包](../../../../docs/testing/evidence/finance-quote-semantics-20261024/live-active-115-238-56-198)；`quote_precision`使用就近的`quotes.precision.request.bin/payload.bin`离线重放这15只证券，其中5只核对独立价格。`modern_commands`同样使用就近的`enhanced-history-0fc6.payload.bin`。三份副本与原始证据字节相同，来源与SHA见[stage5-provenance.json](protocol/stage5-provenance.json)。合成报文另测算术边界，不冒充实盘样本。

增强历史0fc6的四组压缩帧与4876条独立字段期望保存在[专项支持包](../../../../docs/testing/evidence/enhanced-history-0fc6-20261024/README.md)，`enhanced_history_transaction`测试离线重放并核对全部记录字段；四次独立连接的采样不作为全天完整性验收。

金额专项的同连接全天增强/legacy与1m/5m/日线压缩实帧直接引用
[冻结支持包](../../../../docs/testing/evidence/enhanced-history-amount-20261024)。
`trade_amount`、`reported_turnover`、`trade_amount_reconcile` 核对帧/请求/解压 payload、
4876 条原始记录、240/48 个数量边界及独立 Python 金额期望；不通过新 Rust 输出生成期望。
缺少明细来源的离线对齐保持覆盖限制，本地 TCP 回放另测同连接对账与重连失败。

```sh
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test -p jelven-rustdx-complete --test file-day --test fq --test properties --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test -p jelven-rustdx-complete --test kline_decoders --test protocol_decoders --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test -p jelven-rustdx-complete --test finance_semantics --test quote_precision --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test -p jelven-rustdx-complete --test enhanced_history_transaction --locked
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test -p jelven-rustdx-complete --test trade_amount --test reported_turnover --test trade_amount_reconcile --locked
```

重新联网采样使用新目录并保存采集时间、服务器、参数和请求/正文/期望SHA；固定pytdx源码执行前先核对摘要。重解工具见[脚本说明](../../../../scripts/README.md)，语义与覆盖见[协议接口](../../../../docs/architecture/PROTOCOL_REQUESTS.md)。参考日历在[config/calendars](../../../../config/calendars)，验收结果JSON在[docs/testing/evidence](../../../../docs/testing/evidence)。
