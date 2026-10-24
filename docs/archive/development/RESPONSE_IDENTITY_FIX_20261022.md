# P0响应正确性修复

日期：2026-10-22，会话环境日期。对应[项目评审O01](../architecture/CURRENT_PROJECT_REVIEW_20261022.md)。范围为受管7709响应身份校验、错误传播、原始留证与离线重放；采用现有同步I/O、线程和网络预算。

## 修复后的行为

旧、新命令现在通过同一响应匹配逻辑，核对16字节响应头中的前缀、opcode及消息号。检查覆盖K线、快照、财务、当日及历史成交、其他受管查询、握手、手动/空闲心跳和IP探测。

错配返回`io::ErrorKind::InvalidData`，连接不可复用，不进入该次自动重连重发。正常传输故障仍按既有配置和deadline重试。错配判断发生在解压和正文解析之前，完整帧路径的成功统计也在校验之后。

启用观察器时，完整错配响应保留原始头与wire正文，随后记录失败事件；正文未读完整时仅记录失败，不冒充完整响应。没有观察器时，在读到错配头后直接拒绝。错配头加截断正文仍返回身份错误，避免转成可重试的EOF或超时。

`tcp::decode_response(request, header, body, decompress)`新增请求匹配的离线入口；CLI的`tcp_evidence::replay`在解析前使用它。`decode_frame`保留原始长度核对和解压功能；只持有解压正文的独立解析函数无法验证响应头。裸socket研究读取仍由调用方负责。

## 代码与兼容性

| 位置 | 变化 |
|---|---|
| [frame](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/frame.rs) | 共用身份匹配及可识别的内部错误原因 |
| [Tcp](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/mod.rs) | 完整/只读头两条路径均匹配身份，错配不自动重试且连接失效 |
| [空闲心跳](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/heartbeat.rs)、[IP探测](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/ip/probe.rs) | 独立读取socket的内部路径使用相同匹配规则 |
| [离线入口](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/src/tcp/evidence.rs)、[CLI重放](file:///Users/bytedance/Documents/rustdx/apps/rustdx-cli/src/tcp_evidence.rs) | 解压/解析前匹配请求，原始归档仍可读取 |
| [核心专项回归](file:///Users/bytedance/Documents/rustdx/crates/rustdx-core/tests/response_identity.rs)、[归档回归](file:///Users/bytedance/Documents/rustdx/apps/rustdx-runner/tests/wire_archive.rs) | 合法正文配错身份、压缩、重试、池、心跳、探测及重放 |

公开数据DTO、字段单位和请求编码保持原合同。正常模拟响应原先只填长度且响应头身份全零，现已补正确前缀与请求回显；这些是测试服务的合成响应，冻结采样报文与历史摘要未改写。FR-07、协议说明、应用接入、迁移和CHANGELOG已同步。

## 证据与验证范围

修复前，新增回归复现旧K线收到异常前缀但合法正文仍返回`Ok(3)`；修复后同场景拒绝。该复现为本地故障测试，不是公共主站事故。

静态核对仓库中1152组具有请求和响应头的材料，覆盖24种命令。所有已检查材料均满足前缀`b1 cb 74 00`、请求opcode回显和请求字节1–4至响应字节5–8的消息号回显。文件可能在多个证据集重复保存，1152不代表独立采样次数，也不表示所有26项业务均已独立核实。

核心专项9项覆盖：

- 股票/指数K线、快照、财务、当日成交、增强历史和旧历史入口，压缩/非压缩与三类错配，共56组子场景；增强历史传输正文为明确合成材料。
- 只读响应头、完整raw正文和显式低层retry；配置自动重连时仍拒绝错配。
- 失效连接不归还空闲池，握手失败释放许可，探测错配节点不进入有效池。
- 空闲心跳错配失效；观察器保留完整坏帧并记录失败，请求统计不计为成功。
- 离线请求匹配先于解压；11组原有扩展命令冻结夹具逐字节重解。
- 有/无观察器的错配头加截断正文，维持身份错误而不重试。

runner归档新增回归证明三类错配在解析前被拒绝，旧解析结果不被修改。本轮工程命令、退出码、最终测试统计、变更文件摘要和已检查响应头摘要统一保存在[修复验证证据](../../testing/evidence/response-identity-fix-20261022.json)。

| 工程检查 | 最终结果 |
|---|---|
| Rust1.89格式检查 | 退出0 |
| workspace全部target/feature Clippy，`-D warnings` | 退出0 |
| 响应身份专项 | 9通过、0失败 |
| workspace全部target/feature离线测试 | 448通过、0失败、6忽略 |
| workspace文档测试 | 66通过、0失败、18忽略 |
| 严格文档构建，`RUSTDOCFLAGS=-D warnings` | 退出0 |
| `git diff --check`及受影响文档链接 | 通过 |

专项9项已包含在448项内，不重复累加。离线跳过外部服务后直接返回的旧用例仍可能显示ok；工程通过不能作为新实盘或数据库验收。

本轮验证使用macOS、Rust1.89、离线锁定依赖；外部服务用例按`RUSTDX_SKIP_INTEGRATION_TESTS=1`跳过。本地TCP服务与归档重放仍执行。未新增真实主站采样、数据库服务验收、部署、提交或发布。O02日期真实性、O03盘中推送及其他评审建议保留各自状态。
