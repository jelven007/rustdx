# 0feb 真实非空回归夹具

本目录固定两个主站、六个证券、两个请求日期的24组完整非空帧，合计5760条分钟记录。请求日期为2026-08-17/18，实际采样发生于2026-10-24。

每组包含原始request/header/wire/payload及Python标准库独立解码的expected JSON。原始材料、地址、接收时间、精度、SHA256和来源帧位置见[manifest.json](manifest.json)。不通过Rust输出生成自己的期望。

`recent_minutes_fixtures`测试逐字段核对全部记录，使用股票与ETF真实正文测试每个截断位置，并通过本地TCP回放三种真实响应正文。原始夹具为实采数据，本地TCP回放是工程验证。

响应不回显交易日期。`server_trading_date_verified=false`始终保留；来源报告中同日期0fb4对照与0feb不一致，不能据这些样本证明服务器返回了请求日期、完整历史保存窗口或已核实成交量单位。

只读复核：

```sh
python3 scripts/validation/freeze_recent_minutes_fixtures.py \
  --output crates/rustdx-core/tests/fixtures/recent_minutes_nonempty --check
cargo +1.89.0 test --test recent_minutes_fixtures --offline --locked
```

重新固定新样本时用新的输出目录，原始支持包必须已有SHA清单和独立核验：

```sh
python3 scripts/validation/freeze_recent_minutes_fixtures.py \
  --source docs/testing/evidence/recent-auction-contracts-20261024/115.238.56.198-archive \
  --source docs/testing/evidence/recent-auction-contracts-20261024/180.153.18.170-archive \
  --output /path/to/new-fixtures
```
