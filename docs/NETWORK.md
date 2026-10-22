# 受管 TCP/HTTP 的任务与连接预算

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

## CLI整任务时限和取消

`rustdx --timeout 10800 --cancel-file /tmp/my-run.cancel day ...` 从参数解析后到下载、解压、名单获取、因子、解析、CSV校验、数据库导入和独立回读使用同一绝对截止时间。默认10800秒，重试与新阶段不能延长；Ctrl-C/SIGTERM或取消文件出现会协作取消。任务模式下载失败后要求下次显式提供备选来源，不进入无法定时退出的终端输入。失败清单和归档仍允许写入；数据库未知/已确认但未验收时仍保留CSV。已开始的数据库写入不能因取消推断为未写入。

库调用方可用`operation::OperationContext::new(timeout, cancel_file)?.enter()`启用同样策略，克隆上下文并在工作线程`enter()`；内置TCP批次已继承。嵌套范围只能缩短父时限，并继承父取消。未进入上下文的库调用保留原行为。

TCP/HTTP读取每200ms检查取消；HTTP发送每次缓冲传输最多1秒，TCP建连及HTTP解析DNS/建连最多1秒。HTTP正文可持续超过轮询间隔，始终受总体时限限制。ureq的非semver transport接口用于轮询，故固定3.4.0，升级需重跑慢正文、TLS真实请求及取消验收。同步计算、文件系统调用、OS调度不能被抢占；在阶段边界/分块检查，时限不是硬实时承诺。

## 同机跨进程许可与观测

CLI默认在系统临时目录`rustdx_network_permits`保存OS锁许可，`--network-dir`或`RUSTDX_NETWORK_DIR`指定共享目录，`--network-limit`默认8。同一目录所有进程必须使用相同上限，capacity.lock不匹配即拒绝；需要不同预算可使用独立目录，但它们不共享总额。目录及锁文件不可在活动任务期间删除/替换；崩溃自动释放OS锁，文件保留以防inode替换造成双重许可。库可配置`NetworkPolicy.shared: Some(SharedNetworkPolicy { directory, max_connections })`加入该范围。

HTTP禁用空闲连接池，许可贯穿请求及完整正文。受管TCP与HTTP共用本进程/同目录的许可；Mongo工具每个子进程保守预约一个任务许可，其内部驱动/监视连接数由Mongo工具管理，不承诺每个实际socket都被计数。此机制不约束裸socket、其他程序、不同许可目录或跨主机出口。

清单resources记录本进程active/peak、许可等待微秒总量、许可超时、请求失败、空响应、最近至多1024个完整请求耗时的最近秩P95和样本数。TCP按完整帧、HTTP按完整调用统计，DNS/握手不是独立样本；这些是实际运行观测值，不能用离线微型测试推断生产P95或全市场限流阈值。

`tests/shared_network.rs`通过独立子进程和不同工作目录验证饱和及崩溃释放；`http_budget`验证慢正文不刷新截止时间、静默等待可取消、健康正文可超过轮询间隔；`cli_deadline`验证下载阶段完整CLI退出、失败清单及旧文件保留。跨主机仍需部署调度器协调。
