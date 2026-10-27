# 2026-10-27实时遗留修复证据

基线18cc661d1b0ff0169dcbfcaa571f48a5b96dcdf3，分支fix/live-market-legacy-20261027；版本2.0.2未递增。此目录是新取证及修复回归，不修改旧冻结记录。

| 材料 | 事实与边界 |
| --- | --- |
| update-hz/update-sh | 2026-10-27 14:57:52–14:57:59 UTC+8，两节点各4次poll_update、三证券，均RequestedRefresh；原请求/header/wire/payload/时间、精度和最终状态保留 |
| window-hz-explicit/window-sh-explicit | 两节点各11日×3证券，共66次240点0feb请求；原帧及结果保留；000001每日期开末价对官网day，仍没有完整独立分钟日期资格或保存期限保证 |
| window-hz/window-sh及同名log | 初次预算内建连超时的失败材料；后续先完成当日池/连接核验，再在120秒操作预算中采样，成功材料另存，不覆盖失败 |
| refresh-tool | 当轮采样Rust源/Cargo.lock；是历史采样输入，path依赖指向当时隔离工作目录；无后台心跳、无自动重连 |
| finance/daily/minute/update日志 | 缺API或行为失败、修复后窄回归；临时撤销主动刷新时TimedOut，恢复后成功；错误释放许可反例保留 |
| live-replay.json/replay_updates.original.py | 当轮独立回放结果及原脚本，原脚本绝对路径属于当时环境 |
| replay_updates.py | 可迁移的离线复验入口，逐原帧和独立解码核对；不联网且拒绝覆盖已有输出 |
| prepare_*_fixture.py | 原夹具准备脚本；独立源预期由公告/day/腾讯原材料生成，不从Rust实现输出生成 |
| official-current | 官网zip元数据、中央目录/尾部、压缩成员及解压day原材料，用于追溯参考来源；不是全量zip或全市场范围 |
| verification.json/manifest.json | 本轮检查结果及源码/新证据文件摘要；摘要证明字节一致性，来源真实性与业务资格另论 |

Q3、完整日线和209点分钟固定原材料分别在[finance-q3](../../../../crates/rustdx-core/tests/fixtures/finance-q3)、[daily-reference](../../../../crates/rustdx-core/tests/fixtures/daily-reference)、[minute-boundary](../../../../crates/rustdx-core/tests/fixtures/minute-boundary)；逐字段预期与摘要同目录。

从工作区根离线运行：

```sh
python3 docs/testing/evidence/live-market-legacy-fix-20261027/replay_updates.py \
  --source . --output /tmp/rustdx-live-market-replay-new.json
```

完整修复范围、命令、跳过项及上游未关闭条件见[修复报告](../../../archive/development/LIVE_MARKET_LEGACY_FIX_20261027.md)。首点累计量和11:30差异不通过补标签、删点或修改原码处理；RequestedRefresh不计为被动推送。
