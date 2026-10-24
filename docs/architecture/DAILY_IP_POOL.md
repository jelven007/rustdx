# 初始IP池与当日有效IP池

核心入口为 `rustdx_complete::tcp::ip::{IpPool, default_pool}`。保留原有40个
`STOCK_IP`地址作为默认初始值，包含39个7709端口和1个7719端口；初始池
与有效池是独立状态。集成项目可维护共享默认池，也可将独立的`IpPool`
传给`TcpConfig::ip_pool`，客户端批量连接池会继承同一个实例。

## 进程默认启动策略

`rustdx`和`rustdx-runner`本地文件与管理命令默认不初始化行情TCP。
显式`--tcp-warmup`在参数解析和任务上下文/网络策略初始化后调用
`tcp::startup::initialize_from_env()`：

1. 对完整初始池进行三轮、八并发、各阶段三秒的协议测速。
2. 原子发布按耗时排序的当日有效池；空池返回`NotConnected`。
3. 从有效池建立高层`Client`，完成PACK1握手并验证0004心跳。
4. 保留一个预热TCP，默认空闲30秒自动保活；开始后续09:00刷新。
5. 第一个匹配配置的`Tcp`/`Client`/`ConnectionPool`借出请求接收该连接。

初始化失败会阻止命令执行，失败后可重新初始化。同配置、初始列表和
当日测速快照的成功初始化幂等；变更配置/初始列表或快照到期后重新测全池。
带帧观察者的连接单独握手，以保证首帧归档。未领取的预热TCP在其他
受管TCP/HTTP申请许可时释放，释放和析构均在状态锁之外；连接上限为1时
也能继续工作。`startup::release_client()`可在创建子进程前主动释放。
`startup::snapshot()`返回成功初始化证据，包括当日结果、端点、连接ID及
心跳服务端日期；它不保证该连接仍未被业务领取或释放。

`--help`在参数解析阶段退出。旧`--no-tcp-startup`保留，与`--tcp-warmup`
互斥；它禁止入口预热，CLI后续显式TCP来源仍自行核验。runner配置的
`no_tcp_startup`另外禁止TCP来源。是否访问网络由实际命令决定。

二进制环境变量`RUSTDX_TCP_INITIAL_IPS`可整体覆盖初始地址，格式为逗号
分隔的`SocketAddr`，例如`119.97.185.59:7709,47.107.64.168:7709`；
IPv6使用`[地址]:端口`。未设置时保留40个内置节点；非法/重复地址报错，
没有未经验证的有效池导入。runner将初始地址和共享连接策略传给采集子进程，
在启动子进程前释放父进程预热连接；run配置可使用`no_tcp_startup: true`
显式离线，命令行同名开关也传给采集子进程。

Rust库无法安全地替集成应用执行其`main`，因此集成应用在启动入口调用
`startup::initialize()`或`initialize_with_config(&config)`；配置网络策略须
在它之前完成。未调用时，原有首次TCP调用补测行为仍可用。

## 初始池管理

| 操作 | API | 行为 |
|---|---|---|
| 查询完整列表 | `pool.list()` | 返回列表副本，不持有内部锁 |
| 查询地址 | `pool.get(address)` | 返回`Option<SocketAddr>` |
| 添加 | `pool.add(address)` | 已存在返回`Ok(false)`；新地址待测 |
| 删除 | `pool.remove(address)` | 返回是否删除，并立即撤下有效池中的旧地址 |
| 修改 | `pool.update(old, new)` | 不存在返回`NotFound`；重复新地址返回`AlreadyExists` |
| 批量替换 | `pool.replace(addresses)` | 原子校验并替换，不自动放行新地址 |

端口0、未指定IP、多播IP及重复列表会被拒绝。空初始池允许用于集成配置，
但不能建立行情连接。增删改查通过互斥锁共享，测速在锁外执行。测速期间
删除或修改的地址在发布快照时再次过滤，防止旧批次恢复已删除地址。

池状态在进程内存中；需要跨进程或重启保留配置时，集成项目持久化
`pool.list()`，并用`IpPool::new(saved_addresses)`恢复。`DailyIpPool`
与逐轮结果实现Serde序列化，供集成方导出监控和审计证据；不提供跳过
协议验证、直接导入有效地址的接口。

## 测速与调度

项目二进制显式预热或实际TCP来源才主动测速；集成应用调用启动API、
首次建立业务连接或调用`pool.refresh()`时自动启动一个后台同步线程，
也可显式调用`pool.start_daily_refresh()`。运行中的进程每天北京时间
09:00对当前完整初始池重新测速，包括此前失败的地址，不受业务连接
端点冷却状态影响。

固定参数：

- 每个节点三轮，每轮新建独立TCP连接，不在单轮内部重试。
- 每批最多八个测速连接并发，线程数量不会随节点数增长。
- TCP连接、PACK1握手、证券数量心跳三个阶段各自有三秒总体时限。
- 握手读取完整16字节响应头和响应体；心跳按现有协议解压并解析证券数量。
- 三轮全通过才进入有效池，按成功样本总耗时中位数升序排列。

“总耗时”包括建连、握手和一次心跳，不是单次网络RTT。部分轮次成功的
节点仅保留在`DailyIpPool.results`中，不进入`valid_nodes`。

