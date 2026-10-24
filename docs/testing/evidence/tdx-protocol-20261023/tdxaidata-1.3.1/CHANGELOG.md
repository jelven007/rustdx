# 更新说明（Changelog）

## 1.3.1（2026-10-23）
- **新增个股综合信息接口**：
- `get_misc_data(stock_code, field_list=None)` —— 获取个股综合信息情况，返回 `{栏目名: 内容}` 的扁平字典，`content` 为已格式化的中文串（如 `2438.48万股` / `0.13%`）。`stock_code` 需带市场后缀（如 `000001.SZ`）；`field_list` 按中文栏目名筛选，留空返回全部。返回栏目随个股不同，重复栏目名只保留首个。
- **`get_global_data` 支持 `type=1`**：`type=0`（默认）取沪深京市场整体统计（87 个字段），`type=1` 取港股与境外指数、商品、股指期货行情（53 个字段，取值均为字符串）；`type` 非 `0` / `1` 时打印提示并返回 `{}`。
- **批量公式与临时源码公式接口新增数据源参数**：
- `formula_process_mul_zb` / `formula_process_mul_xg` / `formula_process_mul_exp` 新增 `data_source=0`。
- `formula_by_src` / `formula_by_file` 新增 `data_source=0`。
- `data_source=0`（默认）取沪深京市场数据，`data_source=1` 取期货期权、港股美股等扩展行情；取值非 0/1 时打印提示并返回 `{}`。
- **公式接口 `count` 增加上限校验**：单只证券取数条数不得超过 24000，超过时打印提示并返回 `{}`；`count=-1` 仍表示加载全部。
- **修复调用接口后进程工作目录被修改的问题**：动态库改为在 `start()` 时接收库目录，数据请求不再按进程 cwd 查找 `TdxAiData.ini` / `NewTc.dat`，调用方工作目录前后保持一致。

## 1.3.0（2026-10-16）

- **新增市场整体情况接口**：
  - `get_global_data(type=0)` —— 获取沪深京市场的整体运行统计，含各指数成交额与涨幅、行业与概念板块涨跌家数、5 日均线与年线上下方家数等 67 个字段；`type` 当前仅支持 `0`（沪深京市场）。
- **新增批量公式接口（3 个）**：
  - `formula_process_mul_zb(formula_name, ...)` —— 批量调用指标公式。
  - `formula_process_mul_xg(formula_name, ...)` —— 批量调用条件选股公式。
  - `formula_process_mul_exp(formula_name, ...)` —— 批量调用专家系统公式。
- **新增临时源码公式接口（2 个）**：
  - `formula_by_src(formula_src, ...)` —— 直接传入公式源码批量计算，公式仅在本次调用临时生效、不保存到客户端。
  - `formula_by_file(formula_file, ...)` —— 从本地文本文件读取公式源码批量计算。
- **集合竞价接口新增日期参数**：
  - `get_call_auction(stock_code, type=1, date="")`
  - `get_call_auction_batch(stock_list, field_list, type=1, date="", return_df=False)`
  - `date` 支持 `YYYYMMDD` 与 `YYYY-MM-DD` 两种写法，传空取当日；该参数按原值透传，不做格式转换。
- **`get_tick_data` 返回格式改为 `DataFrame`**：已安装 pandas 时返回 DataFrame（列 `Time` / `Price` / `Volume` / `BSFlag`，默认整数索引，各列均为字符串），未安装 pandas 时仍返回原 `dict`。标量字段（如 `TotalNum`）不再参与转换，分页口径相应改为 `len(df)`。

## 1.2.2（2026-09-30）

- **新增集合排名行情接口**：
  - `get_domain_sorthq(domain=6, colname="ZAF", sorttype=1, wantcol=[], startxh=0, wantnum=100, data_source=0)` —— 按指定排序栏目获取某一市场、板块或指数成分域内证券的排名行情，可附加返回最多 5 个其他栏目；`data_source=1` 时取期货期权、港股美股等扩展行情。
- **新增扩展市场板块列表接口**：
  - `get_sector_list_ds(list_type=0)`
- **集合竞价接口新增时段参数**：
  - `get_call_auction(stock_code, type=1)` —— 单只证券当日集合竞价数据。
  - `get_call_auction_batch(stock_list, field_list, type=1, return_df=False)` —— 批量获取集合竞价数据。
  - `type=0` 返回早盘与尾盘两段，`type=1`（默认）只返回早盘。
- **集合竞价返回结果不再包含 `InOutFlag` 字段**。

## 1.1.1（2026-09-20）

- 修复了 1.1.0 版本动态库文件不匹配问题。
- 修复部分已知问题，优化使用体验。

## 1.1.0（2026-09-18）

- **新增集合竞价接口**：
  - `get_call_auction(stock_code)` —— 单只证券当日集合竞价逐笔虚拟撮合数据，含开盘（09:15-09:25）与收盘（14:57-15:00）两段。
- **新增批量取数接口（5 个）**：
  - `get_call_auction_batch(stock_list, field_list)` —— 批量获取集合竞价数据。
  - `get_market_snapshot_batch(stock_list, field_list)` —— 批量获取实时行情快照，实测 26 个字段。
  - `get_stock_info_batch(stock_list, field_list)` —— 批量获取股票基本财务数据，实测 62 个字段。
  - `get_more_info_batch(stock_list, field_list)` —— 批量获取股票扩展数据，实测 92 个字段。
  - `get_kzz_info_batch(stock_list, field_list)` —— 批量获取可转债基础信息，实测 27 个字段。

## 1.0.2（2026-09-04）

- **包内模块改名**：随包实现模块由 `tqserver.py` 更名为 `tdxaidata.py`，顶层用法不变（仍为 `from tdxaidata import tqs`）。
- **Python 版本支持**：声明支持 Python 3.7 及以上（`requires-python = ">=3.7"`，classifiers 增加 3.7）。

## 1.0.1（2026-09-04）

- **命名统一**：随包核心模块与使用文档由旧名 `tqServer` 统一更名为 `tdxaidata`。
- **接口字段更新**：`get_more_info`（扩展证券信息）新增 4 个期货/期权字段并同步到文档字段表：
  - `PreVolIn` —— 昨持仓量（期货期权有效）
  - `VolInStock` —— 持仓量（期货期权有效）
  - `ClearPrice` —— 结算价（期货期权有效）
  - `PreMinum` —— 基差/溢价

## 1.0.0（2026-09-03）

- 首次发布至 PyPI。基于 TdxAiData 原生动态库（Windows `TdxAiData.dll` / macOS `libTdxAiData.dylib` / Linux `libTdxAiData.so`）的跨平台行情 / 专业数据 Python 接口。
- 提供行情快照、K 线、证券基础信息、板块、交易日、专业数据、除权除息、股本、涨跌停、分笔、分时、实时订阅、文件下载等 30+ 接口，命名对齐 `tqcenter`。
- 随包分发三平台动态库与 `TdxAiData.ini` 配置模板；无硬依赖，pandas 为可选增强（`pip install tdxaidata[pandas]`）。
