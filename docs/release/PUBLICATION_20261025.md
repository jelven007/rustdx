# rustdx 2.0.0 发布记录

2026-10-25，核心库与CLI均已实际发布到crates.io，两包的注册表索引可见。
runner0.1.0交付源码与macOS ARM64二进制，不发布crate。

## 发布身份

- 源码提交：`82c6de4e52dae907cb9055d4cd07bbb9444e0c47`，已推送main。
- 工程源码SHA-256：`203ad065fb17585089ab71d21bdff9325bf7dc1882e332ceddeb8a1aa4dc6e93`。
- 工程身份范围：`scripts/release/source_scope.py`的scope1；冻结历史证据另行保留与核验。
- 工具链：Rust1.89.0，edition2024；两包许可证MIT。
- 发布前后源码与工作区状态一致。此后的发布状态文档提交不是已上传包的源码身份。

| 发布单位 | 注册表创建时间（UTC） | 字节数 | 实际`.crate` SHA-256 |
| --- | --- | --- | --- |
| [jelven-rustdx-complete 2.0.0](https://crates.io/crates/jelven-rustdx-complete/2.0.0) | 2026-10-25 03:59:58 | 268950 | `0569ed227346c11a4d2eefa0997324bf031797147fb499a79c952e7d81b3220e` |
| [jelven-rustdx-cli 2.0.0](https://crates.io/crates/jelven-rustdx-cli/2.0.0) | 2026-10-25 04:04:50 | 89157 | `51724fed0fe2278c9942b76528248607b6bf72de19df418518b8557f20ed3905` |

## 实际执行与核验

同一发布提交的完整本机预检16项和启动/心跳专项24项通过；
原生Windows另连续执行10次心跳失败恢复回归。
9项远端发布门禁通过，发布前再次查询了最新同提交运行：
[CI 40157832219](https://github.com/jelven007/rustdx/actions/runs/40157832219)、
[数据库验收40157832186](https://github.com/jelven007/rustdx/actions/runs/40157832186)。
远端contracts实际制品已下载，源码身份与两个审核候选包核对通过。

在干净发布源码上按以下顺序执行，四条Cargo命令均退出0：

```sh
cargo +1.89.0 publish -p jelven-rustdx-complete --dry-run --locked
cargo +1.89.0 publish -p jelven-rustdx-complete --locked
# 核心注册表API、下载包及sparse索引核验通过后：
cargo +1.89.0 publish -p jelven-rustdx-cli --dry-run --locked
cargo +1.89.0 publish -p jelven-rustdx-cli --locked
```

CLI正式dry-run使用注册表核心2.0.0，未使用本地patch。实际包的规范化依赖为
`jelven-rustdx-complete ^2.0.0`，包内Cargo.lock固定核心2.0.0及上表校验和。
CLI实际包与本地隔离候选的整体摘要不同：候选锁文件使用本地审核核心，
实际包记录注册表锁与干净Git提交。全部生产源码、嵌入资源、build.rs、
原始Cargo.toml、README及许可证已逐字节核对一致。

上传后重新读取两包版本API与sparse索引，并下载实际`.crate`。
下载字节与上传审核字节一致，SHA-256与API及索引checksum一致；
版本、MIT、最低Rust1.89、未撤回状态、包清单及Git身份均通过。
所有非Cargo生成文件逐个与发布提交核对，全部src文件均包含在包内。
最终注册表核验时间为2026-10-25 04:05:19 UTC。

## 安装与交付

使用Rust1.89或更新版本安装CLI：

```sh
cargo install --locked jelven-rustdx-cli --version 2.0.0
```

应用使用核心库：

```toml
[dependencies]
rustdx-complete = { package = "jelven-rustdx-complete", version = "=2.0.0" }
```

2.0含返回类型与数据语义变化，升级先阅读[迁移说明](MIGRATION.md)和
[共同消费规则](../development/APPLICATION_INTEGRATION.md)。

本地最终交付目录为`reports/release-2.0.0-published/`，
归档为`rustdx-2.0.0-published-macos-arm64.tar.gz`；包含两个注册表实际包、
审核源码归档、macOS ARM64的rustdx/runner、原本机与远端证据以及新发布证据。
`publication-manifest.json`和`STATUS.json`记录实际上传结果，`SHA256SUMS`校验交付成员。
`release-manifest.json`及`checks/local/preflight.json`保留发布前门禁事实，
对应的原审核候选包另存于`checks/reviewed-candidates/`。

此前`reports/release-2.0.0/`仍保留发布前候选状态，其“待上传”描述属于当时记录。
源码归档和实际包内README也保留发布提交的原字节；当前发布状态以本记录及最终交付状态为准。
本次软件包发布不替代目标生产部署验收，二进制未作位级可复现声明。
