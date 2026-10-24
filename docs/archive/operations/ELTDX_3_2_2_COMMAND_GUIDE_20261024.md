# eltdx 3.2.2：22 个 7709 命令详解与使用约束

核对日期：2026-10-24。版本固定为 **eltdx 3.2.2**，内容依据该版本发布源码中的命令注册、Python API、返回模型和 Rust 编解码器。22 项由 **2 项会话命令 + 20 项业务命令**组成；F10 的 7615 接口、Helper 的组合方法和后续版本新增命令不另计入。[cite:1](#cite-1)

本手册说明 eltdx 的接口合同和源码行为。**调用频率中，30 秒空闲心跳是 SDK 默认配置；其余采集间隔是本文的起始建议，不是通达信公布的 QPS 限额、刷新周期或可用性承诺。** 本次未运行 eltdx SDK、未新增实盘请求，未独立测定服务器限流和历史覆盖深度。官方完整 7709 字节规范及各命令服务端 QPS 未取得。

## 1. 快速查找与调用频率

表中的盘中间隔适用于少量关注证券、复用客户端的查询任务。全市场采集需要按批次、页数和实际请求数重新计算预算，不能让每只证券都独立按此间隔启动任务。

| # | 命令 | 用途 / 主要调用 | 源码边界或默认值 | 建议调用频率 |
| --- | --- | --- | --- | --- |
| 1 | `0x0004` | 心跳：`session.heartbeat()` | 默认空闲心跳间隔 30 秒 | 使用自动心跳 |
| 2 | `0x000d` | 握手：`session.handshake()` | 每条新连接自动初始化 | 建连 / 重连时一次 |
| 3 | `0x000f` | 股本变迁：`corporate.capital_changes()` | 默认每批 75 只，1–200 | 每交易日一次；事件后刷新 |
| 4 | `0x0010` | 基础财务：`corporate.finance_batch()` | 多证券单报文；无自动拆批参数 | 每交易日一次；公告后刷新 |
| 5 | `0x044d` | 证券目录：`codes.list()` / `all()` | 默认 1600 条/页 | 开盘前一次；名单变更后刷新 |
| 6 | `0x044e` | 市场证券总数：`codes.count()` | 返回完整市场代码表条数 | 启动 / 每交易日一次 |
| 7 | `0x0452` | 特殊涨跌停表：`limits.special()` | 表内行号分页；扫描预算默认 10000 | 开盘前一次；需要时复查 |
| 8 | `0x051b` | 分时副图：`minutes.aux()` | 已命名 selector 为 0、11 | 盘中 30–60 秒 |
| 9 | `0x052d` | K 线：`bars.get()` | 单页 1–800；全量默认最多 200 页 | 分钟线每根收完；日线盘后 |
| 10 | `0x0537` | 当日分时：`minutes.today()` | 一次返回主站当前保存的序列 | 盘中 30–60 秒 |
| 11 | `0x053e` | 旧五档快照：`quotes.legacy()` | 单次请求；API 无 80 只专用校验 | 少量关注股 3–5 秒 |
| 12 | `0x0547` | 刷新 / 推送：`quotes.refresh()` | 单次最多 100 只；每只带游标 | 建基线后持续消费；刷新可从 3–5 秒起 |
| 13 | `0x054b` | 分类行情 / 排名：`quotes.list_by_category()` | 默认 80 条/页 | 盘中 5–15 秒 |
| 14 | `0x054c` | 一档基础快照：`quotes.get_snapshots()` | 显式代码列表；一次请求 | 少量关注股 3–5 秒 |
| 15 | `0x056a` | 竞价过程：`auctions.series()` | 专用池；高层无分页参数 | 竞价阶段 3–5 秒；历史一次补齐 |
| 16 | `0x06b9` | 文件：`resources.read()` / `download_file()` | 默认块 30000 字节；最大 60000 | 版本 / 日期变化时下载 |
| 17 | `0x0fb4` | 指定日期分时：`minutes.history()` | 单证券单日；列表输入外层并发 | 补数一次；本地归档 |
| 18 | `0x0fc5` | 当日成交明细：`trades.today()` / `all_today()` | 默认最大 1800 条/页 | 最新页 3–5 秒；全量盘后一次 |
| 19 | `0x0fc6` | 历史成交明细：`trades.history()` / `all_history()` | 默认最大 1800 条/页 | 补数一次；本地归档 |
| 20 | `0x0fd1` | 小走势图：`minutes.sparkline()` | selector=1、window=20 | 页面可见时 30–60 秒 |
| 21 | `0x0feb` | 近期历史分时：`minutes.recent()` | 默认取本机当前日期；近期窗口 | 历史一次；当天 30–60 秒 |
| 22 | `0x0ffc` | 日资金流向：`money_flow.daily()` | 专用池；通常近 5 个交易日 | 盘后一次；盘中观察可 1–5 分钟 |

分页、批量和连接默认值依据固定源码。[cite:2](#cite-2)[cite:3](#cite-3)

## 2. 共同调用约定与限制

### 2.1 安装、连接和生命周期

```bash
python -m pip install "eltdx==3.2.2"
```

```python
from eltdx import TdxClient

with TdxClient(timeout=5) as client:
    snapshots = client.quotes.get_snapshots(["sz000001", "sh600000"])
```

该版本采用 Python API + Rust 核心。公开说明列出的 wheel 覆盖 CPython 3.10–3.14 的部分 Windows、Linux 和 macOS 平台；无匹配 wheel 时需要源码编译环境。以上示例按固定签名编写，本次未执行联网示例。[cite:12](#cite-12)

| 参数 | 默认值 | 实际含义 |
| --- | --- | --- |
| `timeout` | 8.0 秒 | 数字 IP / 已缓存 endpoint 的单次底层请求预算，覆盖排队、建连、握手、发送、响应和一次 retry |
| `server_count` | 2 | 选用的普通主站数量 |
| `connections_per_server` | 4 | 每个选定主站的连接数量 |
| `pool_size` | 未指定时通常为 8 | 最多同时在途的业务请求 / Slot 数，不是每秒请求数 |
| `runtime_workers` | 自动取池大小与可用逻辑处理器数的较小值 | Rust 运行线程数 |
| `heartbeat_interval` | 30.0 秒 | 空闲连接的保活间隔；`None` 关闭 |
| `max_pending_requests` | 256 | 等待空闲 Slot 的请求数量上限；满时 `PoolBusyError` |
| `push_queue_size` | 1024 帧 | 共享 push 队列帧数上限 |
| `push_queue_bytes` | 64 MiB | 共享 push 队列 wire 字节上限 |
| `probe_hosts` | `True` | 创建客户端时对候选主站做 TCP 连通 / 延迟排序 |

普通候选主站和竞价 / 资金流向专用候选主站分别为包内配置的 43 和 35 台；两类专用业务共享排名、分别按需建池。这是候选清单数量，不是保证可用的服务器数量。TCP 测速没有验证全部业务命令。[cite:3](#cite-3)

**复用一个长生命周期客户端。** 每轮重建客户端会重复排名准备、建连和握手。高层全量 / 多证券调用可能触发很多底层请求，`timeout=5` 不等于整个全量任务一定在 5 秒内结束；任务总 deadline 需要调用层另行管理。首次 hostname DNS 解析也有不受 native 请求 deadline 严格取消的例外。

### 2.2 代码、市场、日期和原始字段

| 项目 | 约定 / 使用要求 |
| --- | --- |
| 市场 | `0=sz`、`1=sh`、`2=bj`；属于此协议市场编号 |
| 完整代码 | 8 个 ASCII 字符，例如 `sz000001`、`sh600000`、`bj920000` |
| 裸代码 | 6 位数字按段推断市场；`0/1/2/3` 开头推为深市，`6/9` 为沪市，但 `92` 为北交所，`8` 为北交所 |
| 推断局限 | `000001` 会推为深市股票；上证指数应写 `sh000001`。5 开头的沪市 ETF 如 `510300` 无法按上述规则推断，应写 `sh510300` |
| 日期 | 历史查询支持 `YYYY-MM-DD`、`YYYYMMDD`、日期类型；构造有效自然日不证明该日是交易日或服务器有数据 |
| `full_code` | 属性值 `exchange + code`，建议用作多市场主键 |
| `*_raw` | 原始协议值或未知口径；后缀不代表已经转换为元 / 股 |
| `*_milli` | SDK 的千分价格表示，通常价格为该值 `/1000` |
| `record_hex` | 单条记录的原文；主要供对照和留证 |
| `raw_payload` | 响应业务正文 bytes；不同命令填充规则不同 |
| `None` | 不提供、不可解析或缺失；不能统一当成零 |

多数支持 `include_raw` 的接口默认 `False`，相应 payload / hex 字段保持空值；数值 raw 字段仍可能保留。并非每个接口都接受 `include_raw`，以各命令签名为准。代码前缀和日期规则来自固定解析器。[cite:4](#cite-4)

### 2.3 三种“批量”必须分清

| 参数 / 接口 | 批量的含义 | 对请求量的影响 |
| --- | --- | --- |
| `capital_changes(batch_size=75)` | 每个协议报文最多带该批证券，范围 1–200 | 可能遇响应截断再补拉 |
| `finance_batch(codes)`、快照代码列表 | 单报文带多个证券；高层没有统一自动拆批参数 | 长列表需要调用者控制报文和响应大小 |
| 分时、K 线、成交列表输入 | 每只证券分别请求，外层线程并发 | 一只全量请求又可能触发多页 |
| `money_flow.daily(batch_size=75)` | 最大并发证券数，仍逐只请求 | 实际并发受池容量约束 |

分时 API 的 `batch_size` 可创建超过池容量的工作线程，业务 wire 并发仍由池限制；K 线、成交、资金流向的工作线程计算还会取池容量上限。不要将几个 API 的同名 `batch_size` 都解释为每报文证券数。[cite:2](#cite-2)

### 2.4 缓存与新鲜度

直接调用代码数量、目录、股本、财务、快照、分时、成交、K 线等接口会发起查询；该版本没有为它们统一配置“报价 5 秒、分时 60 秒”之类 TTL。部分 Helper 缓存其内部使用的财务批次、证券表和统计资料，价格解码还会缓存目录精度；`clear_cache()` 可清理相应客户端缓存。[cite:3](#cite-3)

缓存命中率、轮询间隔和主站数据延迟是三个概念。返回成功不证明数据刚更新；当日分时 / 当日成交可能仍是最近交易日数据，日期空缺时需与会话日期、交易日历和本地采集时间一起判断。

## 3. 22 个命令逐项详解

### 3.1 `0x0004`：心跳保活

**用途：** 检查已建立会话的业务响应并维持空闲连接。

```python
ack = client.session.heartbeat()
```

无业务参数；返回 `HeartbeatAck`。请求业务体为空；响应正文至少 10 字节。[cite:5](#cite-5)

| 字段 | 含义 |
| --- | --- |
| `reserved` | 响应前 6 字节保留数据 |
| `server_date_raw` | 主站日期原始 YYYYMMDD 数值 |
| `server_date` | 可解析日期；非法日期为 `None` |
| `raw_payload` | 心跳原始响应正文 |

**频率与约束：** 默认配置为 30 秒空闲心跳。运行核心只在 Slot 空闲、没有活动请求且最后活动已超过间隔时选出心跳候选，因此不是“无论忙闲每 30 秒强制多发一包”。短任务通常无需手动调用；`heartbeat_interval=None` 会关闭自动保活。日期正常只能证明此次响应可解析，不能证明全部业务或当天行情完整。[cite:3](#cite-3)

### 3.2 `0x000d`：连接握手

**用途：** 新 TCP 会话初始化，并读取主站身份、日期时间和时段。

```python
info = client.session.handshake()
```

请求业务体为单字节 `0x01`；返回 `HandshakeInfo`，解析器要求正文至少 189 字节。[cite:5](#cite-5)

| 字段 | 含义 / 局限 |
| --- | --- |
| `server_datetime` | 主站日期时间；为无时区 datetime，不应默认当作 UTC |
| `session_minutes_1/2` | 两组时段数值解码成的时间文本，各最多 8 项 |
| `server_date_1/2` | 两个日期槽；未把它们统一承诺为“今天 / 上一交易日” |
| `server_name` | GBK 解码的主站名称 |
| `product_tag` | 产品标识文本 |
| `unknown_time_1_raw/2_raw` | 未明确命名的时间相关原值 |
| `flags_raw`、`tail_control_raw` | 标志及控制保留片段 |
| `raw_payload` | 完整握手正文 |

**频率与约束：** 建连 / 重连自动握手一次。需要检查主站信息时可显式查询；不必当作实时行情轮询。握手时段是服务器提供的信息，仍需交易所日历及证券交易规则判断是否交易。

### 3.3 `0x000f`：股本变迁 / 除权除息 / GBBQ

**用途：** 获取证券广义权息与股本事件，用于股本研究及复权依据。

```python
block = client.corporate.capital_changes("sz000001", include_raw=True)
batch = client.corporate.capital_changes(
    ["sz000001", "sh600000"], batch_size=75
)
```

单字符串返回 `CapitalChangeBlock`；列表返回 `CapitalChangeBatch`。`batch_size` 是每逻辑协议批次的代码数，默认 75，要求整数 1–200；不是同时查询线程数。服务端响应过大只返回前缀时，高层核对返回代码并补拉剩余代码。[cite:2](#cite-2)[cite:6](#cite-6)

| 字段 | 含义 |
| --- | --- |
| Block 的 `records/items`、`count` | 该证券事件列表、实际事件数 |
| `block_count` | 服务端块数量，不等于事件数 |
| Batch 的 `blocks/items`、`count` | 证券块列表、块数 |
| Batch 的 `records` | 所有证券事件的扁平视图 |
| `raw_payload/raw_payloads` | 单证券 / 各次协议响应原文 |
| Record 的 `date_raw/date` | 事件日期原值 / 日期 |
| `category_raw/category/category_name` | 事件编号 / 同编号属性 / 类别名称 |
| `c1_raw`–`c4_raw` | 四个 wire 字节字段 |
| `c1_float`–`c4_float` | 四字段按 float32 直接解读的值 |
| `c1_value`–`c4_value`、`c1`–`c4` | 按类别转换的业务值及简写 |
| `reserved_7`、`record_hex` | 保留字节 / 单条原文 |
| `time` | 事件日期派生的 15:00 时间，非原始事件发生时刻 |

| 类别 | 名称 | 四字段的使用口径 |
| --- | --- | --- |
| 1 | 除权除息 | c1 每十股现金分红、c2 配股价、c3 每十股送转数、c4 每十股配股数 |
| 2 / 3 | 送配股上市 / 非流通股上市 | 数量类；业务值转换为股 |
| 4 | 未知股本变动 | float 原值；业务语义不作统一推断 |
| 5 | 股本变化 | 数量类；业务值转换为股 |
| 6 | 增发新股 | c2 为元价格、c3 转为股，c1/c4 保留原值 |
| 7 / 8 / 9 / 10 | 回购 / 增发上市 / 转配股上市 / 可转债上市 | 数量类；业务值转换为股 |
| 11 / 12 | 扩缩股 / 非流通股缩股 | float 比例类 |
| 13 / 14 | 送认购 / 认沽权证 | float 权证类 |
| 15 | 重整调整 | float 比例类；不能自动套类别 1 公式 |

数量类别 `2/3/5/7/8/9/10` 的业务值已经从万股乘 10000，不能重复乘。每十股比例转成每股时才 `/10`。`adjustment_factors()` 是该查询的本地派生封装，只选类别 1 计算仿射系数，不增加新命令。[cite:6](#cite-6)

**频率与约束：** 每交易日一次，除权、增发、回购等事件后刷新。直接调用无缓存；“一次批量调用”可能实际包含拆批、补拉及并发请求。要做历史回测，应保存当时可见的股本版本与事件来源，当前查询不证明历史时点的可得性。

### 3.4 `0x0010`：批量基础财务

**用途：** 获取主站最新基础财务快照，包含股本、日期、地区 / 行业编号和财务总量。

```python
batch = client.corporate.finance_batch(
    ["sz000001", "sh600000"], include_raw=True
)
selected = client.corporate.finance_batch(
    ["sz000001"], fields=["流通股本", "总资产", "eps_raw"]
)
```

无 `fields` 返回 `FinanceBatch(records, count, raw_payload)`；传 `fields` 返回字段字典列表，每行包括 `full_code`。字段过滤发生在本地，**不会减少服务器返回的完整财务记录**；未知字段抛 `ValueError`。接口没有股本查询那样的自动拆批 / 补拉合同，应主动分成适量代码批次。[cite:2](#cite-2)

| 字段 | 含义 / 单位 |
| --- | --- |
| `updated_date_raw/updated_date` | 数据更新日期，不自动等于财报报告期 |
| `ipo_date_raw/ipo_date` | 上市日期 |
| `province_raw/industry_raw` | 地区与粗行业编号；没有直接附中文名称 |
| `finance_info_raw` | 136 字节财务主体 |
| `liu_tong_gu_ben_raw_float` | 流通股本，万股 |
| `zong_gu_ben_raw_float` | 总股本，万股 |
| `guo_jia_gu_raw_float` | 国家股，万股 |
| `fa_qi_ren_fa_ren_gu_raw_float` | 发起人法人股，万股 |
| `fa_ren_gu_raw_float` | 法人股，万股 |
| `b_gu_raw_float/h_gu_raw_float` | B 股 / H 股，万股 |
| `eps_raw` | SDK 命名为每股收益的原值；元/股口径，报告期与财务口径需核实 |
| `zong_zi_chan_raw_float` | 总资产，千元 |
| `liu_dong_zi_chan_raw_float` | 流动资产，千元 |
| `gu_ding_zi_chan_raw_float` | 固定资产，千元 |
| `wu_xing_zi_chan_raw_float` | 无形资产，千元 |
| `gu_dong_ren_shu_raw_float` | 股东人数原值，不按金额比例换算 |
| `liu_dong_fu_zhai_raw_float/chang_qi_fu_zhai_raw_float` | 流动 / 长期负债，千元 |
| `zi_ben_gong_ji_jin_raw_float/jing_zi_chan_raw_float` | 资本公积 / 净资产，千元 |
| `zhu_ying_shou_ru_raw_float/zhu_ying_li_run_raw_float` | 主营收入 / 主营利润，千元 |
| `ying_shou_zhang_kuan_raw_float` | 应收账款，千元 |
| `ying_ye_li_run_raw_float/tou_zi_shou_yu_raw_float` | 营业利润 / 投资收益，千元 |
| `jing_ying_xian_jin_liu_raw_float/zong_xian_jin_liu_raw_float` | 经营 / 总现金流，千元 |
| `cun_huo_raw_float` | 存货，千元 |
| `li_run_zong_he_raw_float/shui_hou_li_run_raw_float` | 利润总额 / 税后利润，千元 |
| `jing_li_run_raw_float/wei_fen_li_run_raw_float` | 净利润 / 未分利润，千元 |
| `mei_gu_jing_zi_chan_raw_float` | 每股净资产，元/股 |
| `bao_liu_2_raw_float/record_hex` | 保留字段 / 原文 |

**关键换算，以 3.2.2 属性代码为准：**

```text
circulating_shares = liu_tong_gu_ben_raw_float × 10000
total_shares       = zong_gu_ben_raw_float × 10000
total_assets_yuan  = zong_zi_chan_raw_float × 1000
net_profit_yuan    = jing_li_run_raw_float × 1000
```

不能将其他 SDK 的“金额乘 10000”规则搬到此版本。`eps_raw` 与旧社区实现同位置的字段命名存在历史差异；本手册确认此版本如何命名，没有独立验证其适用于所有证券的 EPS / TTM / 年化口径。计算 PE 前应核对披露报告。[cite:6](#cite-6)

**频率与约束：** 每交易日一次或财报 / 修订披露后刷新。直接查询每次联网，无 `refresh` 参数；最新快照不提供完整历年财务序列。

### 3.5 `0x044d`：证券代码表分页

**用途：** 获得证券名称、价格精度、品种与上市板块分类，也是价格解码的重要元数据。

```python
page = client.codes.list("sh", start=0, limit=1600)
all_sh = client.codes.all("sh", page_size=1600)
a_shares = client.codes.all_a_shares()
```

返回证券记录列表。`start` 为 u32 表内偏移，范围 0–4294967295；单页 `limit` 默认 1600，允许 0–1600。自动分页的 `page_size` 要求 1–1600，按实际返回条数推进，直到空页；无需先查询数量。记录布局为 37 字节。[cite:7](#cite-7)

| 字段 | 含义 |
| --- | --- |
| `name` | GBK 解码证券简称 |
| `multiple` | 协议换算相关倍数原值，不可直接当作所有品种的每手股数 |
| `decimal` | 该证券价格小数位，用于行情精度 |
| `previous_close_price` | 昨收参考价 |
| `volume_ratio_base` | 服务端量比基础值，非直接保证为当前量比 |
| `category` | 本地规则派生：a_share、b_share、index、etf、bond 等 |
| `category_reason` | 品种分类命中理由 |
| `board` | 本地规则派生：沪 / 深主板、科创、创业、北交等 |
| `board_reason` | 上市板块分类命中理由 |
| `unknown0_raw/previous_close_raw/unknown3_raw` | 保留片段 / 昨收原始 bytes |

通用身份字段见 2.2。`category/board` 不是行业 / 概念板块，也不是服务器完整监管分类；新证券或新前缀可能落为 unknown。`all_a_shares()` 返回完整代码字符串列表，单市场 `a_shares()` 返回证券记录。[cite:2](#cite-2)

**频率与约束：** 开盘前加载一次；新股、改名、ST 或品种信息变化后刷新。直接 `all()` 不缓存；精度装载会在行情初始化中复用目录，不必每轮行情主动拉全目录。应检查市场覆盖、重复代码和空结果，解析支持北交所不保证每台主站都提供完整北交所目录。

### 3.6 `0x044e`：市场证券数量

**用途：** 获取指定市场完整代码表数量。

```python
total = client.codes.count("sz")
a_share_total = client.codes.a_share_count("sz")
```

`count(market)` 返回整数，底层按 u16 解读证券数量；市场支持 sz/sh/bj。客户端日期参数由 SDK 填充，高层不要求用户传日期。[cite:7](#cite-7)

**统计边界：** `count()` 包括基金、债券、指数等目录证券，不能当作上市 A 股数量。`a_share_count()` 走 `0x044d` 拉全目录再按 `category=="a_share"` 本地计数，可能产生多页请求；它不是一个更便宜的 `0x044e` 子类型查询。[cite:2](#cite-2)

**频率与约束：** 启动 / 每交易日一次。可用于目录合理性复查；数量一致不证明每条记录正确、无重复或没有过期品种。

### 3.7 `0x0452`：特殊品种涨跌停限制表

**用途：** 查询服务端特殊涨跌停表，通常需要扫描建本地证券索引。

```python
page = client.limits.special(start_index=0)
records = client.limits.scan_special(start_index=0, max_rows=10000)
```

返回 `SpecialLimitPage(start_index, records, count, raw_payload)`。`start_index` 为 u16 行号，范围 0–65535；高层没有单证券 code 参数，也没有可指定 page_size。[cite:5](#cite-5)

| 字段 | 含义 |
| --- | --- |
| `code_num/code/full_code` | 数值代码 / 六位字符串 / 带市场代码 |
| `upper_price_raw_f32/lower_price_raw_f32` | float32 上下限原值 |
| `limit_up_price/limit_down_price` | 上述价格的直接属性，没有额外放大 |
| `record_hex` | 13 字节单记录原文 |

`scan_special()` 默认以 10000 行作循环预算，按实际条数推进，空页结束；末页可能使返回总数略超预算，不是严格 `[:max_rows]`。表里没某只证券，不能解释为该证券无涨跌停；普通涨跌停仍须按交易所、上市阶段、证券状态和当日规则判断。[cite:2](#cite-2)

**频率与约束：** 开盘前扫描一次；需要研究特殊品种时复查。不要逐证券反复从表头扫描。

### 3.8 `0x051b`：分时副图双序列

**用途：** 分时页的委买 / 委卖力道或当日与前日成交对比。

```python
strength = client.minutes.aux("sz000001", kind="buy_sell_strength")
comparison = client.minutes.aux("sz000001", kind="volume_comparison")
```

仅单证券接口；`kind` 可传已命名字符串或 u8 selector（0–255）。已知 `0x00` 为买卖力道，`0x0b` 为成交对比；未知数字能编码，不代表语义已验证。[cite:8](#cite-8)

| 字段 | 含义 |
| --- | --- |
| Series 的 `selector_raw/kind/points/count` | selector、SDK 类型名、双序列点、点数 |
| Point 的 `index/time_label` | 分时序号 / 按分钟表生成的标签 |
| `series_a/series_b` | 两条序列通用值 |
| `buy_commission/sell_commission` | selector=0 的委买 / 委卖口径 |
| `previous_day_cumulative_volume` | selector=11 的前日累计量口径 |
| `current_day_cumulative_volume/cumulative_volume` | selector=11 的当日累计量及同值别名 |
| `record_hex/raw_payload` | 单点 / 整体原文，取决于 `include_raw` |

**频率与约束：** 盘中 30–60 秒；当前序列量是累计口径时，不能将所有点直接相加作为全天量。委买 / 委卖不是实际成交量或账户资金流入。未知 selector 的 `kind` 名称可能回落为 buy_sell_strength，不应只凭名称判断字段合同。

### 3.9 `0x052d`：K 线、周期线与服务器复权

**用途：** 股票 / 指数各种周期 K 线，以及服务端前、后、定点复权查询。

```python
daily = client.bars.get("sz000001", period="day", count=800)
qfq = client.bars.get("sz000001", period="day", adjust="qfq")
fixed = client.bars.get(
    "sz000001", period="day", adjust="fixed_qfq", anchor_date="2024-06-03"
)
history = client.bars.get(
    "sz000001", period="day", all_pages=True, page_size=800, max_pages=200
)
```

| 参数 | 含义 / 边界 |
| --- | --- |
| `code` | 单字符串返回 KlineSeries；列表返回 `{full_code: KlineSeries}`，重复代码去重 |
| `period` | 默认 day，标准或 N 周期字符串 |
| `start` | 最新端向更早位置的偏移，u16：0–65535 |
| `count` | 单页根数，1–800；默认 800 |
| `adjust` | none=0、qfq=1、hfq=2、fixed_qfq=3、fixed_hfq=4 |
| `anchor_date` | 定点复权基准日；业务使用必须提供有效日期 |
| `kind` | stock / index 记录布局；缺省按代码识别 |
| `all_pages` | 默认 False；True 时使用 page_size 自动翻页 |
| `page_size/max_pages` | 1–800；默认最多 200 次请求，None 去掉页数保护 |
| `batch_size/include_raw` | 外层并发证券数 / 原文保留 |

**标准周期：**

| 业务周期 | 协议 `period_raw / period_param_raw` |
| --- | --- |
| 5m / 15m / 30m / 60m | 0/1、1/1、2/1、3/1 |
| day / week / month | 4/1、5/1、6/1 |
| 1m / quarter / year | 7/1、10/1、11/1 |
| 自定义 Nm / Nd / Ns | 8/N、9/N、13/N |

标准 5/15/30/60 分钟走各自标准编号。SDK 能构造自定义周期，不证明主站支持、保存深度或时间标签正确；应用应拒绝无业务意义的 N=0。固定复权模式与日期组合并非全部由高层强制检查，整数 anchor 会按 u32 接受，建议使用有效日期字符串 / date 并自行校验。[cite:4](#cite-4)[cite:9](#cite-9)

| 字段 | 含义 |
| --- | --- |
| Series 的 `period_raw/period_param_raw/period_name` | 协议周期、N 参数、名称 |
| `adjust_mode_raw/adjust_mode` | 复权编号和名称 |
| `anchor_date_raw/anchor_date` | 锚点原值 / 日期 |
| `start/request_count/bars/count` | 请求位置、请求量、返回 K 线、实际根数 |
| Bar 的 `time` | 解码时间；不能直接推断所有周期均为收盘标签 |
| `open/high/low/close` | 开高低收 |
| `open_price_milli/high_price_milli/low_price_milli/close_price_milli` | OHLC 的千分价格表示 |
| `last_close_price_milli` | 当前响应页前一根的收盘价；每页第一根可能为 None |
| `volume_raw/amount_raw` | 压缩数值编码槽，不能直接当作股数 / 元 |
| `volume_wire_value` | 原始量槽经解码得到的数值 |
| `volume_lots` | **SDK 固定计算 `volume_wire_value / 100`** |
| `amount` | 金额槽解码值 |
| `open_delta_raw/close_delta_raw/high_delta_raw/low_delta_raw` | 四项价格差分原值 |
| `up_count/down_count` | 指数布局涨 / 跌家数；普通股票为 None |
| `record_hex/raw_payload` | 原文 |

**分页与顺序：** 全量读取遇空页才结束，短页不结束，偏移按实际返回条数增加；达到 `max_pages` 但还没遇空页会抛 `RuntimeError`。u16 偏移仍限制可定位范围，200 页不表示能访问 160000 根。该版本高层直接按请求顺序拼接 K 线页，**没有全局排序和去重**；使用时应排序、检查重复和窗口完整性。单根的 `last_close_price_milli` 也是页内值，不自动补跨页边界。[cite:2](#cite-2)

**量纲限制：** `volume_lots` 名称及固定除 100 是实现事实，不能证明所有周期 / 品种的 wire 量都为股。本仓库既有局部真实样本显示，同为 052d，所测股票 / ETF 日线 4/1 与 9/1 的量纲不同。这里的 day 是 4/1；仍须为各周期和品种验证单位，尤其自定义 N 日、指数与债券。[既有单位证据](../validation/KLINE_VOLUME_UNITS_20261024.md)

**频率与约束：** 已收完的分钟 K 线按周期边界拉最新少量并覆盖最后一根；盘中要看未结束 K 线可 15–60 秒刷新，但将最后一根标为未完成。日线盘后更新，历史全量只在初始化 / 补数时拉；发生权息变化后，非定点复权历史可能需要重建。服务端复权参数被编码不等于所有主站都支持全部模式。

### 3.10 `0x0537`：当日分时

**用途：** 读取主站当前保存的分钟价格、分钟量和均价。

```python
series = client.minutes.today("sz000001", include_raw=True)
many = client.minutes.today(["sz000001", "sh600000"], batch_size=2)
```

单字符串返回 `MinuteSeries`；列表返回字典，去重后逐证券并发请求。无 date / start / count 参数。[cite:2](#cite-2)[cite:8](#cite-8)

| 字段 | 含义 / 当日特性 |
| --- | --- |
| `trading_date` | **请求和响应不带日期，保持 None** |
| `prev_close/open_price/date_selector_raw` | 当日布局通常缺失，为 None |
| `reserved_zero` | 响应保留槽 |
| `points/count/volume_sum` | 点列表 / 点数 / 所有分钟量之和 |
| Point 的 `index/time_label` | 序号 / SDK 按 A 股分钟表生成的标签 |
| `time` | 无交易日期时为 None |
| `price/price_milli` | 价格及千分表示 |
| `avg_price` | 主站均价，非简单分钟价格算术平均 |
| `volume` | 分钟增量；SDK 文档称手，具体品种应核验 |
| `price_field/avg_field` | 协议价 / 均价字段 |
| `price_raw/avg_raw` | 原值的还原口径 |
| `price_delta_raw/aux_delta_raw` | 差分原值；某布局为 None |
| `record_hex/raw_payload` | 原文 |

**时间约束：** index=0 的 SDK 标签为 09:31；0–119 对应 09:31–11:30，120 开始对应 13:01。这是 SDK 生成规则，不是每点携带服务器时间。常见完整序列为 240 点，不能要求所有交易阶段 / 品种必定 240。[cite:4](#cite-4)

**频率与约束：** 盘中 30–60 秒。每次多为重读当前序列，不能把多次 `volume_sum` 相加。周末 / 凌晨返回的数据可能属于上一交易日；记录 `fetched_at` 与确认的交易日，不能用本机日期填充后声称来自服务器。

### 3.11 `0x053e`：旧版批量五档快照

**用途：** 兼容旧版完整行情布局，取得五档、原始交易状态和尾部指标。

```python
quotes = client.quotes.legacy(["sz000001", "sh600000"])
```

返回 `list[LegacyQuote]`。代码列表必须非空；一次查询不自动按 80 拆批，公开 API 无 include_raw 参数。[cite:2](#cite-2)[cite:10](#cite-10)

| 字段 | 含义 |
| --- | --- |
| `last_price/pre_close_price/open_price/high_price/low_price` | 最新、昨收、开高低 |
| `total_hand/current_hand` | 累计量 / 最新切片量，SDK 股票口径称手 |
| `amount/amount_raw` | 累计成交额解码值 / 编码槽 |
| `inside_dish/outer_disc` | 内盘 / 外盘统计 |
| `open_amount_raw/open_amount_yuan` | 开盘金额槽及元属性 |
| `buy_levels/sell_levels` | 五档；每档 price、volume、price_delta_raw |
| `server_time_raw` | 主站时间相关原值，非已解析带日期 timestamp |
| `active1/active2` | 协议活跃值，不保证业务计数语义 |
| `trading_status_raw/trading_status_hex` | 原始状态 / 十六进制文本 |
| `tail_metrics_raw` | 四个未统一命名的尾部原值 |
| `rise_speed_raw` | 可选原始涨速槽 |
| `unknown_after_time_raw/unknown_after_outer_raw` | 未确认字段 |
| `record_hex` | 单记录原文 |

**批量边界：** 3.2.2 只做通用列表计数和帧长度检查，没有在此命令写 80 只硬校验。80 是 Helper 常用分批值，也可作为保守起点，不能写成“此版本必然拒绝第 81 只”。即使构包允许较大列表，响应大小和主站截断仍会限制实际返回，应逐代码核对完整性。

**状态限制：** `trading_status_raw` 与成交明细的 `status_raw` 不是相同字段。该版本停牌便捷查询检查 `trading_status_raw & 0x20`；不能把所有状态直接映射成成交方向。[cite:2](#cite-2)

**频率与约束：** 少量关注股 3–5 秒起步。全市场应拆批轮转；每次是快照，不提供快照之间的全部交易过程。累计量、内外盘和当前量不能混加。

### 3.12 `0x0547`：游标刷新与未配对推送队列

**用途：** 建立五档刷新基线、携带逐证券游标查询，接收未配对更新帧。

```python
baseline = client.quotes.get_depth(["sz000001", "sh600000"])
page = client.quotes.refresh(
    ["sz000001"], cursors={"sz000001": 0}
)
frame = client.quotes.poll_push(timeout=0.5, parse=False)
```

单次最多 **100 只**。`get_depth()` 等价于 `refresh(codes, cursors={})`，缺省游标为 0；高层不会自动记住上次记录并更新用户的 `cursors` 字典。`cursors` 值为 u32；使用规范化完整代码作键。[cite:2](#cite-2)[cite:10](#cite-10)

| 字段 | 含义 |
| --- | --- |
| Page 的 `requested_codes/records/count` | 请求代码、返回刷新记录、记录数 |
| `decoded_payload/raw_payload` | 内层解码正文 / 外层原文 |
| Record 的 `update_time_raw` | 更新 / 游标相关原值；不是天然跨会话时间戳 |
| `active/status_or_reserved_raw` | 活跃槽 / 状态或保留原值 |
| `last_price/last_close_price/open_price/high_price/low_price` | 最新价 / 上次收盘参考 / 开高低 |
| `total_hand/current_hand/amount/amount_raw` | 累计量、当前量、金额及编码槽 |
| `inside_dish/outer_disc/open_amount_raw/open_amount_yuan` | 内外盘、开盘金额槽及解码金额 |
| `buy_levels/sell_levels` | 五档价格、量、价格差分 |
| `unknown_after_outer_raw/tail_raw/record_hex` | 未确认槽 / 尾部 / 原文 |

`poll_push(timeout=0.0)` 默认非阻塞，无帧返回 None；`parse=False` 返回 ResponseFrame。`drain_pushes()` 取当前已有的所有帧，**消费队列本身不等于发一次新的行情查询**。`parse=True` 需要推送正文与当前解码上下文匹配。[cite:3](#cite-3)

**队列限制：** 默认 1024 帧及 64 MiB。超限会丢最旧帧，随后 poll / drain 抛一次 `PushOverflowError`。捕获后仅继续读取不能恢复已丢更新；需要精确状态时，重新建基线并核对证券范围。游标、首帧状态和推送应同一会话管理；需要固定连接可使用 pin，但 pin 会独占一个 Slot。

**频率与约束：** 先发游标 0 基线，随后持续消费；显式刷新可从 3–5 秒起，根据数据变化和响应时间调整。消费可用 0.5 秒阻塞等待，这个等待参数不是服务端推送周期。不能假定固定 3 秒推送、永久订阅或重连后游标仍有效；标准 API 提供单次刷新与队列，不替用户完成永久订阅、状态合并和断线补齐。

### 3.13 `0x054b`：分类行情列表与服务端排序

**用途：** 按服务端类别取行情 / 排名，比下载全部证券后排序更适合小榜单。

```python
page = client.quotes.list_by_category(
    "沪深A股", sort_by="涨幅", start=0, count=80, ascending=False
)
```

返回 `CategoryQuotePage`。`category` 允许已识别中文或数值；沪深 A 股映射 6。`start/count` 是 u16，允许 0–65535；count 默认 80，**没有“统一最大 80”校验，也没有高层自动全榜翻页**。[cite:2](#cite-2)[cite:10](#cite-10)

| sort_by | 编号 | sort_by | 编号 |
| --- | --- | --- | --- |
| 代码 / 现价 | 0x0000 / 0x0006 | 成交额 / 涨幅 | 0x000a / 0x000e |
| 封单额 / 开盘金额 | 0x001c / 0x001d | 涨速 / 短换手 | 0x002e / 0x00cc |
| 量涨速 / 开盘抢筹 | 0x00d0 / 0x010a | 2分钟金额 / 开盘涨幅 | 0x010c / 0x0119 |
| 最高涨幅 / 最低涨幅 | 0x011a / 0x011b | 回撤 / 攻击 | 0x011e / 0x011f |

`ascending=False` 为降序；无排序类型或代码排序时，原始 reverse 默认 0，不直接按降序标志解释。数值类别 / 排序值能编码不代表主站业务支持。

| 字段 | 含义 |
| --- | --- |
| Page 的 `category/sort_type/start/request_count` | 类别、排序位、起点、请求量 |
| `sort_reverse/filter_raw/header` | 排序方向编码 / 过滤槽 / 响应头值 |
| `records/count/raw_payload` | 返回记录、实际数、原文 |
| Record 的价格与量额、内外盘 | 与快照基础字段类似 |
| `bid1/ask1/bid_vol1/ask_vol1` | 买一 / 卖一价量 |
| `open_amount` | 分类行情中的开盘金额解码值 |
| `rise_speed/short_turnover` | 涨速 / 短周期换手；对应 raw 槽经 SDK 缩放 |
| `min2_amount` | 主站近 2 分钟金额口径值 |
| `opening_rush` | 开盘抢筹口径值 |
| `vol_rise_speed/depth` | 量增速 / 深度槽；精确统计窗口需核实 |
| `locked_amount` | 本地 `bid1 × bid_vol1 × 100`，股票手数假设 |
| `active1/active2/server_time_raw/neg_price_raw` | 原始活跃 / 时间 / 价格相关槽 |
| `status_or_sort_raw/after_outer_raw` | 状态或排序相关 / 内外盘后原值 |
| `rise_speed_raw/short_turnover_raw/opening_rush_raw/amount_raw/open_amount_raw` | 对应编码槽 |
| `extra_pair_raw/extra_meta_raw/tail_raw/record_hex` | 未统一命名片段 / 原文 |

**频率与约束：** 盘中 5–15 秒起。动态排名跨页会变化，可能重复 / 漏证券；全量排名采集应注明采集区间并检查覆盖。买一金额不自动等于涨停封单额，只有确认当时封住涨停才可按封单解释。

### 3.14 `0x054c`：显式代码基础一档快照

**用途：** 一次按代码列表读取基础价量和已确认的一档盘口。

```python
quotes = client.quotes.get_snapshots(["sz000001", "sh600000"])
```

返回 `list[QuoteSnapshot]`；一次请求，不自动拆批。模型字段足以装档位列表，但该命令实盘文档只稳定确认买一 / 卖一；完整五档使用 0547 或 `helpers.full_quotes()` 组合。[cite:1](#cite-1)[cite:10](#cite-10)

| 字段 | 含义 |
| --- | --- |
| `last_price/pre_close_price/open_price/high_price/low_price` | 最新 / 昨收参考 / 开高低 |
| `total_hand/current_hand` | 累计成交量 / 当前切片量，股票常用手口径 |
| `amount/amount_raw` | 累计金额 / 编码槽 |
| `inside_dish/outer_disc` | 内盘 / 外盘统计 |
| `open_amount_raw/open_amount_yuan` | 开盘金额原槽 / 元属性 |
| `buy_levels/sell_levels` | 已确认的一档，每档 price、volume、price_delta_raw |
| `active1/time_raw` | 活跃值 / 时间相关原值 |
| `unknown_after_time_raw/unknown_after_outer_raw/tail_raw` | 未确认槽 / 尾部 bytes |
| `change/change_pct` | 最新减昨收 / 除昨收再乘 100；昨收 0 时百分比 None |
| `sum_buy_vol/sum_sell_vol` | **当前对象实际档位**量之和；只有一档时就是一档量 |

**频率与约束：** 少量关注证券 3–5 秒起。采用约 80 只分批可作为起点，但此 API 源码没有 80 只专用拒绝；必须核对实际返回代码，不能根据参数接受就宣称全量成功。首次行情可能先加载市场目录精度，耗时不能全归于快照命令。

`helpers.full_quotes()` 会按 80 拆批并组合 054c + 0547。它是多次请求结果，字段可能来自不同响应时刻，不保证形成交易所同一瞬间的全市场截面。

### 3.15 `0x056a`：当日 / 历史集合竞价过程

**用途：** 查询虚拟匹配价格、匹配量与未匹配量的竞价过程。

```python
series = client.auctions.series("sz000001", include_raw=True)
historical = client.auctions.series("sz000001", date="2026-08-14")
```

高层只接受单证券、可选 date 和 include_raw；默认 date=None 表示当日模式。默认底层 selector=3、start=0、limit=500，但高层 **没有 start/count 参数和自动分页循环**。即便结果很多，也不能据方法名承诺永远完整。[cite:2](#cite-2)[cite:8](#cite-8)

| 字段 | 含义 |
| --- | --- |
| Series 的 `trading_date` | 请求历史日；当日模式 None，不是服务端确证日期 |
| `mode_or_selector_raw/start_raw/limit_or_count_raw` | 底层模式、起点、数量参数 |
| `points/count/raw_payload` | 按主站顺序的过程点、点数、原文 |
| Point 的 `index/minute_of_day_raw/second_raw` | 序号 / 当天分钟值 / 秒 |
| `time_label/time_seconds` | 时分秒文本 / 当天秒数 |
| `price/price_milli` | 虚拟撮合价 / 千分表示 |
| `matched_volume` | 虚拟匹配数量，不是已完成成交量 |
| `unmatched_signed_raw` | 有符号未匹配原值 |
| `unmatched_volume` | 上述数值绝对值 |
| `unmatched_direction_raw` | 非负 → 1、负 → -1；零也归 1，不能解释为有未匹配买量 |
| `reserved_zero_0e/record_hex` | 保留字节 / 16 字节原文 |
| `matched_amount_estimated` | 本地 `price × matched_volume × 100`，股票手数假设 |

**重要边界：** 056a 的 09:25 点仍属于过程快照；正式开盘撮合从 0fc5 / 0fc6 中找 **非 status=8 且时间 09:25**。过程点的 matched_volume 不能逐点累加，未匹配正负也应保留，不凭 SDK 的 ±1 强行证明交易所买卖方向语义。

**频率与约束：** 专用连接池按需建立。关注竞价阶段可 3–5 秒查询，结束后归档；历史按单日补一次。记录带秒不意味着每秒都有一条或服务器必定每秒更新；历史深度由专用主站决定。

### 3.16 `0x06b9`：服务器文件分块读取 / 下载

**用途：** 取服务器允许提供的资源文件；上层可拼接整文件并解析统计资料。

```python
chunk = client.resources.read("zhb.zip", offset=0, size=30000)
content = client.resources.download_file(
    "zhb.zip", chunk_size=30000, max_bytes=16 * 1024 * 1024
)
stats = client.resources.read_stats("zhb.zip")
```

| 参数 | 含义 / 边界 |
| --- | --- |
| `path` | 非空 ASCII 服务端路径，最多 300 字节，拒绝 NUL |
| `offset` | u32 文件字节偏移 |
| `size/chunk_size` | 请求块长，1–60000 字节，默认 30000 |
| `max_bytes` | 最多读取的字节数，允许整数 0–4294967295；None 不设此上限 |

`read()` 返回 `FileContentChunk`；`download_file()` 返回 bytes；`read_stats()` 返回 TdxStatsResource。[cite:11](#cite-11)

| 字段 | 含义 |
| --- | --- |
| `path/offset/request_size` | 文件名、本次偏移、请求字节数 |
| `chunk_len/content` | 实际返回长度 / 文件内容 bytes |
| `raw_payload` | 含长度头的原文 |
| `is_last` | 返回长度小于请求长度的派生判断 |

下载按实际长度推进，短块 / 空块结束。文件长度刚好为块大小倍数时，需要再取一次空块才能确认末尾。高层在支持 pin 的 transport 上自动固定连接，并拒绝可观察到的中途主站变化。

**`max_bytes` 是读取上限，不是超量报错保证。** 达到上限会直接返回已读取 bytes，可能只是文件前缀；设为 0 返回空 bytes。需要完整文件时，不能仅因返回成功就标为下载完成。主站同连接中途更换资源也仍可能跨版本，应保存内容摘要、资源日期及完整性依据。

`read_stats()` 只解析已知 zhb.zip 中的 GBK `tdxstat.cfg/tdxstat2.cfg`。主要字段：

| 模型 | 字段 / 单位 |
| --- | --- |
| TdxStatRow | stats_date、beta_60d、pe_ttm、free_float_shares_10k（万股）、年内涨停天数、统计天数 / 次数、连板天数 |
| TdxStat2Row | 当前 / 前一 / 前二日 amount_10k、seal_amount_10k（万元）；当前 / 前日开盘量（手）、开盘额_10k（万元） |
| TdxStatsResource | stat/stat2 字典；row(market_id, code)、行数、日期分布与覆盖率 |

统计资源的 `_10k` 金额乘 10000；与 0010 财务的千元不能混用。资源各行日期可能不同，需要核对 `stats_date` 与目标交易日。[cite:11](#cite-11)

**频率与约束：** 资源版本 / 日期变化时下载；每日统计文件一般当天首次使用取一次，内容不变时本地复用。分块尽量串行，不通过重复下载整个压缩包实现高频行情。

### 3.17 `0x0fb4`：指定日期历史分时

**用途：** 按明确日期补取一个交易日的分时价格与分钟量。

```python
series = client.minutes.history("sz000001", "2026-08-14", include_raw=True)
```

参数为单代码 / 列表、必填 trading_date、include_raw、batch_size；无 start/count。返回 MinuteSeries 或 `{full_code: MinuteSeries}`。[cite:2](#cite-2)[cite:8](#cite-8)

字段沿用 3.10，但有以下差别：

| 字段 | 历史布局特性 |
| --- | --- |
| `trading_date` | 来源于请求日期，不等于服务器额外独立回显并确认该日 |
| `time` | 根据请求日期与分钟序号构造的带日期时间 |
| `prev_close` | 历史昨收参考价 |
| `open_price/avg_price/avg_field/avg_raw` | 该布局可能缺失，None |
| `price_delta_raw/aux_delta_raw/price_raw` | 差分 / 辅助槽 / 差分累计原值 |
| `points/count/volume_sum` | 该日点列表、点数、分钟量之和 |

**频率与约束：** 历史补数一次，归档后本地读取；空结果区分休市、停牌、主站缺历史与请求问题。比“近期接口”更适合尝试较早日期，但不能保证任意年月都有分时。均价或今开缺失时不能用 0 冒充。

### 3.18 `0x0fc5`：当日混合成交明细分页

**用途：** 获取当日普通成交、正式撮合、竞价快照和盘后记录。

```python
page = client.trades.today("sz000001", start=0, count=1800)
all_ticks = client.trades.all_today(
    "sz000001", page_size=1800, max_pages=100
)
real_trades = all_ticks.actual_trades
opening = client.trades.opening_match_today("sz000001")
```

单页 `start` 为 u16：0–65535，0 为最新端；count 为 1–1800，默认 1800。多证券列表返回代码字典，逐证券请求。`all_today()` 默认最多 100 页，空页结束，短页继续；达到保护上限抛错。[cite:2](#cite-2)[cite:9](#cite-9)

| TradePage 字段 | 含义 |
| --- | --- |
| `start/request_count/count` | 请求位置、请求量、实际混合事件数 |
| `trading_date` | 当日协议没有明确日期，可能 None |
| `ticks` | 所有原始混合事件 |
| `actual_trades` | 排除 status=8 的记录，包含开盘 / 收盘正式撮合与盘后成交 |
| `auction_snapshots` | status=8 的过程快照 |
| `opening_matches` | 非 8 且 09:25 的正式开盘撮合 |
| `after_hours_trades` | 15:05–15:30、status=5 的盘后固定价成交 |
| `price_base_raw_f32/raw_payload` | 历史布局参考价槽 / 原文；当日参考价槽通常 None |
| `has_more` | 3.2.2 源码为 `count > 0`；仅表示非空时可继续尝试，不是主站给出的完整性标志 |

| TradeTick 字段 | 含义 / 限制 |
| --- | --- |
| `index/absolute_index` | 页内序号 / 请求偏移加页内序号；不是交易所成交 ID |
| `time_minutes/time_label` | 当天分钟值 / 文本；不能据此还原逐秒事件 |
| `trade_datetime` | 能取得日期时构造，否则 None |
| `price/price_milli` | 价格 / 千分表示 |
| `volume` | 原数量；真实成交按量理解，竞价快照不赋予匹配量语义 |
| `order_count` | 原笔数槽；指数等品种须验证，不代表一条就是一笔交易 |
| `status_raw/side` | 原状态 / SDK 映射：0 buy、1 sell、2 neutral，其他可能 status_N |
| `event_kind` | trade / opening_match / auction_snapshot |
| `price_delta_raw/price_acc_raw` | 价格差分 / 累计原值 |
| `unknown_tail_raw/reserved_zero/record_hex` | 尾槽 / 保留值 / 原文 |
| `is_trade/is_opening_match/is_auction_snapshot` | 对应 event_kind 的判断属性 |
| `is_actual_trade` | status != 8 |
| `is_after_hours_fixed_price` | status=5 且分钟位在 15:05–15:30 |
| `trade_amount_yuan` | `price × volume × 100`；只在真实成交且量为股票手时使用 |
| `auction_matched_volume/auction_unmatched_signed_volume/auction_unmatched_volume` | 此明细不推断竞价数量，None |

**分类顺序必须先看状态：**

```text
status_raw == 8                 → auction_snapshot
status_raw != 8 且时间 09:25    → opening_match
其余                           → trade
```

15:00 非 8 的正式收盘撮合仍属于真实成交；盘后 status=5 也不能按“非普通方向”直接丢掉。`trade_amount_yuan` 属性本身不会自动阻止你对快照计算，要先筛 `is_actual_trade`。[cite:9](#cite-9)

**分页顺序与完整性：** 高层将较早页移到前面，保留每页 wire 顺序，不对同一分钟记录全局重排。返回记录仍保留原始 index/absolute_index，不能依靠它们跨请求唯一去重。盘中不断有新记录加入，偏移分页可能发生重复 / 漏条；全量核对优先盘后固定主站、固定连接、确认日期，保存原始数量与边界。

**频率与约束：** 最新页 3–5 秒；盘后一次全量补齐，需纳入盘后成交时等相应阶段结束。避免盘中每 3 秒重复 `all_today()` 扫全日。底层明细可能聚合多个交易，不能宣称为交易所原始逐单成交或纳秒级流。

### 3.19 `0x0fc6`：历史混合成交明细增强

**用途：** 取指定日的增强成交布局，支持全量与正式开盘撮合筛选。

```python
page = client.trades.history("sz000001", "2026-08-14", count=1800)
full = client.trades.all_history("sz000001", "2026-08-14")
opening = client.trades.opening_match_history("sz000001", "2026-08-14")
```

必填 trading_date，其余页数 / 量 / 原文 / 多证券规则与 3.18 类似；返回 TradePage 或代码字典。**它与旧 `0x0fb5` 不是同一布局，不能互换解码器。**[cite:9](#cite-9)

历史页有 `price_base_raw_f32` 参考价槽；日期和 `trade_datetime` 由请求日构造。TradeTick、状态优先的竞价 / 成交分类及股票成交额估算沿用 3.18。

**批量列返回：** `history_batch()` / `all_history_batch()` 返回 TradeBatch；`today_batch()` / `all_today_batch()` 是当日同类封装，均不新增 opcode。`column(name)` 取 19 个存储字段之一，`select(indices)` 保持列形式，`tick(i)` / `to_page()` 才创建对象；is_actual_trade 等计算属性不是可直接取的存储列。[cite:2](#cite-2)

**频率与约束：** 每日补数一次 / 需要时下载后归档。保存深度依主站而定，`all_history()` 只表示对可用数据尝试翻到空页，不保证交易所全天真实明细无缺口。即使 max_pages=None，u16 偏移、响应边界和数据覆盖仍有限制。

### 3.20 `0x0fd1`：单标的小走势图

**用途：** 列表 / 概览中的轻量价格序列。

```python
series = client.minutes.sparkline("sz000001", selector=1, window=20)
```

仅单证券。selector 为 u8：0–255，window 为 u16：0–65535，默认 1 / 20。这是编码范围，完整 selector 业务枚举与 window 对应的实际时间跨度没有独立确认。[cite:8](#cite-8)

| 字段 | 含义 |
| --- | --- |
| `selector_raw/selector_echo` | 请求 / 回显选择器 |
| `window_or_count_raw` | 请求窗口或数量参数 |
| `max_count_raw` | 返回最大数量相关值，不等于已确认“天数” |
| `base_price` | 价格基准 float |
| `prices/count` | 轻量价格元组 / 实际点数 |
| `reserved_param_u32/raw_payload` | 保留槽 / 原文 |

**频率与约束：** 页面可见时 30–60 秒；无对应的真实逐点时间、量额、均价和 OHLC，不应给每点强行配交易分钟或当成完整 K 线。window=20 不保证正好返回 20 点。

### 3.21 `0x0feb`：近期历史分时

**用途：** 近期窗口内分时，包含昨收、今开及均价等增强字段。

```python
series = client.minutes.recent("sz000001", "2026-08-14")
default_date = client.minutes.recent("sz000001")
```

参数为单代码 / 列表、可选 trading_date、include_raw、batch_size。不传日期使用 **本机当前日期**，并非直接询问“最近交易日”。底层将日期编码为近期 selector：

```text
date_selector_raw = 0xfed62304 - Python 日期 ordinal
```

无需用户手工构造 selector。[cite:8](#cite-8)

| 字段 | 含义 / 近期特性 |
| --- | --- |
| `trading_date` | 请求日期上下文 |
| `prev_close/open_price` | 昨收 / 今开 |
| `date_selector_raw/reserved_zero` | 近期日期 selector / 保留槽 |
| `points/count/volume_sum` | 分时点、实际数量、分钟量之和 |
| Point 的 `time_label/time/price/price_milli/volume` | 分钟标签、构造时间、价、千分价、分钟量 |
| `avg_price/price_field/avg_field/price_raw/avg_raw` | 均价及编码 / 还原原值 |
| `price_delta_raw/aux_delta_raw` | 某些近期布局可能 None |
| `record_hex/raw_payload` | 原文 |

**频率与约束：** 历史每证券每天补一次；若明确用它查询当天，30–60 秒。近多少天由主站决定，没有统一“N 天保证”。默认本机日期在周末可能无数据 / 与主站保存日不一致；应按交易日历显式传日。查更早日期可尝试 0fb4，但其字段和覆盖也不同，不能直接当作等价回退。

### 3.22 `0x0ffc`：日资金流向分档

**用途：** 读取日成交金额与分档统计，计算 SDK 的主力 / 主买净额。

```python
flow = client.money_flow.daily("sz000063", include_raw=True)
many = client.money_flow.daily(
    ["sz000063", "sh600000"], batch_size=2
)
```

单证券通常返回 MoneyFlowBlock；列表返回 MoneyFlowBatch。`batch_size=75` 为最大并发数，要求正整数，并受池容量限制，**不是单报文 75 只证券**。使用专用池；协议请求内部使用 service route=0x7e、channel=0x2d，SDK 自动处理。[cite:2](#cite-2)[cite:11](#cite-11)

没有 date/start/end/history 分页参数。作者文档称通常最近 5 个交易日，这不是固定返回保证。响应为 40 字节证券头 + 每日 88 字节记录；每条含日期和 21 个 u32 原值。

| 字段 | 含义 |
| --- | --- |
| Block 的 `records/count` | 单证券逐日记录 / 日数 |
| `main_net_total/main_buy_net_total` | 对返回窗口中的对应日净额求和 |
| `main_ratio_total/main_buy_ratio_total` | 窗口净额 / 窗口总额 × 100；总额为 0 时 None |
| Batch 的 `blocks/count` | 逐证券结果 / **所有证券日记录总数**；证券数取 len(blocks) |
| Daily 的 `date_raw/date` | 日期原值 / 可解析日期 |
| `total_amount` | 总成交金额，元；源码把记录偏移 8–12 字节解读为 float32 |
| `buckets` | 16 个 u16 原始分档值，非元金额 |
| `main_net/main_ratio` | 主力净额 / 占比；ratio 是百分数值 |
| `main_buy_net/main_buy_ratio` | 主买净额 / 占比，另一个统计口径 |
| `main_buy_super_large_net/main_buy_large_net/main_buy_medium_net/main_buy_small_net` | 主买口径四档净额，元 |
| `main_super_large_net/main_large_net/main_medium_net/main_small_net` | SDK 命名的另一套四档净额，元 |
| `raw/record_hex` | 21 个 u32 原值 / 88 字节原文；数值 raw 不依赖 include_raw |

**源码中的精确计算：** 令 b 为 buckets，A 为 total_amount。

```text
D_main = b0-b1 + b4-b5
D_buy  = b2-b3 + b6-b7 + b10-b11 + b14-b15

main_net       = D_main × A / 50000
main_ratio     = D_main / 500
main_buy_net   = D_buy × A / 50000
main_buy_ratio = D_buy / 500

main_* 四档:
  super_large = (b0-b1)   × A / 50000
  large       = (b4-b5)   × A / 50000
  medium      = (b8-b9)   × A / 50000
  small       = (b12-b13) × A / 50000

main_buy_* 四档:
  super_large = (b2-b3)   × A / 50000
  large       = (b6-b7)   × A / 50000
  medium      = (b10-b11) × A / 50000
  small       = (b14-b15) × A / 50000
```

**文档冲突已核对：** 原方法文档一处写“四个 main_* 净额组成 main_net”，但该版本代码和同文档公式都只用第 0/1 与 4/5 两组计算 main_net。应以代码公式为准：**main_net = main_super_large_net + main_large_net**，而非四项和；主买四项和才等于 main_buy_net。b8/9 与 b12/13 的独立业务命名未充分确认，虽然模型生成 medium/small 属性，仍需保留这种不确定性。[cite:11](#cite-11)

**频率与约束：** 盘后一次归档；盘中观察可从 1–5 分钟起，但当前日值是否即时更新仍需测量。重复轮询最近窗口不能累加窗口总净额，应按 `(full_code, date)` 更新覆盖；通常仅近 5 日，想保留更久需每日落地。

资金流向是主站的成交分类统计。一次交易双方资金对价同时发生，正净额不能解释为证券市场实际凭空增加资金，也不能仅据分档判定某机构真实持仓。

## 4. 调用频率、请求量与并发预算

### 4.1 频率的四个层次

| 层次 | 本手册可以确认的内容 | 不能据此推出的内容 |
| --- | --- | --- |
| 主站数据更新 | 此次未独立测定各命令更新间隔 | 每秒查询一定得到新数据 |
| SDK 行为 | 默认空闲心跳 30 秒；直接 API 通常不缓存 | 所有接口自带限流或自动调度 |
| 服务端限制 | 已取得 SDK 页 / 列表约束；0547 作者报告超过 100 截断 | 官方统一每秒 N 次、长期稳定 SLA |
| 应用采集策略 | 第 1 节是保守起点，可按实测调整 | 建议值等于官方允许最大频率 |

### 4.2 按底层请求计算预算

```text
快照每秒请求 ≈ ceil(证券数 / 每批证券数) / 轮询间隔秒
历史全量请求 ≈ Σ(每证券实际页数 + 终止空页)
文件请求     ≈ 所有分块数 + 必要的结束确认
实际总请求   = 业务请求 + 元数据装载 + 心跳 + 重试 + 补拉
```

仅作计算示例：480 只证券、每批 80 只、每 5 秒一轮基础快照，对应基础快照约 1.2 次/秒；若再为每批取一次五档，基础业务请求约翻倍。这个数字是参数计算，不是实测性能或服务端许可。

`pool_size=8` 只是最多同时在途 8 个普通业务请求；不等于 8 QPS，专用池也需要单独纳入总预算。请求很慢时固定启动新轮次会堆积队列。**采用上一轮结束后再启动下一轮、按主站统计延迟与失败、限制总并发和总请求量**的方式；历史补数与实时查询分预算，避免挤占最新行情。

### 4.3 失败、重试与完整性处理

| 情况 | 建议处理 |
| --- | --- |
| `PoolBusyError` | 减少提交并发、合并重复任务，等待预算恢复 |
| 连续超时 / 断线 | 暂停该主站的新增任务，重连后验证；有限次指数退避，不立即无限重试 |
| 返回少于请求证券数 | 按完整代码核对缺项，记录缺失 / 补拉；不能把少返回当作全成功 |
| 历史空页 | 记录主站与请求日期，区分可用历史结束和异常 |
| `PushOverflowError` | 标记序列缺口，必要时重新取基线 |
| `max_pages` 到上限 | 标记未完成，不能把抛错前的部分结果标为全量 |
| 行情日期未知 | 同时保存请求时间、会话信息、交易日历证据 |
| 量纲未知 | 保存原字段、证券类别、请求周期和单位标记；不盲目 ×100 |

退避间隔可从 1、2、4、8 秒等带小幅随机抖动的策略起步，这是应用建议；SDK 的一次 retry 与高层多页 / 多次任务重试不能混成一个无界循环。

## 5. 常见误用及正确口径

| 误用 | 正确口径 |
| --- | --- |
| 22 个命令就是 22 个终端命令 | 是 7709 opcode；公开调用是模块化 Python API |
| 000d / 0004 也采集行情 | 它们是会话初始化 / 保活 |
| 044e 返回 A 股数量 | 返回完整证券目录数量；A 股计数需目录过滤 |
| 把 054c 当作原生五档 | 原生稳定确认一档；0547 / Helper 补五档 |
| 每个 quotes API 最大 80 且自动拆批 | 0547 是 100 明确限制；直接 053e / 054c 没有该专用校验；Helper 另拆批 |
| `all_pages=True` 就一定全部历史且升序 | 仍受主站、页数、偏移限制；K 线多页需主动排序核对 |
| 所有 `batch_size=75` 都是 75 只/报文 | 股本是批次证券数；资金流向是并发证券数 |
| 所有财务金额 raw 都 ×10000 | 3.2.2 财务金额按千元 ×1000；股本万股 ×10000 |
| `volume_lots` 一定正确适配全部品种 | 固定 wire 解码值 /100；必须独立核验量纲 |
| 09:25 点一律正式成交 | 先排 status=8；056a 仍是过程快照 |
| 把竞价 matched_volume 各点求和 | 是各时刻虚拟匹配状态，不能当作增量成交 |
| 所有 tick 直接加量 / 额 | 先按事件分类，再检查品种量纲 |
| `main_net` 等于四个 main_* 的和 | 3.2.2 实际只加超大 / 大单两项；主买四项相加 |
| 近期 / today 名称保证服务器日期 | 日期可能是请求上下文或缺失，需要额外核对 |
| `clear_cache()` 让服务器必然最新 | 只清 SDK 的相关缓存，不改变主站更新进度 |

## 6. 使用范围与许可约束

**3.2.2 发布包使用 `ELTDX Research-Only License`，不是 MIT / Apache 许可。** 包内 LICENSE 允许个人研究目的的阅读、使用、复制和修改，并明确限制项目、源码、衍生作品及通过项目取得的数据用于商业活动、付费服务、生产服务、转售、营利分发、行情售卖与自动交易服务等。[cite:12](#cite-12)

这意味着将该包直接部署成生产行情服务，或复制其源码进入生产产品，都需要先核对并取得相应权利。此处是发布包条款摘录，不是对本仓库自身许可证的重新定义。访问第三方服务和使用数据还受相应服务协议与数据权利约束。

## 7. 证据、版本与复核

本次重新下载固定源码包并校验：

```text
package: eltdx 3.2.2
SHA256: 5ef7c8750feaa26fa001c8e5ee0d5952ba8c424b3745ae7f66661503f1d3b955
```

[核对证据 JSON](../../testing/evidence/eltdx-3.2.2-command-guide-20261024.json)保存 22 命令表、81 个固定源码 / 文档摘要、45 个模型定义与 12 个 API 类的字段 / 方法签名，便于查精确字段名、类型和源码行号。[源码包](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/eltdx-3.2.2.tar.gz)也随资料保存。

主要来源均来自该已校验的 3.2.2 发布包。仅将它们用于证明 **该 SDK 版本如何实现**，没有以多份同包文档冒充独立服务器验证。作者文档中的真实样本是作者报告，未记为本次实测。

<a id="cite-1"></a> **[cite:1] 命令表与实际注册。** [COMMANDS_7709.md](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/docs/COMMANDS_7709.md)、[request.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/request.rs)。清单和注册都为 22，旧测试名 twenty_one 不改变数量。

<a id="cite-2"></a> **[cite:2] 固定 Python API。** [api 目录](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/api)、[原 API_REFERENCE](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/docs/API_REFERENCE.md)。签名、批量、字段过滤、分页实际以各 .py 为准。

<a id="cite-3"></a> **[cite:3] 连接、缓存与空闲心跳。** [client.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/client.py)、[slot.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-runtime/src/slot.rs)、[API_REFERENCE](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/docs/API_REFERENCE.md)。

<a id="cite-4"></a> **[cite:4] 标的、日期、周期及参数。** [protocol unit.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/unit.rs)、[Python request bridge](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-python/src/request.rs)。

<a id="cite-5"></a> **[cite:5] 会话 / 特殊限制编解码。** [session.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/session.rs)、[limits.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/limits.rs)。

<a id="cite-6"></a> **[cite:6] 股本与财务字段 / 单位。** [corporate model](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/corporate.py)、[corporate codec](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/corporate.rs)、[股本方法](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/docs/methods/7709-股本变迁GBBQ.md)、[财务方法](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/docs/methods/7709-财务基础信息.md)。

<a id="cite-7"></a> **[cite:7] 目录、计数与通用限制。** [security.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/security.rs)、[协议 limits.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/limits.rs)。

<a id="cite-8"></a> **[cite:8] 分时、小图、竞价。** [minutes.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/minutes.rs)、[auctions.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/auctions.rs)、[minute model](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/minute.py)、[auction model](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/auction.py)。

<a id="cite-9"></a> **[cite:9] K 线、成交、分类与分页。** [klines.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/klines.rs)、[trades.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/trades.rs)、[trade model](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/trade.py)、[bars.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/api/bars.py)、[trades.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/api/trades.py)。

<a id="cite-10"></a> **[cite:10] 三种快照、增量与分类行情。** [quotes.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/quotes.rs)、[quote model](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/quote.py)、[quotes.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/api/quotes.py)。

<a id="cite-11"></a> **[cite:11] 文件、统计资料与资金流向。** [resources.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/resources.rs)、[resources.py](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/api/resources.py)、[resource model](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/src/eltdx/models/resource.py)、[money_flow.rs](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/crates/eltdx-protocol/src/commands/money_flow.rs)、[资金流向方法](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/docs/methods/0x0ffc-资金流向日数据接口.md)。

<a id="cite-12"></a> **[cite:12] 发布版本与许可。** [PyPI 3.2.2](https://pypi.org/project/eltdx/3.2.2/)、[固定包 LICENSE](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/LICENSE)、[pyproject.toml](../../testing/evidence/eltdx-3.2.2-command-guide-20261024/pyproject.toml)。
