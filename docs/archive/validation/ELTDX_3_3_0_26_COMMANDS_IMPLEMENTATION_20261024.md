# eltdx 3.3.0：26 命令对比与四项补齐

核对日期：2026-10-24。实现位于当前工作区，包含此前未提交的修改；核心库仍为 1.12.2，Rust 2024 / MSRV 1.89，本轮未发布 crate。

**26 项均已有同 opcode 的请求、解码和公开调用链，缺失 opcode 为 0。** 相对 3.2.2 的 22 项，3.3.0 增加 `02c5 / 02cf / 02d0 / 052b`。前三项在 rustdx 中已有旧实现，因此本轮开始时实际是 **25 同码、1 缺码**；本轮新增 `052b`，并将前三项完善为 checked 请求和保留完整字段的 owned 结果。26 包括 2 项会话命令、24 项业务命令；同码覆盖不代表 26 项的全部业务合同均已验收。[cite:1](#cite-1)[cite:2](#cite-2)

## 1. 版本与统计口径

基线为 PyPI 的 **eltdx 3.3.0 发布包**，上传时间 `2026-10-21T11:23:18.481691Z`。本轮重新下载并核对源码包 SHA256：

```text
16fad43e518618728a2ac5c0a5d0abf0a3bcce3ccb12daccec55d8fa912d9a9f
```

发布包中的 `request.rs / unit.rs / resources.rs / corporate.rs / delisted_bars.rs` 与此前固定的 3.3.0 源码逐字节一致。以两个版本的 `SUPPORTED_COMMAND_CODES` 为依据，3.2.2 的 22 项全部保留，新增且仅新增上述四项。本轮不以旧报告中的历史 9/4/9 计数作为当前状态。[cite:1](#cite-1)

| 范围 | 本轮前 | 本轮后 |
| --- | ---: | ---: |
| 3.3.0 注册项 | 26 | 26 |
| 同码调用链 | 25 | 26 |
| 相近命令替代 | 0 | 0 |
| 缺失 opcode | 1：052b | 0 |
| 本轮重点完善 | 02c5、02cf、02d0 的旧实现 | 三项 checked/完整 DTO，加 052b 专用实现 |

“同码”必须存在实际构包、解码与可调用入口；并非仅在源码中出现命令常量。旧实现的参数/字段差距另列，不能将该统计解释为兼容率或实盘验收率。

## 2. 26 项当前对照

下表“已有”指当前工作区已有实现，本轮对其调用链进行源码核对；本轮真实服务器验证范围限定为四个新增注册项。其他命令的验证沿用各自报告，不累计为本轮新增样本。[cite:2](#cite-2)[cite:5](#cite-5)

| # | opcode | eltdx 能力 | rustdx 当前公开入口 | 本轮动作与合同边界 |
| --- | --- | --- | --- | --- |
| 1 | 0004 | 心跳 | `Tcp::heartbeat / Client::heartbeat`，空闲心跳 | 已有同码；服务器日期/原始字段。空闲保活由连接策略控制 |
| 2 | 000d | 会话握手 | `Tcp/Client::with_config`、自动重连初始化 | 已有同码；未完整暴露 eltdx 的主站/会话信息模型 |
| 3 | 000f | 股本变迁 | `Client::xdxr / Xdxr` | 已有单证券旧模型；eltdx 多证券报文及增强类别/raw 模型仍有差距 |
| 4 | 0010 | 批量财务 | `Client::finance / FinanceRequest / FinanceSnapshot` | 已有同码及逐字段财务语义；直接多证券报文、部分字段依据另列 |
| 5 | **02c5** | 板块文件元数据 | **`block_metadata / block_metadata_batch`** | **完善**：40 字节名称校验、size、MD5、保留槽与响应尾部 |
| 6 | **02cf** | F10 栏目目录 | **`company_info_directory / *_batch / f10_categories`** | **完善**：完整 152 字节记录、GBK 名称与文件名、原始字段/尾部 |
| 7 | **02d0** | F10 栏目正文 | **`company_info_chunk / company_info_text / *_batch / f10`** | **完善**：checked 单块、完整范围分块、拼齐后解码 GBK |
| 8 | 044d | 新证券目录 | `security_directory / Client::stocks` | 已有 37 字节记录、u32 分页和精度缓存；普通目录接口仍限制沪深 |
| 9 | 044e | 证券数量 | `Client::stock_count / SecurityCount` | 已有市场总数量；不将总数量当作 A 股数量 |
| 10 | 0452 | 特殊限制表 | `special_limits / special_limits_all` | 已有同码及分页；表中缺席不表示不存在限制 |
| 11 | 051b | 分时副图 | `intraday_aux / *_batch` | 已有 selector 和双序列；未知 selector 不赋予已确认指标语义 |
| 12 | **052b** | 退市日线 | **`delisted_bars / delisted_bars_scan / delisted_bars_all / *_batch`** | **新增**专用请求、千分元 OHLC、整数股数、TDX 金额和原始尾部 |
| 13 | 052d | 普通/指数 K 线 | `bars / index_bars / *_range / k / *_batch / k_adjusted` | 已有同码；仍采用既有请求，本地复权不覆盖全部服务器/N 周期/定点复权合同 |
| 14 | 0537 | 当日分时 | `minute_series / minute` | 已有同码、均价/分钟标签与目录精度；未回显的日期不伪造为服务器日期 |
| 15 | 053e | 五档快照 | `quotes / quotes_with_metadata` | 已有目录精度驱动五档；兼容 QuoteData 不覆盖 eltdx 全部 raw 槽 |
| 16 | 0547 | 游标刷新/推送 | `subscribe_depth / QuoteSubscription` | 已有同步读推送、基线与状态恢复；真实盘中主动推送仍待有效交易窗口 |
| 17 | 054b | 分类行情/排序 | `category_quotes / category_quotes_all` | 已有分类排序及排行分页；未加载的市场精度保留未知 |
| 18 | 054c | 一档快照 | `snapshots / snapshots_batch` | 已有独立请求与完整 raw 页；按当前客户端边界分片 |
| 19 | 056a | 竞价过程 | `auction_series / *_all / *_batch` | 已有日期/符号解码；快照过程不当作正式开盘逐笔，侧别验收另列 |
| 20 | 06b9 | 文件块 | `BlockInfoChunk / Client::block / get_block_info` | 已有板块下载；旧 100 字节文件名路径不等于 eltdx 300 字节通用资源合同 |
| 21 | 0fb4 | 指定日分时 | `Client::history_minute` | 已有同码；增强均价/时间/批量合同按现有模型另列 |
| 22 | 0fc5 | 当日成交明细 | `transaction / trade_page / transaction_all` | 已有明细、分页和事件模型；源数量及估算金额不混为交易所精确逐笔 |
| 23 | 0fc6 | 增强历史明细 | `history_transaction / history_trade_page / enhanced_history_transaction / *_all` | 已有增强专用解码与分页；保留笔数/原始槽，完整性验收限定到主站样本 |
| 24 | 0fd1 | 小走势图 | `sparkline / sparkline_batch` | 已有同码；window 不直接等于返回点数，无真实时间戳不合成交易事实 |
| 25 | 0feb | 近期分时 | `recent_minutes / recent_minutes_batch` | 已有同码及非空解码；日期真实性对照存在冲突，不能用请求日期代替响应证明 |
| 26 | 0ffc | 日资金流向 | `money_flow_daily / *_batch` | 已有分档/raw 和两套净额算术；原始金额单位的外部依据按专项报告判断 |

全部入口通过 `tcp::stock` 导出。四项新/完善代码见 [resource_metadata.rs](../../../crates/rustdx-core/src/tcp/stock/resource_metadata.rs)、[company_info_request.rs](../../../crates/rustdx-core/src/tcp/stock/company_info_request.rs)、[delisted_bars.rs](../../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs)、[Client 接入](../../../crates/rustdx-core/src/tcp/stock/client/version_330.rs)。请求与结果字段的最终定义以这些源码和固定摘要为准。[cite:2](#cite-2)

## 3. 四项实现细节

### 02c5：完整板块元数据

`BlockMetaRequest::new` 接受 trim 后 1–40 字节 ASCII 名称，拒绝 NUL、非 ASCII 和过长输入，不再依赖旧构造器的静默截断。响应至少 38 字节：

```text
size:u32@0 + reserved:u8@4 + MD5:ASCII[32]@5 + terminator:u8@37 + trailing
```

`BlockMetadata` 返回 size、MD5 文本、32 字节原值、保留字节、终止字节、尾部和完整原始正文。MD5 从偏移 **5** 读取；保留服务器 hash 不表示已经对下载文件执行 MD5 验算。`get_block_info` 的元信息阶段也迁移到该 checked 路径，下载块继续采用既有 06b9 实现。[cite:2](#cite-2)[cite:3](#cite-3)

### 02cf：目录与 GBK 文件名

请求正文为 market:u16 + 六位数字 code + 4 个零字节。每条响应为 name:64 + filename:80 + start:u32 + length:u32，共 152 字节。`CompanyInfoDirectory` 保存请求、全部目录项、名称/文件名原值、尾部与来源。`f10_categories` 保持原返回类型，通过 `to_legacy` 映射新结果。[cite:2](#cite-2)

这两个 F10 请求和 052b 显式接受市场 0/1/2，与基线的 market 表达范围一致；本轮实际网络样本只验证市场 0/1。普通行情请求的市场边界未由此扩展。代码 trim 后必须为六位 ASCII 数字，不接受猜测的 `T` 前缀或非数字证券码。

### 02d0：完整正文与字符边界

请求正文 102 字节，文件名使用 GBK 编码，按**编码后字节数**限制为 80 字节；拒绝 NUL、不可编码字符、零长度以及 offset+length 的 u32 溢出。单次请求仍可表达大于一帧的 length，完整读取应使用 `company_info_text`，其默认 30,000 字节分块，分块上限为 60,000。

响应为 10 字节原始头 + declared_length:u16 + 正文字节 + 合法尾部。declared_length 不得超过请求长度，截断报错。`CompanyInfoTextChunk` 保留原始字节；单块可能结束于 GBK 双字节字符中间，因此完整栏目先拼字节再解码，保留正文中的换行和 NUL，不按文本长度推进偏移。

`company_info_text` 即使遇到短块也按实际字节数继续；未达到指定长度先返回空块、耗尽块预算、跨连接/跨主站/北京时间观察日改变或最终 GBK 无效，均返回错误。完整结果才有 `complete=true`。默认每次完整读取最多 64 MiB、4096 块；`Client::f10_with_options` 对全部栏目共用总时限、总字节和总块预算，零长度栏目保留空字符串，不发送非法的零长度请求。原 `Client::f10` 已迁移到该实现。[cite:2](#cite-2)[cite:3](#cite-3)

### 052b：退市专用日线

请求正文为 market:u16 + code:[6] + start:u16 + count:u16，count=1–800。该命令只能表达日线分页，没有 category、日期筛选或复权参数。它返回不复权日线，不能调用 052d 解码器：**052b 成交量是变长整数股数，052d 的量槽布局不同**。

每条记录解析 YYYYMMDD:u32、四个有符号变长价格差、变长整数成交量、TDX 金额 u32 与四字节未知尾部。价格按千分元保存 i64：open 相对本页上一条 close，close/high/low 相对本条 open；每页第一条价格基数为 0，上一收盘字段为 None。日期为北京时间零时；低层页保留线序，集合结果按日期升序去重。

模型保存四个原始价格差、四个整数价格、日期原值、volume_raw/shares/lots、amount_raw/amount、未知尾部、逐条原始字节及页级完整正文。验证日期、OHLC 关系、整数加法溢出、股数 0..u32::MAX、截断、超量和额外字节。金额使用独立 TDX 指数算法，覆盖全零 sentinel、低指数和高字节边界；本轮未修改全仓其他金额解码器。[cite:2](#cite-2)[cite:3](#cite-3)

`delisted_bars_all` 从 start=0 开始；`delisted_bars_scan` 支持自选起始偏移。两者按**实际返回条数**推进，短页继续直到空页。页数/原始记录预算和 u16 偏移耗尽返回已保留结果及明确 termination，`complete=false`；相同日期重叠保留原始页并合并完全相同的 OHLC/量/额/尾部；数据冲突或整页无新增日期报错。`complete=true` 仅代表从指定偏移读取到了该主站的空页，不保证交易所历史完整，也不保证原子快照。

## 4. 调用方式

```rust,no_run
use rustdx_complete::tcp::stock::{
    Client, CommandScanOptions, CompanyInfoReadOptions, CompanyInfoTextRequest,
};

fn main() -> std::io::Result<()> {
    let mut client = Client::new()?;
    let meta = client.block_metadata("block_gn.dat")?;
    println!("size={}, md5={}", meta.size, meta.md5);

    let directory = client.company_info_directory(0, "000001")?;
    if let Some(item) = directory.entries.iter().find(|item| item.length > 0) {
        let section = client.company_info_text(
            CompanyInfoTextRequest::new(
                0, "000001", &item.filename, item.start, item.length,
            )?,
            CompanyInfoReadOptions::default(),
        )?;
        println!("{}: {} bytes", item.name, section.content_raw.len());
    }
    let all_f10 = client.f10(0, "000001")?;
    let archive = client.delisted_bars_all(
        0, "000038",
        CommandScanOptions { max_records: 20_000, ..Default::default() },
    )?;
    println!("{} bars, complete={}", archive.records.len(), archive.complete);
    Ok(())
}
```

批量入口：

| 方法 | 输入与结果单位 |
| --- | --- |
| `block_metadata_batch(filenames, max_parallel)` | 每文件一项元数据 |
| `company_info_directory_batch(stocks, max_parallel)` | 每证券一项目录 |
| `company_info_text_batch(requests, options, max_parallel)` | 每指定文件字节范围一项完整正文 |
| `delisted_bars_batch(requests, max_parallel)` | 每指定 start/count 一页 |
| `delisted_bars_all_batch(stocks, scan_options, max_parallel)` | 每证券一项分页集合 |

批量返回 `io::Result<Vec<io::Result<T>>>`，维持输入顺序及独立错误；最多 10 万项，复用现有 Client 批次锁、同步线程与连接池，受进程连接预算约束。非法调用参数在池请求前拒绝，超时与取消继承现有 OperationContext。原始正文在 JSON 中跳过，完整字节由捕获观察器另存。

## 5. 真实样本与独立核验

两台服务器 `115.238.56.198:7709`、`180.153.18.170:7709` 均先通过当日 IP 池三轮探测，再使用受管连接。捕获时间为 2026-10-24 06:23:33–34 UTC，北京时间周六休市。每台记录 49 组帧，其中 1 组握手、1 组 02c5、2 组 02cf、22 组 02d0、23 组 052b；合计 **98 组完整帧，96 组四命令业务帧**。[cite:4](#cite-4)

| 样本 | 每台结果 | 本轮验证 |
| --- | --- | --- |
| `block_gn.dat` | 返回 size 和 32 字节 MD5 | 正确偏移、原始保留字段/尾部 |
| 深市 `000001` F10 最大栏目 | 311,393 字节，11 块 | 按目录实际 filename/start/length 分块，无截断 |
| 沪市 `600000` F10 最大栏目 | 323,277 字节，11 块 | 同上；两节点原始正文完全相同 |
| 深市 `000038` 退市日线 | 5,072 条；1994-08-08..2023-07-11 | 800×6 + 272 + 空页，实际偏移 5072 确认空页 |
| 深市 `002087` 退市日线 | 4,215 条；2006-11-30..2024-06-27 | 800×5 + 215 + 空页 |
| 沪市 `600001` 退市日线 | 2,752 条；1998-01-22..2009-12-15 | 800×3 + 352 + 空页 |

另外每证券每节点请求 count=1，验证页长与页首价格基数。因此独立核对的日线记录合计 **24,084 条**，包括每台 12,039 条完整扫描及 3 条单页重采样；唯一证券历史条数不能加倍计为 24,084 条。三个证券的全部标准化记录在两节点逐条一致。

独立脚本仅使用 Python 标准库，重读 request/header/wire/payload，验证长度、前缀、opcode/消息号、解压与正文一致，然后分别按 LE、sign/magnitude、GBK、TDX 指数重算全部已解码字段，核对 JSON、分页偏移、完整范围和终止状态。44 个 F10 正文块拼齐后再次解码，两节点内容摘要：

```text
000001: 0dc3504dd0943a3482f9c9eb65711a625f35b5f31002783527b4dab8245487ce
600000: 37505468cc3edd8fcd8f4bd59824c8f11a304a2c65af5cc8f1ab6de793cd80da
```

每个原始文件和 DTO 均保存 SHA256。第三方 SDK 未作为运行时依赖，也未执行其业务调用；上游源码用于确认布局，本库真实网络响应与独立脚本用于验证实现。[cite:4](#cite-4)

## 6. 回归与兼容边界

专项测试涵盖完整构包、非法参数、每个截断边界、响应超量/尾部、GBK 文件名与跨块字符、大栏目、空/短块、零长度栏目、052b 日期/OHLC/成交量/溢出、金额零与低指数、短页/重叠/冲突、预算/偏移、全部新增批量入口及 98 组真实帧离线重放。公共来源检查测试覆盖连接、主站与北京时间跨日改变。[cite:3](#cite-3)

Rust 1.89.0 下，`fmt --all --check`、workspace 全 targets/features 的 `Clippy -D warnings`、workspace 的 `rustdoc -D warnings` 均通过；14 项专项测试及公共来源检查通过，完整 workspace 回归 **498 项通过、0 失败、24 项忽略**，含文档测试。测试环境关闭非本轮主动执行的外部集成服务测试。[cite:3](#cite-3)

首轮 workspace 回归中一项模拟服务器只接收首批连接，未体现连接池 `max_idle=0` 的每批释放策略；修正为接受主连接和四批各两条连接后，专项与完整回归均通过。首次失败日志独立保留，未放宽生产校验。两个服务器的真实样本通过不代表市场 2、全部退市证券、所有 F10 文件或全部 26 项业务合同均已验证。

旧低层 `BlockInfoMeta / CompanyInfoCategory / CompanyInfoContent` 保留兼容；其直接构造/解析行为不自动获得新接口的全部校验。推荐新调用使用 checked 请求和新 DTO，高层 `f10/f10_categories/get_block_info` 已切换相关路径。其余 22 项的增强参数、主站会话、财务/股本/通用文件模型和真实盘中推送边界按各自证据处理，本轮没有据同码覆盖宣称全部兼容。

## 7. 依据与复现

<a id="cite-1"></a>**[1] 发布包与注册表。** [PyPI 3.3.0](https://pypi.org/project/eltdx/3.3.0/)、[发布源码包](https://files.pythonhosted.org/packages/ce/ed/d28b09bc33829a43169af4ba8929b41da4f71e9fde04caccec7ca48e76a4/eltdx-3.3.0.tar.gz)、[3.3.0 注册源码](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/request.rs)、[3.2.2 注册源码](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/request.rs)。本轮发布包源码校验及四项差集见[机器证据](../../testing/evidence/eltdx-3.3.0-26-commands-implementation-20261024.json)。

<a id="cite-2"></a>**[2] 本库源码。** [公开导出](../../../crates/rustdx-core/src/tcp/stock/mod.rs)、[四项 Client 接入](../../../crates/rustdx-core/src/tcp/stock/client/version_330.rs)、[元数据](../../../crates/rustdx-core/src/tcp/stock/resource_metadata.rs)、[F10](../../../crates/rustdx-core/src/tcp/stock/company_info_request.rs)、[退市日线](../../../crates/rustdx-core/src/tcp/stock/delisted_bars.rs)。机器证据固定本轮相关文件摘要和 26 项入口。

<a id="cite-3"></a>**[3] 回归。** [四项专项及实帧重放测试](../../../crates/rustdx-core/tests/eltdx_330_commands.rs)、[公共来源检查](../../../crates/rustdx-core/src/tcp/stock/command_request.rs)、[最终检查日志](../../testing/evidence/eltdx-3.3.0-commands-20261024/checks)。

<a id="cite-4"></a>**[4] 本轮真实网络。** [live-a](../../testing/evidence/eltdx-3.3.0-commands-20261024/live-a/independent-verification.json)、[live-b](../../testing/evidence/eltdx-3.3.0-commands-20261024/live-b/independent-verification.json)、[捕获示例](../../../crates/rustdx-core/examples/capture_eltdx_330_commands.rs)、[独立解码脚本](../../../scripts/validation/verify_eltdx_330_commands.py)。

<a id="cite-5"></a>**[5] 既有能力与验收边界。** [四条替代命令迁移](../development/FOUR_COMMANDS_MODERNIZATION_20261024.md)、[九命令实现](../development/NINE_COMMANDS_IMPLEMENTATION_20261024.md)、[财务与报价语义](../development/FINANCE_QUOTE_SEMANTICS_IMPLEMENTATION_20261024.md)、[真实合同验证](LIVE_COMMAND_CONTRACT_VALIDATION_20261024.md)、[0547 主动推送验证](QUOTE_PUSH_VALIDATION_20261024.md)。这些是既有工作，不作为本轮新增实测数量。

```bash
export CARGO_HOME="$PWD/.cargo"
export RUSTUP_HOME="$PWD/.cargo/rustup"
export RUSTDX_SKIP_INTEGRATION_TESTS=1
.cargo/bin/cargo +1.89.0 fmt --all --check
.cargo/bin/cargo +1.89.0 clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
.cargo/bin/cargo +1.89.0 test --workspace --all-features --offline --locked --no-fail-fast
python3 scripts/validation/verify_eltdx_330_commands.py \
  docs/validation/evidence/eltdx-3.3.0-commands-20261024/live-a \
  docs/validation/evidence/eltdx-3.3.0-commands-20261024/live-b
```

新网络捕获使用示例所列命令和全新输出目录；已有证据目录拒绝覆盖，复现不需要重连真实服务器。
