# 项目默认技能

在本仓库中进行 Rust 开发、调试、重构、代码审查或性能优化时，默认使用 `rust-engineer`，无需用户再次指定。

- 开始相关任务前，读取已安装技能：`/Users/bytedance/.trae-cn/skills/rust-engineer/SKILL.md`。
- 按任务需要读取技能目录下的 `references/` 文件。
- 项目使用 Rust 2024，最低 Rust 版本为 1.89；以仓库的 `Cargo.toml` 为准。
- 遵循项目现有的同步 I/O 和有界线程并发设计；引入异步运行时须有明确的任务需求。
- 修改 Rust 代码后，按变更范围执行格式检查、Clippy 和相关测试；无法执行时明确说明。
- 用户的明确要求和本仓库指令优先于技能中的通用建议。

# 工程与文档边界

- 根目录为虚拟 Cargo workspace；核心库位于 `crates/rustdx-core`，应用位于 `apps/`，外部集成测试位于 `tests/integration`，独立 fuzz 位于 `tools/fuzz`。
- 新测试与固定夹具就近放在所属 crate；原始协议/验收证据放 `docs/testing/evidence`，历史分析和实施报告放 `docs/archive`。
- 当前文档从 `docs/README.md` 导航；保持 FR-01 至 FR-18 追踪，跨接口消费规则统一维护在 `docs/development/APPLICATION_INTEGRATION.md`。
- 不改写冻结证据的字节、来源或历史摘要来满足新检查；路径迁移与新的验证结果另行记录。

# 版本与分支

- 每项任务使用独立分支和工作目录，通过 PR 合入 `main`。
- 开发、提交、本地合并和功能分支推送不改版本；合并最新主线可继承版本。
- `VERSION` 严格采用单数字 `a.b.c`，十进制进位，最高 `9.9.9`。
- 每次远程 `main` push 仅由 CI 递增一次，同次多提交仍一次，重跑复用 `run_id` 记录。
- CI 先同步自有包/前后端元数据、锁及当前文档，再执行全部质量检查，通过后打 `v<版本>` 标签。
- 新增元数据登记到 `config/release/versions.json`；不修改第三方/协议/数据版本及历史证据。
- 完整执行与权限边界见 [版本管理](docs/release/VERSION_POLICY.md)。
