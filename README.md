# rustdx

当前源码版本：<!-- version:project -->2.0.1<!-- /version -->。数字由主线 CI 同步；注册表历史发布事实另列。

Rust 行情库、通达信日线处理工具和单机数据发布控制层。项目使用 Rust 2024，最低 Rust 1.89，采用同步 I/O、有界线程和显式资源预算。

当前源码覆盖 eltdx 3.3.0 的 26 个同码命令，并提供 day/lc/gbbq 文件解析、复权、指标、日历和日期化规则。26 个同码入口不代表所有增强业务契约已经验收：市场2、0feb响应日期、0547真实盘中主动推送及目标生产负载仍有明确边界。

当前核心包 `jelven-rustdx-complete` <!-- version:core -->2.0.1<!-- /version -->，导入名 `rustdx_complete`；CLI 包 `jelven-rustdx-cli` <!-- version:cli -->2.0.1<!-- /version -->，二进制 `rustdx`；控制层 `rustdx-runner` <!-- version:runner -->2.0.1<!-- /version --> 不发布。历史核心与CLI 2.0.0已发布到crates.io，源码身份和包摘要见[发布记录](docs/archive/release/PUBLICATION_20261025.md)，迁移见[行为说明](docs/release/MIGRATION.md)。

## 快速开始

从仓库根执行：

```sh
cargo +1.89.0 build --workspace --locked
cargo install --path apps/rustdx-cli --locked
cargo run -p jelven-rustdx-complete --example local_data
rustdx day /path/to/lday -o /path/to/stocks.csv \
  --report /path/to/collection.json
```

本地文件和运维命令默认不初始化行情TCP。HTTP下载/数据库连接按命令实际需要启动；TCP采集核验自己的冻结节点。`--tcp-warmup`显式测速和预热通用当日IP池；普通在线调用见[外部应用调用约定](docs/development/APPLICATION_INTEGRATION.md)。

从crates.io安装CLI：

```sh
cargo install --locked jelven-rustdx-cli --version 2.0.0
```

应用使用已发布核心库：

```toml
[dependencies]
rustdx-complete = { package = "jelven-rustdx-complete", version = "=2.0.0" }
```

源码集成可改用 `path = "/path/to/rustdx/crates/rustdx-core"`，并固定提交和锁文件。已发布1.x包不包含2.0.0的全部新增接口。

## 工程入口

```text
crates/rustdx-core/     核心库及就近测试、示例、基准
apps/rustdx-cli/        采集/文件导出/兼容数据库导入
apps/rustdx-runner/     单机账本、业务覆盖、版本发布、恢复
tests/integration/     外部接口测试包
tools/fuzz/            独立模糊测试工作区
config/                无凭据配置示例、版本化参考日历
deploy/                数据库与 Linux/systemd 部署模板
scripts/               开发、验证、测量、运维、发布工具
docs/                  当前文档、历史报告和固定证据
data/                  运行数据约定；实际产物不提交
```

完整职责和新增文件规则见[项目结构](docs/architecture/PROJECT_STRUCTURE.md)。

## 使用与生产边界

外部应用先阅读[共同调用规则](docs/development/APPLICATION_INTEGRATION.md)，再查[26命令签名、业务含义和约束](docs/reference/COMMAND_API.md)。证券身份包含市场，未知值保留资格，分页检查完成状态，批量逐项检查错误，订阅按同步状态消费，所有受管请求共用预算。

day CSV 的量保持手，标准行情与 runner 日线的量为股。累计收益因子与价格复权分别处理；财务候选值、聚合成交估算和竞价快照各有资格标记。

CLI 提供原子文件发布及 schema7 清单。数据库写入回执后仍须全导出键、全字段精确回读，达到 `verified` 后才允许清理 CSV。runner 进一步要求已核实事实覆盖、封存 release 和全部目标验证，然后以 SQLite 事务推进版本目录；消费者固定目录版本读取。

runner 当前来源可选本地 A 股原价日线或显式052b TCP日线，TCP范围与预算见[来源合同](docs/development/APPLICATION_INTEGRATION.md#9-tcp日线采集与runner发布)。真实全市场状态、Linux 部署、长期 SLO 和数据库服务端灾难恢复须在目标环境验收。

## 文档与验证

[文档总入口](docs/README.md) · [需求](docs/requirements/REQUIREMENTS.md) · [技术方案](docs/architecture/PRODUCTION_FUSION_PLAN.md) · [开发](docs/development/DEVELOPMENT.md) · [测试](docs/testing/VALIDATION.md) · [运营](docs/operations/OPERATIONS.md) · [运维](docs/runbooks/RUNBOOK.md) · [迁移](docs/release/MIGRATION.md)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDX_SKIP_INTEGRATION_TESTS=1 cargo test --workspace --all-features --locked
```

离线检查仍执行本地模拟服务。外部用例跳过不构成在线验收。当前验收范围见[验收矩阵](docs/testing/PRODUCTION_ACCEPTANCE.md)，历史报告见[归档索引](docs/archive/README.md)，本次目录迁移与删除清单见[目录迁移记录](docs/archive/release/PROJECT_RESTRUCTURE_20261024.md)。

生产工程入口：[目录职责](docs/architecture/PROJECT_STRUCTURE.md)、[需求追踪](docs/requirements/TRACEABILITY.md)、[文档治理](docs/development/DOCUMENTATION_GOVERNANCE.md)、[版本政策](docs/release/VERSION_POLICY.md)。已完成专项结果在archive；当前验收与待验范围保持分开。
