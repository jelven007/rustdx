# 上游能力修复与扩大采样证据（2026-10-27）

这是第二阶段的新记录，不改写[第一阶段原帧和结论](../live-market-legacy-fix-20261027)或更早冻结证据。
当前行为以[应用合同](../../../development/APPLICATION_INTEGRATION.md)为准，分析见[修复报告](../../../archive/development/UPSTREAM_CAPABILITIES_FIX_20261027.md)。

| 材料 | 含义 |
| --- | --- |
| [raw-captures.zip](raw-captures.zip) | 八轮原始目录逐字节封装；各目录包括请求、响应头、wire、正文、端点、UTC、DTO、精度输入和日期列表 |
| [independent-replay.json](independent-replay.json) | stdlib独立重解全部464帧，其中456次0feb请求、180非空、276空、43,200点；列全部跨节点差异索引及日线端点差异 |
| [replay.py](replay.py) | 不调用Rust/第三方SDK的原帧和现金口径重放；源数据不一致列为差异，不当作客户端解码错误 |
| [replay_negative.py](replay_negative.py) | 仅内存合成损坏：头魔数、消息号、日期selector、DTO原价，四项均须拒绝 |
| [source-info.json](source-info.json) | 原帧ZIP/第三方源码来源、摘要及请求日期schema2的补充解释 |
| [probe-tool](probe-tool) | 实际采样工具原源码/锁文件及四组输入，固定两节点、有界同步查询；Cargo path依赖保留当时路径 |
| [eltdx-3.3.0.tar.gz](eltdx-3.3.0.tar.gz) | PyPI完整源码包，含许可证；只支持已检查的0547请求布局和本地响应边界 |
| [verification.json](verification.json) | 本阶段新执行的工程/消费/证据检查、真实退出码、范围与未执行项 |
| [manifest.json](manifest.json) | 交付源码、夹具与新增证据的字节摘要；摘要不授予源业务真实性 |

回放不联网：

```sh
python3 docs/testing/evidence/upstream-capabilities-20261027/replay.py --output /tmp/upstream-replay.json
python3 docs/testing/evidence/upstream-capabilities-20261027/replay_negative.py --output /tmp/upstream-negative.json
```

原采样summary的schema=1描述该工具的结果格式，未写专用日期schema字段。
不能将其误解为旧ordinal选择器：本次每个实际请求字节、DTO请求和有效YYYYMMDD逐一按32位补码核验，日期schema=2另在source-info/独立结果登记；原summary不回写。
源精度输入继承第一阶段目录证据，metadata_generation=0及获取时间是采样工具新建快照的本地元信息，不是重新加载目录的服务端证明。

边界证据只支持当前节点在采样时点返回了240点或空。两节点各225个唯一日期/证券键，另有6次重复请求；重复查询在各自节点一致。
33个跨节点键不一致；开/昨收header在这些键相同，正文价/均价/量仍不同。
sz000001的106组官网day开末价比较有35组差异；day开价是整数分，先按/100转binary32再与0feb头部bits比较，末价比较整数分。
20261027不在冻结day文件中，明确登记为参考缺失；不补造当日参考。

财务原公告和物理第33页文字使用继承[Q3夹具](../../../../crates/rustdx-core/tests/fixtures/finance-q3)，新整数操作数固定在[现金口径期望](../../../../crates/rustdx-core/tests/fixtures/finance-cash-basis/expected.json)。
经营/投资/筹资合计8,515百万元，加汇率−2,643为披露净额5,872；源现金槽104偏移的0x4b01edb8严格等于8,515×1000的binary32，转换为元不替换原值。

半帧回归为本地TCP模拟：发送前已接收7字节，后续同message id的push游标11不能满足新请求，真正响应游标12才返回。
`push-boundary-red-*.log`、`push-boundary-reverse-red.log`、`cash-basis-red.log`、`cash-basis-reverse-red.log`及`targeted-restored-green.log`保存原始红绿/反转输出。
这些材料不证明真实服务器被动推送、完整历史连续性、固定TTL或生产可用性。

首次全量工程检查有一项既有进程组回收断言失败；`workspace-tests-first-failure.log`保留退出101原输出。
`isolated-cleanup-results.json`及三份对应日志记录单独三次通过；根因未确认，未修改该用例或生产回收实现。
后续同命令全量和其余检查以verification/check-results记录为准。
