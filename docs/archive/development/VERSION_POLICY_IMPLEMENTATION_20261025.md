# 版本规则实施记录

2026-10-25，基线 `15de10993cb9411476363fa2eb81cf35a962eacc`，
独立工作目录 `.worktrees/version-policy-20261025`，
分支 `codex/version-policy-20261025`。当前规则见
[VERSION_POLICY.md](../../release/VERSION_POLICY.md)，原始检查见
[本轮证据](../../testing/evidence/version-policy-20261025/summary.json)。

`VERSION=2.0.0`。本地核心/CLI、runner、integration、fuzz 的包版本保持原值。
修复 fuzz 锁文件仍引用核心 1.12.2 且遗漏当前 MD5/ZIP 依赖图的残留；
Cargo `update --workspace` 保留已锁定第三方版本，只补齐当前所需依赖。
主线 CI 首次执行时统一同步到 2.0.1；第三方依赖与协议/数据版本不随项目加号。

Main version 仅监听所有 main push。prepare 先按十进制加号、同步登记文件、提交
run_id 记录，普通 fast-forward push 完成写回；冲突重读主线，确认丢失查原记录。
每次源 push 只计一次，重跑定位记录引入提交，不回退较新主线。
不使用共享 main concurrency 队列，避免 GitHub 覆盖尚未执行的事件。
GITHUB_TOKEN 写回不再次触发工作流。

统一质量工作流通过显式 revision 检查同步后的提交，保留三平台、MSRV、Clippy/
文档、行为、实际包、fuzz、性能和真实数据库任务。prepare/tag 分别使用写 token，
质量与 PR 使用只读 token，tag 必须等待全部质量成功。
制品使用实际检查 SHA 命名；release gate 用记录的 run_id 和原始 push SHA 定位
最新运行尝试，再按实际检查 SHA 校验包身份，保留旧的失败/缺任务拒绝边界。

本机 Rust 1.89：fmt、all-targets/all-features Clippy 和 workspace/文档测试通过，
585 passed / 0 failed / 28 ignored。运行设置 `RUSTDX_SKIP_INTEGRATION_TESTS=1`；
外部线上用例未验收。Python 31 项通过，最终版本定向回归 8 项通过，
actionlint 1.7.12 通过。测试使用私有临时 Git remote，覆盖全部 999 次有效进位、
9.9.9 上限、非法版本、本地写入口拒绝、PR 继承和非法改号、前端 JSON 字段、
第三方锁/历史字节保护、多提交单推送、重复事件、并发冲突、确认丢失和标签冲突。

另在 `/tmp/rustdx-version-policy-20261025/synced-source` 模拟统一 2.0.1。
所有 workspace 包与 fuzz 包/核心锁图通过 Cargo metadata `--locked`；
实际核心/CLI `.crate` 完成清单核对、解包核心下游运行、解包 CLI 编译及
当前调用方运行/旧日历、心跳、财务签名 E0308 预期失败。使用 offline 本地 patch
审核 CLI 注册表依赖，不代表注册表已存在 2.0.1。该包预检为 package-only，
`local_gate_passed=false`、`publication_qualified=false`，没有上传。
实际包和源码归档留在临时预检目录；本轮证据提交报告、摘要和原始日志。

远程主线新流程要在 PR 合并后取得真实结果，本机模拟不能替代 Actions；
本轮未合并 main、未打远程版本标签、未发布软件包、未运行真实数据库/fuzz/三平台验收。
私有仓库 protection/rulesets 查询返回 403：
`Upgrade to GitHub Pro or make this repository public to enable this feature.`；
服务器强制 PR 保护尚不能启用，未变更仓库可见性或账户方案。