测速连接与业务长连接分开限流：所有池和诊断共享探测总配额，默认进程
最多八个，同时受每批八工作线程限制。不占用业务`NetworkPolicy.max_connections`。
共享业务策略存在时自动在其`probes/`子目录协调同机进程的八个OS锁槽；
首次探测前可独立设置`ProbePolicy`。等待配额记录`ProbeStage::Quota`，
默认至多30秒，受外层deadline和取消约束；阶段耗时不包括该排队时间，
总elapsed包括排队。`probe_stats()`返回active/peak/waits/timeouts。
业务TCP、连接池和重连继续共享原有业务许可。

没有当日结果时立即补测，包括首次启动、错过09:00后启动和跨日后的
第一次使用。09:00之前生成的快照会在09:00再次测试。后台线程在进程
退出后不能运行；最后一个独立池引用释放后至多60秒内退出。

主动启动初始化使用独立预算：`ceil(节点数/8) × 3轮 × 3阶段 × 3秒`
加网络策略的连接预算、请求预算和5秒余量。默认40节点阶段预算135秒，
整个启动预算185秒，并受命令的整任务截止时间进一步收紧。并行池或其他
进程的配额争用也消耗这个预算，该公式不保证争用时能完成。
`startup::initialize_until(&config, deadline)`可指定绝对时限；等待初始化锁、
测速、客户端握手和心跳共用此时限。测速线程继承`OperationContext`，
读写以最多200ms轮询检查取消，阻塞TCP建连最长仍受阶段3秒约束。
超时/取消不会发布部分测速结果，也不会保留半初始化客户端。

`pool.refresh_until(deadline)`同样覆盖刷新锁等待及测速；`pool.refresh()`也
继承任务取消信号，可在初始池变更后立即重新测试。每日调度线程独立运行，
不继承一次命令的截止时间。未调用启动API时，首次业务连接等待后台补测
仍计入原有30秒连接预算；该等待超时不会取消独立后台测速。

## 当日连接约束

`Tcp::new`、`Tcp::with_config`、带观察者的连接、重连、客户端批量池和
裸`tcpstream_*`构造函数都仅从有效池选址。有效池为空返回`NotConnected`，
不会恢复初始列表或使用昨日快照。显式`TcpConfig::ip`也必须在所选初始池
中，并通过当日测试；不在初始池返回`NotFound`，未通过返回`NotConnected`。

新建连接继续在有效地址范围内轮换和故障转移，并跳过业务冷却节点。
已有`Tcp`在下一次库内请求前重新检查日期和成员资格：撤下节点后，
自动选址连接重连到当前有效节点，即使`auto_reconnect=0`也执行该策略
切换；显式指定的旧地址则返回错误。连接池归还及借出时排除已失效对象，
即使关闭心跳健康检查也不会复用撤下节点。

调用方通过`into_inner()`提取或通过`get_ref()`克隆裸socket后，后续直接
I/O由调用方控制，库无法拦截其跨日使用。需要当日池约束时应保持通过
`Tcp`/`Client`发起请求。独立`check_alive*`函数是诊断接口，扫描初始池，
不发布当日有效池。

节点的特殊命令观察独立于普通探活：`capability_observation`按端点/opcode
记录传输身份或正文通过，`capable_nodes(opcode, max_age)`按最新解码结果
筛选当前节点。使用条件和业务资格见[应用接入](../development/APPLICATION_INTEGRATION.md)。

## 集成示例

```rust,no_run
use rustdx_complete::tcp::{TcpConfig, ip::IpPool, startup, stock::Client};

fn main() -> std::io::Result<()> {
    let pool = IpPool::default(); // 保留40个初始节点
    let address = "119.97.185.59:7709".parse().unwrap();
    pool.add(address)?;
    assert!(pool.get(address).is_some());
    let config = TcpConfig {
        ip_pool: Some(pool),
        auto_reconnect: 2,
        ..Default::default()
    };
    let report = startup::initialize_with_config(&config)?;
    println!("{}: {}个有效节点", report.daily.date, report.daily.valid_nodes.len());
    let mut client = Client::with_config(&config)?; // 接收已验证的预热连接
    let quotes = client.quotes(&[(1, "600000")])?;
    println!("{}条行情", quotes.len());
    Ok(())
}
```

默认共享模式使用`ip::default_pool()`进行增删改查，不设置`ip_pool`字段
即可让所有默认客户端使用该池。`TcpConfig`新增`ip_pool`字段，使用
`..Default::default()`的调用保持兼容；完整结构体字面量需补`ip_pool: None`。

本地回归覆盖两个二进制默认启动、显式离线开关、启动失败阻止命令、
超时/取消、初始化和刷新锁限时、幂等/配置变化、预热交接、观察者完整握手、
单许可TCP/HTTP切换，以及三轮筛选、拒绝连接、心跳解析失败、八并发上限、启动补测、
09:00边界、北京时间跨日、空池、增删改查、测速中删除、指定地址校验及
旧连接/连接池的撤销。测试无需访问公共行情服务器。

2026-10-24验证：Rust1.89格式检查、全工作区Clippy拒绝警告、
411项工作区/文档测试通过，最终启动专项及真实采集子进程回归通过。
未覆盖初始列表的真实CLI启动约30秒完成，本次40节点中16个三轮全通过，
客户端连接`119.97.185.59:7709`并完成0004验证。逐轮协议参数、命令、
实际范围及外部测试跳过说明见[验证证据](../testing/evidence/tcp-startup-20261024.json)。
