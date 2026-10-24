# 板块完整文件验证支持包

最终源码两节点采样为`live-a/`、`live-b/`，`independent.json`独立重解80帧与全部105,826条成分。`initial/`保留格式整理前首次采样及其独立结果；完整文件字节相同。历史旧02c5、0feb及TCP日线材料不在本包内改写。

- 每个frame保留request/header/wire/payload原字节、实际端点及UTC观察时点。
- 每节点保留池三轮验证结果、完整block_gn.dat/block_zs.dat、元数据及全部成分输出。
- `capture-tool/`保留实际采样程序源码及独立Cargo.lock；不保留二进制，摘要见`capture-tool-manifest.json`。manifest中的path依赖指向本次实际工作区，其他检出位置须调整构建副本。
- `source-manifest.json`记录249项当前源码、Cargo/AGENTS、三个验证工具和新夹具的SHA；不是Git提交或可独立编译的源码归档。
- `checks/`保留初次故障和最终成功日志/退出码。最终以block-workspace-final、block-clippy-complete、block-fmt-complete及block-live-final-a/b判定；较早的失败不作为最终失败。
- `environment.json`记录工具环境；总检查范围见[机器摘要](../block-integrity-fix-20261024.json)，结论见[报告](../../../archive/development/BLOCK_FILE_INTEGRITY_FIX_20261024.md)。

从仓库根重新核验原帧，输出须使用新的文件：

```sh
python3 scripts/validation/verify_block_integrity.py \
  docs/testing/evidence/block-integrity-fix-20261024 \
  --output /tmp/rustdx-board-recheck.json
```

`manifest.json`记录包内文件字节数及SHA-256（不包含manifest自身）。冻结后新增执行结果写新文件/目录，不更新旧源码或原帧摘要来满足新检查。
