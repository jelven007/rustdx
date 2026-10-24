# 通达信近期协议与接口更新研究

检索及资料保存日期：2026-10-23。研究范围：“2026年9月份之后”严格按2026-10-01至10-23处理，同时保留9月更新及“21个7709命令”的历史来源。rustdx源码基线：`408e30eb172704070e384fb35fb0e56549a1fbc5`。

**已获取官方最新接口资料和社区最新协议实现资料。官方10月23日更新为tqcenter/tdxaidata 1.3.1，官网注明“仅内测版支持”；tdxaidata 1.3.1已上传PyPI。社区eltdx 3.3.0的7709清单及源码实际包含26个命令。本次检查的公开官方资料中未找到完整的新版7709逐字节规范，也未找到能证明服务端在9月后统一替换协议的公告。**

本次交付是公开资料研究及版本固定，不是线上协议验收。原始安装包仅保存、校验和静态阅读，没有安装、加载动态库或调用行情服务器，没有修改Rust实现。

资料入口：[资料说明](../../testing/evidence/tdx-protocol-20261023/README.md)、[研究证据JSON](../../testing/evidence/tdx-protocol-research-20261023.json)、[全部源文件摘要清单](../../testing/evidence/tdx-protocol-20261023/SOURCE_MANIFEST.json)。2026-10-24裁剪三份附件的无关来源内容，分别标明原摘要与当前衍生件摘要；协议结论和固定解码样本保留。

## 1. 已核实的版本与更新日期

| 来源 | 日期及版本 | 已核实变化 | 日期/证据口径 |
| --- | --- | --- | --- |
| 官方TdxQuant | 2026-09-04 | 全面支持tdxaidata后台数据模式；数据服务Key、安装入口及部分扩展字段 | [官网更新说明][official-versions]，9月背景 |
| 官方TdxQuant | 2026-09-20，tdxaidata 1.1.1 | batch类接口、`query_history_trade`及修正 | 官网日期；安装包另记录1.1.0的9月18日功能，不混成同一天 |
| 官方TdxQuant | 2026-09-30，1.2.2 | `get_sector_list_ds`、`get_domain_sorthq`、扩展市场支持 | 官网最近一条未附“仅内测版支持”的更新 |
| 官方TdxQuant | 2026-10-16，1.3.0 | `get_stock_mark`、`formula_by_src/file`；tdxaidata批量公式、历史竞价、`get_global_data` | 官网明确仅内测版支持；本次PyPI元数据中没有1.3.0发布条目 |
| 官方TdxQuant / tdxaidata | **2026-10-23，1.3.1** | 综合信息、扩展市场整体行情、公式新周期及数据源、工作目录修复 | 官网明确仅内测版支持；PyPI最早上传11:37:45 UTC，源码包11:37:49 UTC |
| 社区eltdx | 2026-08-16，3.0.0 | Rust重写发布说明写明“21个7709命令” | [历史发布说明][eltdx-3-0]及PyPI；21是社区旧版本计数 |
| 社区eltdx | 2026-09-01，3.1.0 | 接入`0x0FFC`日资金流向 | 包内更新说明；PyPI上传为8月31日17:18 UTC，即UTC+8的9月1日 |
| 社区eltdx | **2026-10-05，3.2.3** | 接入`0x052B`退市股票不复权日线、自动分页与批量入口 | 包内发布说明与PyPI记录 |
| 社区eltdx | **2026-10-21，3.3.0** | 接入`0x02C5/0x02CF/0x02D0`；新增7727、7615、离线文件、财报及CLI能力；K线`bar_time` | [发布说明][eltdx-3-3]与校验过的3.3.0源码包 |
| 社区injoyai/tdx | 2026-10-12、10-13；v1.0.0于10-13发布 | 成交时间按中国市场UTC+8解码，生成K线保留该时区 | [Go包版本][go-package]及两条源码提交；属于解析修复 |

软件发布日期说明该软件何时接入或修复某能力。它不等于命令码在通达信服务端首次出现的日期。尤其eltdx 3.3.0新增的F10及板块元信息命令早已存在于rustdx。

