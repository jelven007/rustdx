# 外部集成测试

[integration](integration/Cargo.toml)是根workspace的非发布测试crate。显式网络/数据库条件与本地模拟分别判定；skip或缺条件提前返回不计在线验收。产品回归优先放所属crate/tests。

[验证方法](../docs/testing/VALIDATION.md)
