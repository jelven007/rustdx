# 0feb 日期schema2真实夹具

两个主站、六个证券、2026-06-09/10、08-17/18、10-22/23，共72组非空完整帧和17,280条独立期望记录。采样发生于2026-10-24；每个请求的日期selector为负YYYYMMDD整数的小端补码，逐组版本见[manifest.json](manifest.json)。

原始请求、响应头、压缩正文、解压正文按来源字节复制，expected JSON由Python标准库解码。完整来源、接收时点、精度及SHA均保留。每组分钟价格/数量与同节点同日0fb4一致，开盘价与末价与带日期052d日线一致。两节点的分钟价和头部昨收/今开一致；510300有9个均价raw相差1、相邻两分钟量差+200/−200，600000有相邻两分钟量差+8/−8，见修复报告。周末10-24和未来10-26为空；空响应保存在完整支持包，不进入非空夹具。

`recent_minutes_fixtures`回归验证全部记录及原始请求编码。旧[nonempty夹具](../recent_minutes_nonempty)保持原字节与8月注释；修复后的测试按其实际发送的6月selector回放，旧期望不被重写。根因、官方day对照与范围见[修复报告](../../../../../docs/archive/development/RECENT_MINUTES_DATE_SELECTOR_FIX_20261024.md)。

`server_trading_date_verified=false`保持不变。样本对照没有建立适用于任意请求的DTO日期资格、完整历史保存窗口或成交股数单位。

```sh
python3 scripts/validation/freeze_recent_minutes_fixtures.py \
  --output crates/rustdx-core/tests/fixtures/recent_minutes_date_selector --check
cargo +1.89.0 test -p jelven-rustdx-complete --test recent_minutes_fixtures --offline --locked
```
