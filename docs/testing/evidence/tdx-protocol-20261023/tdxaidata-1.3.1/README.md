# tdxaidata

通达信 TdxAiData 行情 / 专业数据 Python 数据接口（**跨平台：Windows / macOS / Linux**）。

基于 TdxAiData 原生动态库，用 `ctypes` 直接调用，对齐 `tqcenter` 命名，提供行情、证券基础信息、板块、交易日、专业数据、除权除息、股本、涨跌停、分笔、分时与实时订阅能力。**不依赖** `tqcenter.py`、`TPyth.dll`、策略运行环境或通达信客户端交互。

## 特性

- 跨平台：Windows（`TdxAiData.dll`）/ macOS（`libTdxAiData.dylib`）/ Linux（`libTdxAiData.so`），随包自动匹配
- 与 tqcenter 一致的接口命名：`get_market_data`、`get_market_snapshot`、`get_financial_data`、`subscribe` 等
- K 线、除权除息与分笔行情在装有 pandas 时返回 `DataFrame`
- 可选 `TDX_AI_DATA_LIB` 环境变量指定动态库路径

## 安装

```bash
pip install tdxaidata
```

可选：pandas 支持

```bash
pip install tdxaidata[pandas]
```

## 快速开始

```python
from tdxaidata import tqs

# 实时行情快照
snap = tqs.get_market_snapshot("600000.SH", ["Name", "Price", "Close"])
print(snap)

# K 线（需 pandas）
market = tqs.get_market_data(
    field_list=["Close", "Volume"],
    stock_list=["600000.SH", "000001.SZ"],
    period="1d",
    count=100,
    dividend_type="front",
)
print(market["Close"].tail())

# 订阅实时行情（保持进程运行）
import time

def on_quote(data):
    print("[行情]", data)

tqs.subscribe(stock_list=["000001.SZ", "600000.SH"], callback=on_quote)
time.sleep(30)
tqs.unsubscribe()
```

## 配置（必读）

随包提供 `TdxAiData.ini`（位于包内 `tdxaidata/lib/`），默认指向通达信官方 AI 数据服务（行情 `aihs.tdx.com.cn:7709`、数据 `aids.tdx.com.cn:7727`）。**Token 未预填**，安装后必须按实际环境修改：

- `[TcHqHost]` / `[TcDsHost]`：行情 / 数据服务地址（默认已指向官方域名，一般无需改动）
- `[Token]`：填写 `token=`（**必填**，登录通达信个人版商城 → 会员中心 → 积分和Key管理 → 创建数据服务 Key；缺失或错误会导致连接 / 认证失败）

> 动态库初始化时读取其所在目录（`tdxaidata/lib/`）的 `TdxAiData.ini`。也可以不改模板，通过环境变量 `TDX_AI_DATA_LIB` 指向外部动态库目录。

## 动态库查找顺序

1. 构造 `_NativeTdxAiData(dll_path)` 显式传入的路径
2. 环境变量 `TDX_AI_DATA_LIB`
3. 包内 `tdxaidata/lib/` 下的平台对应动态库（`TdxAiData.dll` / `libTdxAiData.dylib` / `libTdxAiData.so`）
4. 模块同目录（源码部署兼容）

## 接口概览

- 行情与基础信息：`get_market_data` / `get_divid_factors` / `get_stock_info` / `get_market_snapshot` / `get_more_info`
- 证券列表、板块与交易日：`get_stock_list` / `get_sector_list` / `get_sector_list_ds` / `get_stock_list_in_sector` / `get_trading_dates` / `get_zzgz_stocklist`
- 集合竞价：`get_call_auction` / `get_call_auction_batch`
- 集合排名行情：`get_domain_sorthq`（按指定栏目在某一市场 / 板块 / 指数成分域内排序取数，可附加返回其他栏目）
- 市场整体情况：`get_global_data`（`type=0` 沪深京市场的成交额、涨跌幅、涨跌家数、均线上下方家数等整体统计；`type=1` 港股与境外指数、商品、股指期货行情）
- 个股综合信息：`get_misc_data`（单只证券的行业地位、主营业务、所属板块、最新分红等已格式化中文栏目）
- 专业数据：`get_financial_data` / `get_financial_data_by_date` / `get_gpjy_value(_by_date)` / `get_bkjy_value(_by_date)` / `get_scjy_value(_by_date)` / `get_gp_one_data`
- 关系 / 可转债 / ETF / 涨跌停 / 股本：`get_relation` / `get_kzz_info` / `get_trackzs_etf_info` / `get_zdt_data` / `get_gb_info(_by_date)` / `get_exday_data`
- 分笔 / 分时：`get_tick_data` / `get_minute_data`
- 公式批量计算：`formula_process_mul_zb` / `formula_process_mul_xg` / `formula_process_mul_exp`（批量调用客户端公式）、`formula_by_src` / `formula_by_file`（临时源码公式，不保存到客户端）
- 订阅：`subscribe` / `unsubscribe` / `get_subscribe_hq_stock_list`
- 文件下载：`download_file`（HTTP，不依赖动态库）

> 完整参数说明与字段参考见包内 `tdxaidata/docs/tdxaidata使用文档.md`（随 wheel/sdist 分发；源码仓库另附 `SKILL.md`）。

## 版本更新说明

- **1.3.1**：新增个股综合信息接口 `get_misc_data`；`get_global_data` 支持 `type=1`；批量公式与临时源码公式接口（`formula_process_mul_zb` / `formula_process_mul_xg` / `formula_process_mul_exp` / `formula_by_src` / `formula_by_file`）新增 `data_source` 数据源参数；公式接口 `count` 增加 24000 上限校验；修复调用接口后进程工作目录被修改的问题
- **1.3.0**：新增市场整体情况接口 `get_global_data`；新增批量公式接口 `formula_process_mul_zb` / `formula_process_mul_xg` / `formula_process_mul_exp` 与临时源码公式接口 `formula_by_src` / `formula_by_file`（均新增 `domain` 市场分类域参数）；集合竞价接口新增日期参数 `date`；`get_tick_data` 在装有 pandas 时返回 `DataFrame`
- **1.2.2**：新增集合排名行情接口 `get_domain_sorthq`、扩展市场板块列表接口 `get_sector_list_ds`；集合竞价接口新增时段参数 `type`，返回结果不再包含 `InOutFlag` 字段
- **1.1.1**：修复了 1.1.0 版本动态库文件不匹配问题；修复部分已知问题，优化使用体验
- **1.1.0**：新增集合竞价接口 `get_call_auction`，以及批量取数接口 `get_call_auction_batch` / `get_market_snapshot_batch` / `get_stock_info_batch` / `get_more_info_batch` / `get_kzz_info_batch`
- **1.0.2**：随包实现模块 `tqserver.py` 更名为 `tdxaidata.py`（顶层 `from tdxaidata import tqs` 用法不变）；支持 Python 3.7+
- **1.0.1**：模块与使用文档统一更名为 `tdxaidata`（含 `download_file` 的 User-Agent）；使用文档改为 pip 直连 `import` 模式；`get_more_info` 新增 4 个期货/期权字段（`PreVolIn` / `VolInStock` / `ClearPrice` / `PreMinum`）
- **1.0.0**：首次发布

> 完整变更记录见 `CHANGELOG.md`（随 wheel/sdist 分发，位于包内 `tdxaidata/docs/CHANGELOG.md`）。

## 错误处理

新版多数错误（库未加载、返回空 / 无效 JSON、`ErrorId != 0`、参数非法等）会**打印 `[错误]` 提示并返回空值**（`None` / `{}` / `[]`），调用后请检查返回值。

## 许可

MIT License
