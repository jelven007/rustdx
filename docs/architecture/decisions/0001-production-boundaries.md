# ADR-0001：单机产品与生产工程边界

<!-- lifecycle:registry -->
读者：架构/开发；责任：架构维护者。源码核验：2026-10-25；每90日及并发或协调模型变化复核。状态为源码已核验，目标现场另验；过期信号和权威输入见[文档治理](../../development/DOCUMENTATION_GOVERNANCE.md)登记。
<!-- /lifecycle -->

状态：已采纳，2026-10-25；源码结构已实施，目标生产部署仍按验收矩阵核实。读者为架构与开发维护者，责任为架构维护者。权威输入是根Cargo.toml、各产品包和部署配置；每次依赖方向或部署模型变化及每90日复核。

当前产品采用同步I/O、有界线程、受控子进程和本地Unix持久盘。核心协议与计算可独立复用，CLI承担单次数据处理，runner承担业务发布状态；全市场事实、调度、凭据、数据库和备份由部署环境提供。

| 选择 | 判断 |
| --- | --- |
| 保留虚拟workspace、核心库与两个应用 | 采用；保留包名/接口、独立职责和现有回归 |
| 按将来服务拆成多个空crate或服务目录 | 不采用；没有远程协调、独立交付或故障模型需求 |
| 所有示例、部署及阶段材料继续平铺 | 不采用；使用、在线采样、临时验收和正式操作难以区分 |

核心禁止依赖应用账本和部署环境；CLI复用核心，runner调用固定身份CLI并使用核心模型。tests/integration只依赖产品，fuzz保持独立workspace。运行数据、私有env、target及本地工具不进入源码交付。

示例按usage/capture/validation/diagnostics分组并显式注册Cargo名称。deploy/production使用持久目标模板，deploy/acceptance仅使用隔离临时资源。config按runtime、runner、maintenance、consumption、acceptance、release和documentation分工。

代价是维护显式示例表和路径映射；收益是干净检出即可定位产品、验证和现场操作。冻结历史不改字节，新验证单独记录。多主机、异步运行时或新业务契约需要新ADR，明确协调、失效写入隔离、预算和验收后再改变本决策。

落实检查：Cargo metadata/全目标编译、文档治理与链接、实际包解包消费、证据摘要。[结构](../PROJECT_STRUCTURE.md)、[迁移](../../release/STRUCTURE_MIGRATION.md)和[验收](../../testing/PRODUCTION_ACCEPTANCE.md)分别说明当前目录、升级操作和待验范围。