其他检索结果：mootdx最新PyPI版本0.11.7发布于2024-05-04，tdxpy 0.2.7发布于2024-03-10，不能称为2026年9月后的更新。tdx-cli 0.1.7发布于2026-09-27，仅作为9月背景保存；其元数据链接的GitHub仓库本次返回404，不能据此核实后续提交。

## 2. 官方最新资料：1.3.1接口与7709的边界

### 2.1 三种接口层次

| 接口体系 | 连接及调用方式 | 使用条件 | 与本项目7709的关系 |
| --- | --- | --- | --- |
| tqcenter本地客户端模式 | Python调用本地通达信TQ客户端 | 支持对应功能的本地客户端；近期功能有内测版本限制 | 高层函数接口，不是rustdx的二进制命令清单 |
| 官方HTTP调用 | `POST http://127.0.0.1:17709/`；JSON包含`id/method/params` | 先启动支持TQ的通达信客户端 | **17709 HTTP**，端口及协议均与7709 TCP不同 |
| tdxaidata后台模式 | Python `tqs`调用TdxAiData原生动态库 | Windows/macOS/Linux相应动态库；配置数据服务Key；不需要开启本地客户端 | 官方后台SDK；公开高层函数不能直接推导成7709报文 |

依据：[官方HTTP说明][official-http]、[官方后台模式说明][official-backend]。底层动态库内部使用哪些服务器、认证及二进制报文，本次没有取得足以完整描述的公开规范。

官方后台简介中仍有不提供公式相关能力的旧概述，当前函数列表、10月更新页及1.3.1包内实现已包含公式调用。本报告按有日期的更新和实际版本区分该不一致，不据旧概述否定新接口。

### 2.2 1.3.1的具体新增及兼容变化

| 接口/字段 | 最新版本中的含义 | 调用或数据处理影响 |
| --- | --- | --- |
| `get_misc_data(stock_code, field_list=None)` | 个股综合信息；返回`{中文栏目名: 已格式化内容}` | 代码带市场后缀，例如`000001.SZ`；返回值可能为“2438.48万股”等字符串，不能直接当统一数值；重复栏目保留首项 |
| `get_global_data(type=0)` | 沪深京市场整体统计；1.3.1说明为87个字段 | 1.3.0说明为67个字段，字段数量随版本变化 |
| `get_global_data(type=1)` | 港股、境外指数、商品及股指期货行情；说明为53个字段 | 值为字符串；不能直接套沪深京字段或单位 |
| `formula_process_mul_zb/xg/exp`的`data_source` | 批量指标、条件选股、专家系统公式的行情来源 | 0为沪深京，1为期货期权、港股美股等扩展行情 |
| `formula_by_src/file`的`data_source` | 临时源码/文件公式的行情来源 | 同上；临时公式不是一个新的7709命令 |
| 公式周期 | 官网新增说明支持`5s/10s/15s/fst/auc` | 属于公式计算周期支持，不据此宣布普通7709全部证券都支持秒K |
| 公式`count` | 单只证券取数不得超过24000；`-1`仍表示全部 | 包内说明无效参数会打印提示并返回`{}`，调用方须识别这一结果 |
| 工作目录 | 原生库在`start()`时接收库目录 | 修复数据请求影响进程cwd的问题 |
| `get_tick_data` | 自1.3.0起，存在pandas时返回DataFrame，否则仍返回dict | DataFrame列为`Time/Price/Volume/BSFlag`，各列值为字符串；不能继续按旧字典分页方式取数量 |
| `get_call_auction`及批量入口的`date` | 自1.3.0起可指定历史竞价日期 | 接受`YYYYMMDD`或`YYYY-MM-DD`，参数按原值透传 |

依据：官网10月23日条目，以及固定的[tdxaidata 1.3.1更新说明](../../testing/evidence/tdx-protocol-20261023/tdxaidata-1.3.1/CHANGELOG.md)、[完整包内使用文档](../../testing/evidence/tdx-protocol-20261023/tdxaidata-1.3.1/src/tdxaidata/docs/tdxaidata使用文档.md)。表内字段数量及行为是该版本文档/源码信息，本次没有重新向官方服务采样验证。

### 2.3 官方K线字段不能沿用当前TCP单位

