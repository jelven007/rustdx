# 受管 TCP 的进程预算

更新日期：2026-10-22；对应源码5fb5b70及后续优化，尚未发布新crate版本。开发调用见[DEVELOPMENT.md](DEVELOPMENT.md)，网络排障与部署限制见[RUNBOOK.md](RUNBOOK.md)。

`Tcp::new/with_config`、Client 主连接和 ConnectionPool 都使用同一进程预算，默认最多8条，包含正在建立、借出、空闲和重连的连接。重连先 shutdown 旧 socket，在原许可内握手；失败连接及丢弃的池槽位释放许可。同一 Client 的批次通过有界等待串行化，批次内固定 worker 并行，批次完成不保留连接，防止空闲批量池占住其他 Client 的配额。

在首次创建 Tcp 或读取 network_policy/network_stats 之前配置一次：

```rust,no_run
use rustdx_complete::tcp::{configure_network, NetworkPolicy, network_stats};
use std::time::Duration;

configure_network(NetworkPolicy {
    max_connections: 6,
    batch_timeout: Duration::from_secs(180),
    ..Default::default()
})?;
println!("{:?}", network_stats());
# Ok::<(), std::io::Error>(())
```

重复或晚于初始化的配置返回 `AlreadyExists`，不会在运行中改变现有配额。默认预算：获取进程许可5秒、全部服务器连接与握手30秒、一次完整帧含重连/重发/退避15秒、分页含空结果复查120秒、批次含排队与连接300秒。`TcpConfig.timeout` 仍限制每次服务器尝试与单次读写；总体时限与它取更紧的限制。`get_connection_timeout` 现在覆盖等待、建连和探活。

自动选服轮换起点，并在连接/请求失败后冷却10秒；全部处于冷却期时返回 WouldBlock，避免反复攻击故障端点。显式指定 `TcpConfig.ip` 表示调用方选择该服务器，仍会尝试该地址。观测包含当前/峰值连接数、许可超时、完整帧请求和失败数量、各端点操作/失败次数及剩余冷却时间，不包含业务解析错误。

`Tcp::with_timeout` 可进一步收紧一组请求。读取响应头和正文都按剩余时间逐段更新 socket 超时，缓慢持续发送字节不会刷新总体时限。同步用户闭包中的计算不能被抢占，返回后仍检查超时；OS 超时粒度和线程调度可能带来少量误差。

预算边界是受管 Tcp：旧 `tcpstream_*` 返回裸 socket，`into_inner` 提取句柄会释放许可，`get_ref` 后自行克隆/发请求也需调用方管理。这些兼容接口不承诺进程连接限制。普通自定义连接池若长期保留空闲连接会占用全局预算；可关闭池或设置 `max_idle=0`。上限1只能容纳单 Client 主连接，批量查询需要额外可用配额。

本地 `tests/network_budget.rs` 独立进程验收主连接与池共用预算、饱和等待、重连不增加峰值、重复批次释放配额、握手及缓慢正文的总体时限。内置 governor 回归另测轮换和冷却，不需要外部行情服务器。

这些时限适用于受管TCP及对应分页/批次，不是day下载/解析/数据库完整流水线的任务总时限，也不约束独立进程或其他HTTP客户端。CLI下载与数据库时限分别见[开发说明](DEVELOPMENT.md)和[导入说明](IMPORTS.md)；真实P95、出口并发和跨主机任务由部署环境观测与协调。
