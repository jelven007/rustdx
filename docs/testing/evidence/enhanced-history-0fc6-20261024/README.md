# 0fc6 真实报文支持包

采集于2026-10-24 04:40 UTC，显式主站`115.238.56.198:7709`，证券`sz300308`，请求日期`20261023`。四个独立连接分别请求start=0/1800/3600/4876、count=1800，返回1800/1800/1276/0条。

每个`live-start-*`目录保存：

- `frame-00.*`：握手请求、响应头、压缩体、解压体、主站。
- `frame-01.*`：0fc6业务请求及上述完整报文材料。
- `daily-pool.json`：该显式主站的三轮预验证结果。
- `summary.json`：Rust采集器的参数、时间、分类数量与覆盖边界。
- `expected-records.json`：Python独立解码生成的全部13字段期望。
- `independent-verification.json`：逐字段一致结论、参考价及原始材料SHA-256。

为减少重复，原始Rust`page.json`不重复归档；冻结期望前已逐字段核对，固定请求/压缩体可以重建它。使用以下只读命令重新独立核对，四个目录可分别执行：

```sh
python3 scripts/validation/verify_enhanced_history_transaction.py \
  docs/validation/evidence/enhanced-history-0fc6-20261024/live-start-0
```

Rust的`enhanced_history_transaction`专项测试重放四组压缩报文，逐字段核对独立期望。参考价是f32，Rust JSON最短小数显示590.11；Python扩为f64显示590.1099853515625。核对比较原始f32位模式，不将文本长度差异误判成价格差异。

参考协议来源为[固定eltdx 3.2.2 trades.rs](../eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/trades.rs)及[frame.rs](../eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/frame.rs)，不运行完整SDK。采集器为[example](../../../../examples/capture_enhanced_history_transaction.rs)，独立重解器为[Python脚本](../../../../scripts/validation/verify_enhanced_history_transaction.py)。

本支持包验证单证券的命令支持、五个varint布局、原始字段、分类及空页行为。四次业务查询使用不同连接，没有交易日回显、独立日量核对或全市场证据；不作为同会话全天完整性验收。实现与约束见[接入记录](../../../development/ENHANCED_HISTORY_0FC6_IMPLEMENTATION_20261024.md)，工程检查见[汇总JSON](../enhanced-history-0fc6-20261024.json)。