[官方`get_market_data`页面][official-bars]明确说明：OHLC为元，`Volume`为最小单位（股或张），**`Amount`为万元**。`ForwardFactor`是前复权价格因子，仅`dividend_type=none`时返回有效值，不是rustdx中累计收益口径的字段。

`fill_data=True`为默认值，多证券取数时可向后填充缺失数据。接入真实原始行情时须保留这一配置和填充事实，不能把补出的点当作实际成交记录。一次最多24000条；文档称后台模式日线以上全部、1分钟自2024年9月、5/15/30/60分钟自2017年1月可取，但仍需按账户、服务器、证券和实际返回确认。

这些是官方SDK规则，不能用来改写rustdx现有7709解码器的成交量、成交额或历史深度约定。

## 3. “21个命令”的来源与最新26项映射

eltdx 3.0.0历史说明明确写“21个7709命令”。最新3.3.0包的`docs/COMMANDS_7709.md`列出26个不同命令码，协议源码`SUPPORTED_COMMAND_CODES: [u16; 26]`亦登记26个，两者逐项一致。PyPI描述仍有21的旧文字，源码测试函数名仍带`twenty_three`，实际断言已为26；本报告采用清单与注册代码的核对结果。

以下按命令码比较，社区26项**包含心跳和握手**。本库当前为14个业务命令加1个活跃握手命令；“同码”仅证明请求编码有该opcode，不能证明请求参数、响应布局、字段语义及覆盖范围一致。原始详情入口为[社区26命令清单](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/COMMANDS_7709.md)。

| 命令码 | eltdx 3.3.0中的业务能力 | rustdx当前关系 |
| --- | --- | --- |
| `0x0004` | 心跳保活 | 未接入该码；本库heartbeat复用`0x044E` |
| `0x000D` | 会话握手 | 同码；本库活跃`PACK1` |
| `0x000F` | 股本变迁、除权基础 | 同码；`xdxr` |
| `0x0010` | 批量财务查询 | 同码；本库公开`finance`按单只请求，不等于具备社区批量合同 |
| `0x02C5` | 板块文件元数据 | 同码；`block`第一步 |
| `0x02CF` | 公司信息/F10目录 | 同码；`f10_categories/f10` |
| `0x02D0` | 公司信息/F10内容 | 同码；按栏目和字节段读取 |
| `0x044D` | 证券代码表分页 | 未接入；本库目录使用`0x0450` |
| `0x044E` | 证券数量 | 同码；`stock_count`，并用于heartbeat |
| `0x0452` | 特殊品种涨跌停限制表 | 未接入；本库日期化涨跌停规则不能冒充该源表 |
| `0x051B` | 个股分时副图 | 未接入 |
| `0x052B` | 退市股票不复权日线 | 未接入；社区3.2.3新增入口 |
| `0x052D` | K线、周期线 | 同码；本库股票/指数K线，参数及量口径需对照 |
| `0x0537` | 当前日分时，含均价 | 未接入；本库当日首选`0x051D` |
| `0x053E` | 旧版完整批量快照 | 同码；本库`quotes`及五档 |
| `0x0547` | 游标刷新、增量推送及五档 | 未接入 |
| `0x054B` | 分类行情分页 | 未接入 |
| `0x054C` | 显式代码基础快照 | 未接入；不能替换`0x053E`解析器 |
| `0x056A` | 当日/历史集合竞价过程快照 | 未接入 |
| `0x06B9` | 服务器文件分块读取 | 同码；本库板块文件下载使用该码 |
| `0x0FB4` | 指定日期历史分时 | 同码；`history_minute`及当日回退 |
| `0x0FC5` | 当日成交明细分页 | 同码；`transaction` |
| `0x0FC6` | 增强历史成交明细分页 | 未接入；本库历史使用`0x0FB5` |
| `0x0FD1` | 单标的小走势图 | 未接入 |
| `0x0FEB` | 近期历史分时 | 未接入 |
| `0x0FFC` | 日资金流向 | 未接入；社区3.1.0新增入口，属于9月背景 |

核对结果：**同码12项、社区清单中本库未接入14项、本库另有3项未列入该社区清单**。另有3项为`0x0450`证券目录、`0x051D`当日分时、`0x0FB5`历史成交明细。10月更新的injoyai/tdx源码仍使用这3个旧码，所以“清单未列入”不能解释为它们已被全网上游废弃。

