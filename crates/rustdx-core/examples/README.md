# 核心示例

| 分组 | 用途 |
| --- | --- |
| usage | 本地文件、可靠客户端、共享元数据、指标和使用方式 |
| capture | 有源/时间/节点/请求/响应的原始采样 |
| validation | 显式在线验收与门禁 |
| diagnostics | 调试、性能、字段及连接探针 |

45个名称在Cargo.toml显式登记，仍使用`cargo run -p jelven-rustdx-complete --example <名称>`。分组不改变联网行为；capture/validation不能当默认离线检查。新原始证据用新目录保存，禁止覆盖既往采样。
