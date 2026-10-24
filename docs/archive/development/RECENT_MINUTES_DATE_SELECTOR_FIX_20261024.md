# 0feb 日期编码修复与真实日期对照

日期：2026-10-24。需求追踪：FR-01、FR-03、FR-04、FR-18。本轮关闭的是旧请求日期错位的编码缺陷；通用DTO日期资格、完整历史保存窗口和成交股数单位仍待验收。没有提交、推送或发布。

旧20260817/18请求实际向服务器发送了20260609/10的selector。改为负YYYYMMDD整数的32位补码后，两个真实主站、六个证券、六个交易日共72组非空分时、17,280分钟点与同节点同日0fb4的分钟价量全部一致；72次开盘/末价与带日期052d日线一致。周末与未来日期为空。

## 1. 根因和修复

旧公式 `0xfed62304 - Gregorian ordinal` 混用了公历序号和协议日期整数。实际selector是 `-YYYYMMDD` 的小端32位补码；不是按自然日序号递减，也不是按位取反。

| 调用日期 | 旧selector | 旧selector实际选择日期 | 修正selector |
| --- | --- | --- | --- |
| 20260817 | `0xfecad8ff` | 20260609 | `0xfecad82f` |
| 20260818 | `0xfecad8fe` | 20260610 | `0xfecad82e` |

[RecentMinutesRequest](../../../crates/rustdx-core/src/tcp/stock/recent_minutes.rs)先验证有效1000–9999年YYYYMMDD，再使用 `requested_date.wrapping_neg()`；`RECENT_MINUTES_DATE_SCHEMA_VERSION`从1升为2。解码、目录精度、同步I/O、受管连接和日期资格门禁保留。

已有24组旧非空夹具仍保留原始8月注释、request/header/wire/payload、独立expected及SHA。修复后的回归按其实际发送的6月selector重放，不重写旧日期来使测试通过。日期核验/冻结工具对缺少版本字段的旧支持包使用schema1恢复原调用注释，新采样显式写入 `recent_minutes_date_schema_version=2`。离线工具的schema1分支仅服务历史复核，当前Rust构造器不再发送旧公式。

## 2. 新实采与独立核验

| 项目 | 范围与结果 |
| --- | --- |
| 节点 | 115.238.56.198:7709、180.153.18.170:7709 |
| 采样UTC | 13:33:38起，两个节点分别于13:34:23结束 |
| 证券 | 深市000001、000988、159915；沪市600000、600519、510300 |
| 有数据交易日 | 20260609、20260610、20260817、20260818、20261022、20261023 |
| 空日期 | 周六20261024、未来20261026；两个命令、六证券、两节点共48次空响应 |
| 请求 | 每节点48次0feb、48次同日0fb4、六次160条日K及目录/快照/心跳；没有竞价请求 |
| 原帧 | 每节点141帧，分别2,662,125/2,662,166字节归档计量；原请求与响应身份逐帧核对 |
| 预算 | 当日IpPool.refresh与受管Tcp，3秒请求超时、180秒操作预算；请求间隔250ms，最多512帧/64MiB |
| 同日期分钟核对 | 每节点8,640分钟价量，两个节点合计17,280点，与同节点0fb4全点相同 |
| 日期日线核对 | 每节点960条日K独立解码；各36组分时开盘/末价与所请求日期日线一致 |
| 旧错位佐证 | 24组旧“8月”正文与新对应6月正文逐字节相同，与修正后的8月正文不同 |

采样入口为[validate_recent_auction](../../../crates/rustdx-core/examples/validate_recent_auction.rs)的 `--date-selector-check`。完整请求、响应头、压缩/解压正文、原生DTO、目录精度、时间、地址及SHA见[新支持包](../../testing/evidence/recent-minutes-date-selector-20261024)。独立Python验证不调用Rust或第三方SDK生成期望。