本库旧`PACK2`也是`0x000D`、旧`PACK3`为`0x0FDB`，当前正常连接不继续发送，不计入这15项。股票/指数共用`0x052D`，分页、checked请求、前后复权和高层批量封装也不能另算opcode。

本库具体字段及源码入口继续以[主动获取字段字典第14节](../operations/UPSTREAM_ACTIVE_INTERFACES.md#14-与通达信7709协议命令的关系)为准。此处不把社区字段表覆盖到当前已实现类型。

## 4. 对rustdx最有意义的报文与字段差异

### 4.1 K线：同为0x052D，但公开参数和量表达不同

| 项目 | rustdx当前checked请求/解码 | eltdx 3.3.0固定源码 |
| --- | --- | --- |
| 完整请求长度 | **38字节** | **54字节：12字节头＋42字节请求数据** |
| 周期 | API公开`category`；第二周期参数字节固定为1 | `period.raw`和`period.parameter`均可表达 |
| 默认日线/1分钟映射 | 标准Day为9、Minute1为8 | 标准day为`4/1`、1m为`7/1` |
| 自定义周期 | 当前checked category限定0–11 | 文档及源码还表达N分钟`8/N`、N日`9/N`、N秒`13/N` |
| 复权 | 上游原价；`k_adjusted`另取事件在本地计算 | 请求中传`adjust` 0–4及`anchor_date_raw`，文档称主站返回对应价格 |
| OHLC差分还原 | 整数差分累计后**÷1000** | 整数差分累计后**÷1000** |
| 成交量解码 | `vol = vol_amount(encoded_u32)`，解码器未执行÷100 | 保留`volume_wire_value`，另给`volume_lots = wire_value / 100` |
| 原始字段 | 当前KlineData主要为业务值 | 另保留毫厘价、原始量额、差分、逐记录hex等 |
| 时间表达 | 非分钟周期在本库补15:00 | 另有`bar_time`起点/右端点选择；退市日线文档以00:00表达日期 |

eltdx请求数据的静态布局如下；位置相对42字节data，从0计数，数值均为小端：

| data位置 | 类型 | 含义 |
| --- | --- | --- |
| 0–1 | u16 | 市场 |
| 2–7 | 6字节ASCII | 证券代码 |
| 8–9 | u16 | 周期编号raw |
| 10–11 | u16 | 周期参数parameter |
| 12–13 | u16 | 历史分页start |
| 14–15 | u16 | 请求count |
| 16–17 | u16 | 0不复权、1前复权、2后复权、3定点前复权、4定点后复权 |
| 18–21 | u32 | 复权锚定日期，0表示未使用 |
| 22–41 | 20字节 | 构包时填0的保留区 |

依据：[本库checked K线](../../../crates/rustdx-core/src/tcp/stock/kline_request.rs)、[本库周期映射](../../../crates/rustdx-core/src/data/model.rs)、[社区固定K线源码](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/commands/klines.rs)、[社区周期及复权定义](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/crates/eltdx-protocol/src/unit.rs)、[社区K线说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-K线周期线.md)。

**本库K线价格已经是÷1000，不能写成“旧库按分、新库按毫厘”。本库快照中的÷100属于另一命令及另一布局。** 同样，成交量表达差异不能仅凭源码就定性为本库少除或多乘100；旧/新请求、服务器、品种是否返回相同源单位，须以独立真实报文和成交量对照确认。时间标签和服务器复权也仍需验证，不据社区新参数直接更改本库解析。

### 4.2 行情快照、分时和成交明细

| 能力 | 社区最新描述 | 对当前接口的实际影响 |
| --- | --- | --- |
| 基础快照`0x054C` | 无游标、显式代码列表，稳定确认买一/卖一 | 当前`QuoteData`来自`0x053E`完整旧快照，不能假设相同响应布局 |
| 五档刷新`0x0547` | 首次游标0建立基线，后续刷新及未配对推送队列；作者把单次上限固定为100只 | 接入需要游标、推送缓冲与丢失状态，当前同步一次`quotes`不自动具备这些能力 |
| 旧完整快照`0x053E` | 五档、交易状态及原始尾部指标 | 同码仍须按样本核实布局；当前类型未保留全部状态字段 |
| 当日分时`0x0537` | 价格、均价、成交量、时间及原始字段 | 当前`MinuteTimeData`仅price/vol；增加avg_price需要新请求及解码合同 |
| 增强历史成交`0x0FC6` | 作者说明单页最多1800；混合普通成交、09:25撮合、`status=8`竞价快照、`status=5`盘后成交 | 当前历史`0x0FB5`与其不同；不能把所有新记录都归为真实成交，也不能把num视为全局唯一编号 |
| 集合竞价`0x056A` | 查询当日/历史过程，含虚拟匹配、未匹配等信息 | `0x0FC6`的竞价状态记录不能替代完整竞价数量与秒级过程 |
| 日资金流`0x0FFC` | 主力净额、主买净额及四档分类，保留21个raw字段 | 统计分类不等同账户实际资金收支，也不能从当前内外盘直接推导 |
| 退市日线`0x052B` | 仅确认日线、不复权，单页800 | 新命令不能自动补齐证券退市日、全生命周期覆盖或全部退市证券历史 |

本表为固定版本文档及源码描述。[快照边界](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/COMMANDS_7709.md)、[历史成交说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-历史成交明细.md)、[竞价说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/7709-集合竞价明细.md)、[资金流说明](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/docs/methods/0x0ffc-资金流向日数据接口.md)均已随资料保存。作者给出的服务器上限、状态分类及成功样本不等于本次独立实测结果。

## 5. 网络检索范围与证据判断

本次以中文/英文“通达信、7709、2026年9月/10月、协议更新、tdxaidata、tqcenter、eltdx”等词族检索官网、GitHub/Gitee、PyPI、pkg.go.dev，并以CSDN、CodeKK、DeepWiki等索引发现线索。日期以正文条目、PyPI上传时间、发布记录及提交时间分别标注，没有把搜索引擎抓取日期当成协议发布日期。

| 证据层级 | 采用方式 | 本次结果 |
| --- | --- | --- |
| 官方官网说明 | 判断官方支持条件、日期、函数及单位 | 取得9月背景、10月16日及10月23日更新，17709 HTTP和后台SDK说明 |
| 包注册元数据＋原始安装包 | 固定可获取的版本、上传时间及实际文件 | 当时3个源码包SHA-256与PyPI记录一致；当前保留两个原包及eltdx协议展开文件/分发摘录 |
| 社区原仓库及源码 | 判断该实现的命令码、编码、解码及实际变更 | eltdx清单/注册代码26项一致；injoyai时间修复可见实际提交 |
| 社区issue | 记录具体故障和适用范围 | a-stock-data #52报告特定旧库及环境的空响应 |
| 搜索索引/镜像介绍 | 发现候选来源，回到原站核对 | 有旧版本描述和无关命中，未用于证明新版协议 |

[a-stock-data #52][empty-issue]创建于9月11日、更新于9月20日，报告mootdx 0.11.7/tdxpy 0.2.7在其测试的10个主机上，K线、快照、成交为空而部分其他接口可用。它支持“该环境存在兼容问题”，不支持“全网7709全部失效”或“必须换成某一个社区SDK”。

在所检查的公开官方页面中没有找到完整7709请求/响应逐字节规范。这个结果限定于本次可访问资料；登录论坛、客户专属文档、未索引资源和厂商未公开材料可能另有信息。本次没有获取这些材料。

## 6. 原始资料固定、许可及实施边界

主要安装包如下，原字节已保留：

| 包 | 文件 | SHA-256 |
| --- | --- | --- |
| tdxaidata 1.3.1 | [源码分发包](../../testing/evidence/tdx-protocol-20261023/archives/tdxaidata-1.3.1.tar.gz) | `ab528e804b74958cedf2b3381a7f16764591c9eae9f0772ad2e1bac065d98f38` |
| eltdx 3.3.0 | [分发记录摘录](../../testing/evidence/tdx-protocol-20261023/registries/pypi-eltdx.json)，原包已移除，协议展开文件保留 | 原包`16fad43e518618728a2ac5c0a5d0abf0a3bcce3ccb12daccec55d8fa912d9a9f` |
| tdx-cli 0.1.7 | [9月背景源码包](../../testing/evidence/tdx-protocol-20261023/archives/tdx_cli-0.1.7.tar.gz) | `2382b717a5237929ac90c3e328b3163d4b66501041cf20a98687f9ea4dc314df` |

社区GitHub检索状态另固定为eltdx主分支`104f23336f712a98e69821a58db745cd4040862d`、injoyai/tdx主分支`2a9000681f5a9896ea98e4de8bc4904f1dce78ba`。**GitHub观察时点与PyPI打包时点独立**，本次命令及K线对照使用上述摘要固定的eltdx 3.3.0安装包，不声称整个主分支与安装包逐字节一致。

eltdx原始[LICENSE](../../testing/evidence/tdx-protocol-20261023/eltdx-3.3.0/LICENSE)为“ELTDX Non-Commercial Research License”，允许个人阅读、复制及协议研究，禁止商业或生产服务使用。保存资料不构成依赖引入或生产采用。injoyai/tdx许可证为MIT；tdxaidata包内代码许可证为MIT，但代码许可不替代数据服务Key和行情服务使用条件。

本次未运行第三方SDK、没有新增7709网络探针、没有重新验证全部字段单位或市场覆盖；资料中的真实样本均注明原作者来源，不算rustdx本次采集。Rust源码未变，本轮检查仅覆盖资料摘要、链接、清单与源码命令计数，不重新运行无关Cargo测试。

若继续更新rustdx，先对目标命令取得有请求身份、服务器、采样时间和原字节的独立报文，核实请求38/54字节、周期/复权、量价单位和混合事件，再设计相应请求及结果类型。官方17709/tdxaidata接入需要独立的高层API适配，不应并入当前7709命令数量。

## 7. 主要公开来源

- [官方版本更新说明][official-versions]；[保存的原始页面](../../testing/evidence/tdx-protocol-20261023/official/official-versions.html)，采样时间2026-10-23 13:23:56 UTC。
- [官方HTTP方式调用][official-http]、[TdxAiData后台接口][official-backend]、[官方K线接口及单位][official-bars]。
- [tdxaidata PyPI元数据](https://pypi.org/pypi/tdxaidata/json)、[eltdx PyPI元数据](https://pypi.org/pypi/eltdx/json)；前者原始JSON、后者相关分发记录摘录随资料保存。
- [eltdx 3.3.0发布说明][eltdx-3-3]、[3.0.0“21命令”历史说明][eltdx-3-0]。
- [固定的eltdx GitHub命令清单](https://github.com/electkismet/eltdx/blob/104f23336f712a98e69821a58db745cd4040862d/docs/COMMANDS_7709.md)，仅用于固定仓库观察状态。
- [injoyai/tdx版本记录][go-package]、[成交UTC+8修复](https://github.com/injoyai/tdx/commit/aa1ebed19cab7b6ee0eee0b0f4baef7869759384)、[生成K线保留时区](https://github.com/injoyai/tdx/commit/58d425e7ca748e53fe1da6a24862efa4974644b4)。
- [a-stock-data特定环境空响应报告][empty-issue]，按个案引用。

[official-versions]: https://help.tdx.com.cn/quant/docs/markdown/mindoc-1cfsjkbf8f3is/TdxQuantVersion.html
[official-http]: https://help.tdx.com.cn/quant/docs/markdown/mindoc-1hdhbmi50d038.html
[official-backend]: https://help.tdx.com.cn/quant/docs/markdown/mindoc-1hjbgqpdhv114.html
[official-bars]: https://help.tdx.com.cn/quant/docs/markdown/mindoc-1ctuhthaq5qmg/mindoc-1h10g60jt68sc.html
[eltdx-3-0]: https://github.com/electkismet/eltdx/blob/104f23336f712a98e69821a58db745cd4040862d/docs/releases/v3.0.0.md
[eltdx-3-3]: https://github.com/electkismet/eltdx/releases/tag/v3.3.0
[go-package]: https://pkg.go.dev/github.com/injoyai/tdx?tab=versions
[empty-issue]: https://github.com/simonlin1212/a-stock-data/issues/52
