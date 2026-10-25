# 工程目录与责任边界

<!-- lifecycle:registry -->
读者：开发/部署；责任：架构维护者。源码核验：2026-10-25；每90日及目录或依赖方向变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

源码核验：2026-10-25。供开发者、测试和部署维护者定位文件与依赖；责任为架构维护者。权威输入是Cargo工作区、实际源码和部署配置。每次目录/依赖改变及每90日复核；触发后未同步或超过周期即过期。治理登记见[文档治理](../development/DOCUMENTATION_GOVERNANCE.md)。

根目录是虚拟Cargo工作区，统一依赖、profiles和锁文件。核心库、采集CLI、单机控制层分别承担产品责任；不按未来微服务假设拆出空包。决策见[ADR-0001](decisions/0001-production-boundaries.md)。

## 1. 当前目录

```text
rustdx/
├── Cargo.toml / Cargo.lock             # 虚拟workspace、共享依赖与profiles
├── README.md / CHANGELOG.md / AGENTS.md
├── LICENSE / rustfmt.toml
├── crates/
│   └── rustdx-core/                    # jelven-rustdx-complete / rustdx_complete
│       ├── Cargo.toml / README.md / LICENSE
│       ├── src/
│       │   ├── tcp/stock/client/        # 同步协议、行情、分页/批量/语义
│       │   ├── file/                   # day、lc、gbbq
│       │   ├── data/                   # 标准数据与单位/价格口径
│       │   ├── calendar/ / limit/      # 有覆盖边界的日期与事实规则
│       │   └── operation.rs / pool.rs / cache.rs / indicators.rs
│       ├── tests/fixtures/             # 固定二进制、报文、独立期望
│       ├── tests/snapshots/            # Rust结构版本快照
│       ├── examples/
│       │   ├── usage/                  # 应用使用
│       │   ├── capture/                # 原始请求/响应采样
│       │   ├── validation/             # 显式在线验收
│       │   └── diagnostics/            # 调试及性能探针
│       └── benches/                    # 指标性能观测
├── apps/
│   ├── rustdx-cli/                     # jelven-rustdx-cli / rustdx_cli / rustdx
│   │   ├── Cargo.toml / build.rs / README.md / LICENSE-MIT
│   │   ├── src/cmd/                    # day、data-check、data-status、show
│   │   ├── src/io/                     # 分块、schema、前因子、数据库与精确回读
│   │   └── tests/ / examples/
│   └── rustdx-runner/                  # 单机控制层；publish=false
│       ├── Cargo.toml / README.md
│       ├── src/                        # Job、账本、覆盖、封存、数据库、消费/恢复
│       │   ├── pipeline/facts.rs       # 冻结事实校验与应有键；无发布副作用
│       │   └── database/capacity.rs    # 发送前容量/证据/回读预算
│       └── tests/                      # control_plane、wire_archive、startup
├── tests/integration/                  # 外部接口测试crate；publish=false
├── tools/fuzz/                         # 独立workspace与锁文件
├── config/                             # 无凭据配置及版本化参考数据
│   ├── calendars/
│   ├── runtime/                        # 无凭据env模板
│   ├── runner/                         # 严格Job及TCP来源模板
│   ├── maintenance/ / consumption/     # 事实维护及消费样例
│   ├── acceptance/                     # 验收矩阵/目标模板
│   └── release/ / documentation/       # 版本/门禁与文档登记
├── deploy/                             # 数据库/systemd/告警/恢复模板
│   ├── production/
│   │   ├── clickhouse/                 # 持久数据库模板
│   │   └── runner/                     # systemd/DB/告警
│   └── acceptance/                     # 独立临时数据库环境
├── scripts/                            # 可从仓库外调用的工程工具
│   ├── dev/ / ops/ / bench/
│   ├── validation/
│   └── release/
├── docs/
│   ├── README.md
│   ├── requirements/ / architecture/ / development/
│   ├── reference/                      # 稳定的26命令接口参考
│   ├── testing/evidence/               # 当前验证方法、验收矩阵、固定证据
│   ├── operations/ / runbooks/ / release/
│   └── archive/                        # 历史分析、实施与验收报告
├── data/README.md                       # 运行数据约定；实际产物忽略
└── .github/workflows/                   # CI、数据库验收、覆盖巡检
```

## 2. 依赖和构建规则

核心库、CLI、runner和tests/integration属于根工作区；默认构建三个产品包，完整工程检查显式用--workspace。fuzz保持独立工作区，不把libFuzzer依赖引入产品包。

在根目录调用示例、测试和基准时，显式使用包名，例如：

```sh
cargo run -p jelven-rustdx-complete --example local_data
cargo test -p jelven-rustdx-complete --test protocol_decoders
cargo bench -p jelven-rustdx-complete --bench indicators
cargo install --path apps/rustdx-cli --locked
```

45个核心示例在Cargo.toml显式登记名称和新路径，autoexamples=false；同名命令保持有效。采样及validation示例可能联网，不属于默认离线验收。crate内部相对测试路径以该crate为基准；工程级证据从CARGO_MANIFEST_DIR定位根目录。脚本从自身位置定位根目录。根目录没有src/lib.rs，不作为path依赖入口。

## 3. 文件放置和所有权

| 内容 | 位置 | 规则 |
| --- | --- | --- |
| 可复用协议、规则、计算 | crates/rustdx-core/src | 不依赖应用层账本/任务 |
| 一次采集任务及兼容导入 | apps/rustdx-cli | 应用参数、清单、输出与回读 |
| 生产业务发布与恢复 | apps/rustdx-runner | SQLite、覆盖、版本目录和恢复 |
| 核心/CLI/runner回归 | 各crate/tests | 就近固定样本，模拟数据标明来源 |
| 外部源集成检查 | tests/integration | 显式联网；skip不作为在线通过 |
| 配置和部署 | config、deploy | 无凭据示例入库，私有环境与数据卷另管理 |
| 当前文档 | docs按生命周期分类 | 稳定入口、一处定义、相对链接 |
| 历史报告与固定证据 | docs/archive、docs/testing/evidence | 历史结论保留轮次，原始字节和摘要不改写 |
| 运行产物 | 显式部署目录，参考data/README | 不进入Git或源码包 |

target、本机.cargo、fuzz语料、日志和私有配置属于本机/运行环境。发布crate保留源码、README和许可证，工程样本、部署附件及运行产物不进入核心发布包。

## 4. 迁移和清理

2026-10-24完成根工作区分层；本次完善示例分组、生产/验收部署、运行env和生命周期文档。完成阶段报告归archive，根审查材料归testing/evidence。逐文件摘要及兼容入口见[当前迁移说明](../release/STRUCTURE_MIGRATION.md)。冻结JSON中的旧路径仍是当时事实，解析器通过迁移映射定位。