[日期专项验证](../../../scripts/validation/verify_recent_minutes_date_selector.py)同时只读复核旧支持包SHA及历史schema、全部旧夹具文件、新同日对照、节点差异和官方day。[机器结果](../../testing/evidence/recent-minutes-date-selector-20261024/date-selector-verification.json)保留每项记录。[新72组夹具](../../../crates/rustdx-core/tests/fixtures/recent_minutes_date_selector/README.md)固定17,280条独立期望。

## 3. 官方day和节点差异

重用已于10月22日从 `https://data.tdx.com.cn/vipdoc/hsjday.zip` 取得的原始成员 `sh\lday\sh600000.day`，本轮未重新下载。原ZIP摘要为 `07aab2f1166a794cebfa380cda058417499c4f74049a5fe36fe029f5c803d400`；成员摘要为 `22855dea0db065e4875d005c7ba55dff5cccc4cbf491aeccfb43d8a278529748`。从第五阶段重建的子集ZIP提取，成员字节与来源清单一致。完整成员、源摘要链见[official](../../testing/evidence/recent-minutes-date-selector-20261024/official)。

独立按32字节day记录解析，600000的6月9/10、8月17/18和10月22共五日、两节点十次昨收/开盘/末价均匹配。6月9日官网开盘9.33、收盘9.37；6月10日9.37、9.59；8月17日9.09、9.04；8月18日9.07、8.97。旧“8月”正文匹配的是前两日，修复正文匹配后两日。该来源截至10月22日，未用它证明10月23日。

两节点48组日期/证券中46组正文逐字节一致，另两组保留如下源差异：

- 510300的20260610：九个分钟均价raw相差1；索引29/30的量为9410/7418与9210/7618，相差+200/−200，合计相同。分钟价及头部昨收/今开相同；这些平均价没有独立金额来源资格。
- 600000的20261022：索引133/134的量分别为1470/723与1462/731，相差+8/−8，合计相同；各节点均与自己的同日0fb4一致。

这些差异没有被改写或用容差抹平，不能把节点之间的分时量分配视为精确一致。机器结果另保留分钟量乘100与官网股数的候选差值；例如6月9日−7股、6月10日−40股。没有据此验收成交量的股单位或提高默认量资格。

## 4. 工程检查与仍待验收

Rust1.89、macOS环境下：fmt检查、Clippy工作区/all-targets/all-features及 `-D warnings`通过；四组专项共32项通过，覆盖日期字节、闰日/年月边界、非法日期、旧实帧与新17,280条期望、截断和日期门禁。工作区测试运行器汇总546 passed、27 ignored；设置 `RUSTDX_SKIP_INTEGRATION_TESTS=1` 的外部集成项可提前返回，不计为本轮真实数据库/网络验收。

首次工作区命令未设置跳过开关，CLI外部ClickHouse测试因本机没有服务而Connection refused；日志和退出101保留，后续按仓库已有开关执行离线回归。专项Python先假定跨节点正文或分钟量完全一致，被上述源差异拒绝；改为保留逐字段差异并严格验证同节点日期对照，失败记录也保留。早期旧二进制启动不识别新参数，在创建采样目录/发送请求前退出，记录独立留存。最终命令、退出码、源码摘要、日志和冻结库存见[verification.json](../../testing/evidence/recent-minutes-date-selector-20261024/verification.json)。

源码与上一阶段相比，仅七份Rust文件变化，其余223份原Rust输入摘要一致。Python日期回归、合成解码回归、六脚本语法、新旧夹具来源/SHA和当前文档链接也核对。没有重跑Linux、数据库原生恢复、远端CI、插桩fuzz或真实盘中推送；原阶段结论按原输入保留。

`server_trading_date`仍为None，`date_qualification()`仍为Unverified，`require_trading_date()`仍返回Unsupported。所列样本的独立日期对照没有形成可应用于任意请求的DTO日期证据。通用日期资格、完整保存窗口、全市场/全年范围、均价独立业务口径及分钟成交股数仍待验收；跨接口消费规则维护在[APPLICATION_INTEGRATION](../../development/APPLICATION_INTEGRATION.md)。
