# 第五阶段验收支持包

范围、最终源码身份和判定见[实施报告](../../../archive/development/REMAINING_ACCEPTANCE_STAGE5_20261024.md)。

- `source*-manifest.json`与同名归档：按路径排序的逐文件SHA-256，保留各次输入，不用Git HEAD替代未提交源码身份。最终输入为`source-pacing-final`。
- `local-checks`、`capacity-final-checks`、`rowbinary-checks`：本机命令、退出码和日志；子目录保留失败或中间检查。
- `mac`：本轮初始源码的实验macOS数据库结果，不包含后续RowBinary修复。
- `tcp-live-0/1`、`tcp-independent.json`：两个节点的原帧、请求、CSV和独立对照；跨源成交股数仍未精确一致。
- `official-subset.zip`、`official-subset-provenance.json`：从原官网ZIP取得128个原字节成员，保留原包与派生包摘要及逐成员来源。
- `linux`：最终隔离Linux执行日志、源码/二进制/工具/镜像身份、统计、故障与真实历史清单、清理审计及`packaged-files.json`；`evidence/pre-rowbinary-failure`与`evidence/pre-pacing-failure`保留前轮失败，`evidence/float-probe`保留独立精度诊断。
- `reproduce`：本轮支持脚本。脚本中的私有目录是当时现场；复验应选择新的私有目录并准备固定工具及数据库，不能覆盖本包或原现场。生产账号、真实全市场和业务SLO不由这些脚本生成。
- `final-verification.json`：最终源码归档与工作区一致、原夹具/ZIP成员字节、Linux打包摘要、命令退出码、清理及文档链接检查。

资源计量口径以清单字段为准。Linux采用debug二进制做功能验收；本包不证明release性能、生产部署或数据库服务端卷恢复。完整历史CSV位于现场JSON记载的私有scratch目录，支持包只保存其摘要、清单与日志。
