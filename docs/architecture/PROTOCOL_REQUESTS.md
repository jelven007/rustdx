# checked协议接口与独立解码

本接口属于2026-10-23仓库源码，Cargo版本未变，尚未发布。同步TCP、网络许可、重试与整任务时限沿用现有实现。

| 新请求 | 参数与响应边界 | 独立解码 |
| --- | --- | --- |
| `KlineRequest` | 显式市场、六位ASCII代码、周期0–11、1–800条；日期/时间/数值检查 | `decode_stock_bars`、`decode_index_bars` |
| `QuotesRequest` | 1–80个不重复的市场/代码对；返回证券必须属于请求且不得重复 | `decode_quotes` |
| `FinanceRequest` | 单证券；响应数量0/1，返回市场/代码须一致；全部浮点有限，非零日期须有效 | `decode_finance` |
| `TransactionRequest` | 1–2000条，u16分页偏移；检查响应数量、时分、正价格、非负量和编号 | `decode_transactions` |
| `HistoryTransactionRequest` | 上述分页参数及有效YYYYMMDD；历史逐笔没有编号，返回num=0 | `decode_history_transactions` |

请求参数私有，构造返回Result，编码只依赖构造后不变的参数。解码不保存原始正文，不修改请求，拒绝截断、超数量与尾随字节；错误不交付部分行。未知逐笔方向值原样保留，不强行解释为买卖。财务日期0表示来源未知，负利润/现金流合法。行情无返回不能据此认定停牌或退市。

`Client::quotes/finance/transaction/history_transaction`已迁移新路径。旧`SecurityQuotes/FinanceInfo/Transaction/HistoryTransaction`公开类型和构造器保留，`try_parse`复用独立解码，`parse`遇错清空data；旧构造器仍可能panic，旧公开字段和发送字节仍可能被调用方改得不一致，建议迁移新请求。

财务换算修正为先将源f32扩展到f64，再乘10000，与pytdx一致。旧实现先在f32中乘10000，可能额外舍入；资产/股本等字段低有效位可能变化。例如本次真实财务流通股本旧值19,405,684,736，新值19,405,685,000，与源f32换算及独立pytdx一致。接口保留原始财务编码，不提供无来源的行业/省份映射。

## 带周期及单位的K线

```rust
use rustdx_complete::{
    data::{Period, Symbol, VolumeUnit},
    tcp::stock::KlineRequest,
};

let symbol: Symbol = "sz000001".parse()?;
let request = KlineRequest::for_symbol(&symbol, Period::Minute5, 0, 3)?;
let bars = request.recv_canonical_stock(&mut tcp, VolumeUnit::Shares)?;
```

`for_symbol`拒绝未核实的北交所TCP映射。`decode_canonical_stock/recv_canonical_stock`与指数对应入口从请求推导周期，调用方必须明确源成交量单位，产物标记TdxTcp及Raw。不推断分钟、指数、基金的量单位；未核实单位须先查源规则。标准CanonicalBar没有指数上涨/下跌家数，需要这些字段时继续使用`decode_index_bars`。旧`CanonicalBar::from_kline`与`Bar::from_kline`继续保留兼容行为。

## 交叉验证

2026-10-23 07:31 UTC+8从115.238.56.198:7709保存原始正文；[夹具清单](../../tests/fixtures/protocol/manifest.json)记录来源、采集时间、请求/正文/预期CSV摘要。参考是pytdx提交`2857fdad08534533610bd2aeca387f760c4baa42`，执行前核对全部引用源码SHA-256，运行时无pytdx依赖。

[验证脚本](../../scripts/validation/verify_protocol_pytdx.py)无`--capture`时离线重解已有正文，带该参数才重新联网采样。Rust回归逐字段核对两个快照、单证券财务及当日/历史逐笔各三条，并覆盖逐字节截断、超量、尾随、返回证券错配/重复、NaN/Infinity、非法日期/时分/负量、旧状态清空和Client实际发送字节。K线原有九条真实样本继续通过；新增标准周期、市场、量换算及北交所拒绝回归。样本范围有限，不代表全市场或所有服务端版本。
