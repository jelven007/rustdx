# 0fc6 全天验证支持包

对象：`sz300308` / 请求日 `20261023`。业务请求固定 `115.238.56.198:7709` 和一个 TCP 连接，2026-10-24 04:53:17–04:53:18 UTC 抓取，自动重连关闭。

- `capture.json`：连接身份、预算、两轮分页、边界重叠及 K 线查询清单；末尾 `capture_complete=true`。
- `frame-000` 至 `frame-017`：逐帧请求、16 字节响应头、压缩正文、解压正文和实际远端地址，包含握手。
- `pass-*`、`overlap-*`、`latest-32.json`：本库实际解码的完整页，包含原始 payload 和派生字段。
- `kline-{9,4,0}.json`：本库原始 K 线输出；单位未经统一缩放。
- `daily-pool.json`：指定服务器的当日 IP 池探测结果；不把探测连接算作业务分页连接。
- `inputs.json`：109 个捕获输入及 8 个相关源码的固定 SHA-256。
- `canonical-records.json`：4,876 条完整时间顺序记录，列名显式给出，保留相同字段记录的重复次数。
- `results.json`：全部独立解码、分页、重叠、分钟覆盖、48 个累计量边界和量价额核对结果。
- `checks/`：本次 Rust 构建/实时抓取、Clippy、16 项相关回归、格式及离线复核日志。

结果和规范事件文件是重放生成物，未反过来作为解码期望；默认重放核对固定捕获输入和源码，源码变动时明确失败。校验日志与本说明在冻结输入后归档，不属于 `inputs.json` 的 109 个捕获输入。

从仓库根目录执行，不联网：

```sh
python3 scripts/validation/verify_enhanced_history_day.py \
  docs/validation/evidence/enhanced-history-day-20261024
```

[完整结论及限制](../../ENHANCED_HISTORY_DAY_COMPLETENESS_20261024.md)：未检出源接口分页遗漏；常规成交量与 48 根五分钟线精确一致。日线价格极值未被 0fc6 覆盖，不能据此承诺交易所逐笔全集完整。请求日期没有响应回显，空页仅是同源枚举终止证据。
