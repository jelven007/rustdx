# 独立工程工具

[fuzz](fuzz/Cargo.toml)保持独立Cargo workspace与锁文件，仅用于nightly/cargo-fuzz解析器验证。语料、崩溃和target为本地产物；最小异常作为有效回归样本就近维护。

[验证与范围](../docs/testing/VALIDATION.md#5-fuzz和性能)
