# 核心示例

| 分组 | 用途 |
| --- | --- |
| usage | 本地文件、可靠客户端、共享元数据、指标和使用方式 |
| capture | 有源/时间/节点/请求/响应的原始采样 |
| validation | 显式在线验收与门禁 |
| diagnostics | 调试、性能、字段及连接探针 |

46个名称在Cargo.toml显式登记，仍使用`cargo run -p jelven-rustdx-complete --example <名称>`。分组不改变联网行为；capture/validation不能当默认离线检查。新原始证据用新目录保存，禁止覆盖既往采样。

`checked_owned_bars`离线重放真实股票日线夹具，展示显式市场/周期请求、checked解码、
借用结束后仍可持久化的`CanonicalBar`和原子JSON写入。可传新输出路径；完整保存来源、
价格口径、成交量单位依据和请求页参数。成功解码只建立结构有效性，示例单位仍为
`CallerSpecified`，没有取得时间和节点资格，不授予日期、量额或发布许可。

`LegacyIndicatorError`对应旧通用`Error::Indicator`，
`IncrementalIndicatorError`对应checked EMA/MACD参数与增量状态错误。
原`IndicatorError`和`indicators::IndicatorError`路径保留，无隐式转换。
