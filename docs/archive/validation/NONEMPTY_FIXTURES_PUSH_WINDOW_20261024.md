# 0feb 真实非空夹具与 0547 交易窗口回归

日期：2026-10-24。本文是[上一轮边界验证](RECENT_MINUTES_PUSH_BOUNDARIES_20261024.md)的后续更新：**0feb 已有真实非空完整帧回归；0547 的交易窗口验收算法和本地连续推送已覆盖，真实盘中主动推送仍待实际交易时段留证。**

## 1. 本轮交付

| 交付 | 内容 |
| --- | --- |
| [真实非空夹具](../../../crates/rustdx-core/tests/fixtures/recent_minutes_nonempty/README.md) | 两个主站、六个证券、两请求日期，24组非空完整帧、5760条分钟记录；每组request/header/wire/payload与独立期望 |
| [非空实帧测试](../../../crates/rustdx-core/tests/recent_minutes_fixtures.rs) | 全部原始字段和换算、头部f32位模式、请求selector及帧身份核对；真实股票/ETF正文截断检查与本地TCP回放 |
| [夹具固定/核验工具](../../../scripts/validation/freeze_recent_minutes_fixtures.py) | 先检查原始支持包SHA，再用Python标准库独立解码；拒绝覆盖已有目录；`--check`只读复核来源、全部文件SHA和5760条字段 |
| [推送验收示例](../../../crates/rustdx-core/examples/validate_quote_push.rs) | 将实际帧评估提取为`assess_rows`，采样循环与测试使用同一逻辑；增加3项交易窗口/通知判定回归 |

本轮新增6项Rust测试：`recent_minutes_fixtures` 3项、`validate_quote_push` 示例中3项。合并既有边界/会话用例，五组专项共 **45通过、0失败**；格式和workspace全targets/allfeatures Clippy `-D warnings`通过。全workspace/allfeatures回归为 **483通过、0失败、24忽略**，外部数据库和旧live测试按既有离线开关排除；框架的ok数量不表示实盘验证次数。完整检查与固定摘要见[机器证据](../../testing/evidence/nonempty-fixtures-push-window-20261024.json)。

## 2. 0feb 的真实样本范围

| 项目 | 值 |
| --- | --- |
| 主站 | 115.238.56.198:7709、180.153.18.170:7709 |
| 证券 | 深市000001、000988、159915；沪市600000、600519、510300 |
| 请求日期 | 2026-08-17、2026-08-18 |
| 实际采样日 | 2026-10-24 |
| 每组点数 | 240，含正/负差值及股票/ETF精度 |
| 数量 | 2主站 × 6证券 × 2日期 = 24帧 / 5760记录 |
| 来源 | [杭州归档](../../testing/evidence/recent-auction-contracts-20261024/115.238.56.198-archive)、[上海归档](../../testing/evidence/recent-auction-contracts-20261024/180.153.18.170-archive) |

原先最近五个交易日的15次空响应仍是当时的真实结果。后续更早日期的归档采样获得非空正文，因此不再将“没有非空真实响应”列为缺口；保存窗口仍不能由这两个请求日期完整推断。

每条记录精确核对`price_field_raw / average_field_raw / volume_raw / price_raw / average_raw`及索引、分钟标签；转换价格和均价只容忍JSON浮点往返误差，不容忍任何原始整数差异。时间标签包括11:30→13:01午休跳转。帧解压后必须与保存payload逐字节一致，请求重编码必须与原始request一致。

负向测试对股票000001、ETF159915和高价股票600519的真实payload检查每个截断位置，拒绝尾随字节与超240点声明。另将000001、159915和510300的原始压缩/非压缩正文经本地TCP送入`RecentMinutesRequest::recv`，核对完整字段和本地来源；原始夹具不修改，只有本地回放帧的消息路由ID适配请求。

**保留的日期限制：** 0feb没有服务端交易日期回显，`server_trading_date=None`及夹具`server_trading_date_verified=false`保持不变。归档独立核验中，同请求日期0fb4对照与0feb不一致；不能据该差异单独判断哪一路的日期真实，也不能声称已完成交易日真实性、完整历史窗口或成交量单位验收。

## 3. 0547 交易时段判定用例

新增测试使用显式注入的测试时间`2026-10-26 09:35`，不修改计算机时钟，不伪造实盘证据。

- **完整判定链：** 第4秒的合法更新只推进参考状态；第6秒重复相同状态不计数；第7秒有效游标与累计量变化计1；重复帧保持1；第9秒第二次变化达到2，才满足推送帧数条件。
- **5秒边界：** 第5秒前1纳秒不计有效推送，恰第5秒可计；基线前旧通知不得覆盖较新的游标/状态。
- **其他验收条件：** 即使有两次变化，观察期产生请求、诊断模式或观察未完成仍不能通过。
- **交易会话边界：** UTC 01:30对应北京09:30；09:30/13:00可以开始30秒窗口；11:29:29/14:59:29可以完成；09:29:59、结束恰11:30或15:00的窗口拒绝。

这些测试调用真实采样循环使用的`assess_rows / window_open / acceptance`。已有21项订阅回归继续覆盖多证券被动更新、基线补齐、同消息号推送、完整五档替换、重复/空帧、整页拒绝、半帧、游标回退和资源释放。

当前仍为周六，严格采样门禁是`market_closed`；两主站此前60秒休市诊断都未观察到后续推送，见[真实推送验证报告](QUOTE_PUSH_VALIDATION_20261024.md)。新测试证明判定和会话路径，不能替代真实交易时段连续推送。下一交易日为2026-10-26，可按该报告在09:35或13:05执行严格采样与独立轨迹核验。

## 4. 复现

```sh
python3 scripts/validation/freeze_recent_minutes_fixtures.py \
  --output tests/fixtures/recent_minutes_nonempty --check

cargo +1.89.0 test -p jelven-rustdx-complete \
  --test recent_minutes_fixtures --test recent_minutes_boundaries \
  --test quote_subscription --test command_boundaries \
  --example validate_quote_push --offline --locked
```

不需要联网即可复跑24组真实非空夹具及45项专项。新增真实盘中采样使用新目录并遵守既有连接/队列/时限预算；仅基线、空通知、游标单独变化或未知尾部变化都不满足验收。本轮未新增运行时依赖、异步运行时、CLI或runner任务，也未提交或发布。
