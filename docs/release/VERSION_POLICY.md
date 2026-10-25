# 版本管理

当前项目版本：<!-- version:project -->2.0.0<!-- /version -->。唯一递增源为根目录 `VERSION`；
版本格式严格为 `a.b.c`，每位为 0–9，按十进制进位：
`0.4.9 → 0.5.0`、`0.9.9 → 1.0.0`，到 `9.9.9` 后停止并明确报错。
这套数字是提交序列，兼容性和破坏性变更另见 [MIGRATION.md](MIGRATION.md)。

每项任务使用独立分支和工作目录，经 PR 合入 `main`。开发、提交、本地合并和功能分支推送
保持版本不变；合并最新主线自然继承版本。PR 检查以与主线的 merge-base 为基准，
拒绝自行改号或编造主线推送记录。

每次远程 `main` push 由 [Main version 工作流](../../.github/workflows/main-version.yml)
递增一次，不设路径过滤；功能、修复、文档、构建都计入，同次 push 多个提交仍只计一次。
GitHub `run_id` 在重跑时不变，`.github/version-events/<run_id>.json` 保存原推送、
前一提交、旧/新版本；原记录引入提交就是本次检查提交。重跑复用它，即使主线已继续前进。
CI 用 `GITHUB_TOKEN` 写回，GitHub 不会为该 token 的 bookkeeping push 再触发工作流。
不使用会覆盖待运行事件的主线 concurrency 队列；并发写回依靠普通 fast-forward push，
冲突后读取最新主线重试，确认丢失后先查记录。拒绝 force push。

执行顺序：

1. 同步版本文件、自有 Cargo 包/依赖、根及 fuzz 锁文件、前后端元数据和当前文档。
2. 对同步后的同一提交运行三平台、MSRV、格式/Clippy/Rustdoc、行为、实际包消费、
   Python、fuzz、性能及真实数据库检查。
3. 全部成功后创建轻量标签 `v<版本>`，指向检查过的提交。检查失败保留已递增的
   主线版本且不打标签；重跑仍检查这个版本。后续成功版本不填补失败版本的标签。
   已有同目标标签可重用，异目标同名标签立即拒绝。

[版本登记表](../../config/release/versions.json) 明确自有包、锁、JSON 字段和当前文档。
初始 PR 保留既有数字（核心/CLI 2.0.0、runner 0.1.0、integration 1.1.0、fuzz 0.0.0），
首个主线 CI 统一至 2.0.1。没有现有前端；新增 `package.json` / `package-lock.json`
必须登记 `json_metadata` 的 `path` 与 `fields`（嵌套键数组，例如 `["packages", "", "version"]`）。
新增 Cargo 包也须登记，检查会拒绝遗漏。第三方依赖、数据库/协议/schema 版本不随项目加号。
文档只同步 `<!-- version:project -->` 等成对标记中的当前数字，历史迁移章节、
CHANGELOG、archive 和冻结证据保留原值。

本地只读检查：

```sh
python3 scripts/release/versioning.py check
python3 scripts/release/versioning.py check-pr --base origin/main
python3 -m unittest discover -s scripts/validation/tests -v
```

`prepare` / `tag` 入口仅接受 GitHub Actions 的 `push`、`refs/heads/main` 及匹配事件载荷。
质量工作流和 PR 使用只读 token；版本准备与标签任务单独持有 `contents: write`。
标签不是 crates.io 发布，正式上传仍按 [RELEASES.md](RELEASES.md)。

远端强制 PR 保护还受仓库账户权限约束。2026-10-25，此私有仓库的 branch protection /
rulesets API 返回 403：`Upgrade to GitHub Pro or make this repository public to enable this feature.`
当前规则由工程流程和 CI 实施；服务器强制限制需要仓库所有者调整 GitHub 方案后配置，
并为版本机器人设置最小范围的主线写回许可。
