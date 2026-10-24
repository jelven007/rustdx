# 第六阶段恢复证据

2026-10-24 的隔离 Linux 数据库原生全备与空目录恢复。数据和证券事实明确为合成输入，两个 runner 版本、每库 2,400 行；不是全市场或生产验收。

- [最终演练](linux/evidence/attempt2/recovery.json)：71 条工具命令/SQL 请求，含预期拒绝，最终 passed。
- [首次失败](linux/evidence/attempt1/recovery.json)：恢复数据一致，最后的 ClickHouse 错误措辞判定失败；输入和原始响应完整保留。
- [清理审计](linux/evidence/cleanup-audit.json)：无剩余自建容器/匿名卷，独立 daemon、socket 和该目录 containerd 已停止/移除。
- [最终脚本及部署输入](linux/evidence/attempt2/source/manifest.json)、[二进制来源](binary-provenance.json)、[本地完整性验证](verification.json)。
- [传输清单](support-manifest.json)：回传文件的大小/摘要、未回传的停止后服务器卷文件、归档摘要及按内容去重的 tar 编码。
- [分析与范围](../../../archive/development/REMAINING_ACCEPTANCE_STAGE6_20261024.md)。

每次演练的 `drill-*` 子目录包含 `.day`/合成事实、固定版本导出、全库快照、原生 ZIP/BSON/metadata 备份、控制层完整备份及恢复文件，`logs/` 保存命令正文、stdout/stderr、镜像/容器信息。随机目录和原始 JSON 的远端绝对路径不改写；可按 `recovery.json` 的 `work` 末段，在同一 `linux/evidence/attempt*/` 下定位回传内容。日志内 PID 只表达当时进程，不能作为当前操作对象。

重复字节在传输 tar 内用相对硬链接存储，本地逐文件大小/SHA-256已核对。服务器原数据目录没有回传，其停止后库存保留在各 drill 的 `old-server-stopped-inventory.json`；恢复时未挂载它们。没有回传临时 Mongo 连接配置。
