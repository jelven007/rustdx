# tdxaidata 使用文档
> 文档版本：2026-10-23（对应 tdxaidata 1.3.1）
>
> 本文档面向通过 PyPI 包 `tdxaidata`（`pip install tdxaidata`，包内含 Windows / macOS / Linux 各平台动态库）调用 `tqs` 的 Python 开发者。接口返回示例用于说明结构，实际字段和值会随代码、日期和服务端数据变化。

## 文档范围

`tdxaidata` 提供基于 TdxAiData 动态库（随 PyPI 包分发：Windows `TdxAiData.dll` / macOS `libTdxAiData.dylib` / Linux `libTdxAiData.so`）的行情、证券基础信息、板块、交易日、专业数据、除权除息、股本、涨跌停、分笔、集合竞价与批量取数和实时订阅能力。它不依赖 `tqcenter.py`，也不提供交易、公式执行、自选板块管理等 `tqcenter` 专属能力；`download_file` 是通过 HTTP 实现的文件下载能力，不依赖客户端 RPC。

本文档包含：

- 总体架构、运行依赖和调用方式。
- 已实现 `tqs` 对外接口的函数签名、入参、出参、调用示例和返回结构示例。
- 股票代码、日期、周期、复权和字段筛选等公共约定。

## 快速开始

最小调用：

```python
from tdxaidata import tqs

result = tqs.get_market_snapshot("600000.SH", ["Name", "Price", "Close"])
print(result)
```

如果脚本需要订阅实时行情，必须保留进程并通过回调接收数据；订阅接口不会自动替调用者管理业务循环。

---
## 何时使用

- 用 `tqs` 类从 TdxAiData 动态库（随包分发：Windows `TdxAiData.dll` / macOS `libTdxAiData.dylib` / Linux `libTdxAiData.so`）取**行情、基础信息、专业数据、分笔、订阅实时行情**；
- 生成、修改、审查基于 `tqs` 的 **Windows / macOS / Linux 跨平台 Python 数据脚本**；
- **不适用**：`tqcenter` / `TPyth` / 交易 / 通达信客户端交互（见文末「禁止生成的接口」）；公式仅支持批量计算（见 §8.9、§8.10）。

---

## 安装与首次使用

`tdxaidata` 通过 PyPI 发行（包内已含各平台动态库与 `TdxAiData.ini`），安装到当前 Python 库目录（`site-packages`）后**直接 import 即可**，无需配置目录或插入 `sys.path`：

1. **安装 / 更新**：未安装 → `python -m pip install tdxaidata -i https://pypi.tuna.tsinghua.edu.cn/simple`；已安装 → 用 `python -m pip index versions tdxaidata -i https://pypi.tuna.tsinghua.edu.cn/simple` 检查最新版本，落后 → `python -m pip install -U tdxaidata -i https://pypi.tuna.tsinghua.edu.cn/simple`；验证：`python -c "import tdxaidata; print(tdxaidata.__version__)"`。
2. **确认 Token**：动态库初始化时读取包内 `lib/TdxAiData.ini`（`site-packages\tdxaidata\lib\`）的 `[Token]` 节，默认 `token=` 为空会导致认证失败；须先在 `[Token]` 节填写 `token=`（及 `user=`）。
3. **调用**：`from tdxaidata import tqs` 直接使用下文接口。

---

## 标准接入代码（每次脚本必带）

```python
# -*- coding: utf-8 -*-
from tdxaidata import TqsError, tqs
```

运行前确认（可选）：

```python
import configparser
from pathlib import Path

import tdxaidata  # 未安装 / ImportError → 先执行「安装与首次使用」第 1 步

# Token 检查：动态库初始化时读取包内 lib/TdxAiData.ini 的 [Token] 节，token 为空会导致认证失败
_pkg_dir = Path(tdxaidata.__file__).resolve().parent
_ini = configparser.ConfigParser()
_ini.read(_pkg_dir / "lib" / "TdxAiData.ini", encoding="utf-8")
token = _ini.get("Token", "token", fallback="").strip()
if not token:
    raise RuntimeError(
        "tdxaidata 包内 TdxAiData.ini 未配置 Token：请编辑 "
        "site-packages\\tdxaidata\\lib\\TdxAiData.ini，在 [Token] 节填写 token=（及 user=）后重试"
    )
```

统一错误处理（所有 `tqs` 调用包这层）：

```python
try:
    result = tqs.get_market_snapshot("600000.SH")
    print(result)
except TqsError as exc:
    print(f"[接口错误] {exc}")
except ValueError as exc:
    print(f"[参数错误] {exc}")
except OSError as exc:
    print(f"[DLL 原生错误] winerror={getattr(exc, 'winerror', None)}: {exc}")
```

---

## 公共约定

| 项目           | 规则                                                               |
| ------------ | ---------------------------------------------------------------- |
| 股票代码         | 带市场后缀字符串：`600000.SH`、`000001.SZ`、`430047.BJ`                     |
| 日期           | 接受 `YYYYMMDD`、`YYYY-MM-DD`、`YYYY-MM-DD HH:MM:SS`；模块归一化为 DLL 所需格式 |
| `field_list` | 空 `[]` 保留 DLL 全部字段；非空时仅保留指定字段                                    |
| 列表入参         | 必须传 Python `list`，如 `['600000.SH', '000001.SZ']`                 |
| 返回样例         | 下文 JSON / DataFrame 均为结构示例，数值与可用字段由实际 DLL 数据决定                   |
| pandas       | 已装 pandas 时，K 线与除权除息返回 DataFrame；未装时按接口返回 dict / list            |
| `return_df`  | 仅批量接口（§8）支持，默认 `False`；设为 `True` 且已装 pandas 时返回以代码为索引的 DataFrame   |

#### 市场代码后缀（官方常量）

| 后缀      | 市场      | 后缀      | 市场     |
| ------- | ------- | ------- | ------ |
| `.SZ`   | 深圳交易所   | `.SH`   | 上海交易所  |
| `.BJ`   | 北京交易所   | `.NQ`   | 新三板    |
| `.SHO`  | 上海个股期权  | `.SZO`  | 深圳个股期权 |
| `.HK`   | 香港交易所   | `.US`   | 美国股票   |
| `.CSI`  | 中证指数    | `.CNI`  | 国证指数   |
| `.HG`   | 国内宏观指标  | `.CFF`  | 中金期货   |
| `.CZC`  | 郑州期货    | `.DCE`  | 大连期货   |
| `.SHF`  | 上海期货    | `.GFE`  | 广州期货   |
| `.INE`  | 上海能源    | `.HI`   | 港股指数   |
| `.OF`   | 开放式基金净值 | `.CFFO` | 中金所期权  |
| `.CZCO` | 郑州期货期权  | `.DCEO` | 大连期货期权 |
| `.SHFO` | 上海期货期权  | `.GFEO` | 广州期货期权 |
| `.QHZ`  | 期货类指数   |         |        |

#### K 线周期（period，官方常量）

| period | 说明          |
| ------ | ----------- |
| `1m`   | 1 分钟        |
| `5m`   | 5 分钟        |
| `15m`  | 15 分钟       |
| `30m`  | 30 分钟       |
| `1h`   | 60 分钟（1 小时） |
| `1d`   | 1 天         |
| `1w`   | 1 周         |
| `1mon` | 1 月         |
| `1q`   | 1 季         |
| `1y`   | 1 年         |
| `tick` | 分笔          |

#### 复权类型（dividend_type，官方常量）

| 值       | 说明  |
| ------- | --- |
| `none`  | 不复权 |
| `front` | 前复权 |
| `back`  | 后复权 |

---

## 接口目录

> 每个接口给出：**签名 → 关键参数 → 返回形状 → 最小示例**。详细出参字段含义（尤其 `FN*/GP*/BK*/SC*/GO*` 与 `get_more_info` 字段）见 §9 字段参考。

### 一、行情与基础信息接口

#### 1.1 `get_market_data` — K 线 / 分钟线 / 历史行情

```python
tqs.get_market_data(field_list: list[str] = [], stock_list: list[str] = [],
                    period: str = "", start_time: str = "", end_time: str = "",
                    count: int = -1, dividend_type: str | None = None,
                    fill_data: bool = True) -> dict
```

| 参数              | 必填 | 类型          | 说明                                                                       |
| --------------- | -: | ----------- | ------------------------------------------------------------------------ |
| `field_list`    |  否 | `list[str]` | 需返回的字段列表；空列表返回所有字段                                                       |
| `stock_list`    |  是 | `list[str]` | 股票代码列表                                                                   |
| `period`        |  是 | `str`       | K线周期                                                                     |
| `start_time`    |  否 | `str`       | 开始时间                                                                     |
| `end_time`      |  否 | `str`       | 结束时间；未传则默认当前时间                                                           |
| `count`         |  否 | `int`       | `count>0`：取截止 `end_time` 最近 n 条；`count<=0`：使用 `start_time`/`end_time` 区间 |
| `dividend_type` |  否 | `str`       | 复权类型：`'none'`（不复权）、`'front'`（前复权）、`'back'`（后复权）                          |
| `fill_data`     |  否 | `bool`      | 是否向前填充缺失数据，默认 `True`                                                     |

返回：`Dict`，键为字段名，值为 `pd.DataFrame`（行=时间，列=股票代码）；无 pandas 时为 `field -> {code: [values]}`。
  
**注意：**

- `count>0` 时 `start_time` 失效，`end_time` 若未传默认为当前时间
- 后复权数据与取的数据个数有关，只在返回的数据中进行后复权
- `dividend_type=None`（Python None，非字符串）时默认不复权
- 开高低收单位为元，成交量单位为量的最小单位，成交额单位为万元
- `ForwardFactor` 必须用 DLL 原始值；整列 `0` 时保持全 `0`，不得自行推算

```python
market = tqs.get_market_data(
    field_list=["Open", "High", "Low", "Close", "Volume", "Amount", "ForwardFactor"],
    stock_list=["600000.SH", "000001.SZ"],
    period="1d", start_time="2025-01-01", end_time="2025-01-31",
    dividend_type="front",
)
print(market["Close"].tail())
```

#### 1.2 `get_divid_factors` — 除权除息数据

```python
tqs.get_divid_factors(stock_code: str, start_time: str = "", end_time: str = "") -> DataFrame | list
```

| 参数                        | 必填 | 说明      |
| ------------------------- | -: | ------- |
| `stock_code`              |  是 | 单只股票代码  |
| `start_time` / `end_time` |  否 | 闭区间日期筛选 |

返回：pandas 下以 `Date` 为索引的 DataFrame，列 `Type`/`Bonus`（每10股分红）/`AllotPrice`（配股价）/`ShareBonus`（送股比例）/`Allotment`（配股比例）；无 pandas 为 `list[dict]`（`Date` 为字符串）。

```python
factors = tqs.get_divid_factors("600000.SH", "2000-07-06", "2020-07-23")
print(factors)
```

#### 1.3 `get_stock_info` — 证券基础信息

```python
tqs.get_stock_info(stock_code: str, field_list: list[str] = []) -> dict
```

| 参数           | 必填 | 类型     | 说明                     |
| ------------ | -: | ------ | ---------------------- |
| `stock_code` |  是 | `str`  | 单个证券代码，如 `'688318.SH'` |
| `field_list` |  是 | `list` | **不能为空，必须明确指定字段列表**    |

返回：`Dict`，证券基础信息字典，完整返回字段（84个）见 §9 F.4。

```python
info = tqs.get_stock_info("600000.SH", ["Code", "Name", "Market"])
print(info)
```

#### 1.4 `get_market_snapshot` — 实时行情快照

```python
tqs.get_market_snapshot(stock_code: str, field_list: list[str] = []) -> dict
```

| 参数           | 必填 | 类型     | 说明                       |
| ------------ | -: | ------ | ------------------------ |
| `stock_code` |  是 | `str`  | 单个证券代码，如 `'300505.SZ'`   |
| `field_list` |  否 | `list` | 指定返回字段（大小写不敏感）；空列表返回所有字段 |

返回：实时行情字典，字段含 `Now`/`Open`/`High`/`Low`/`Volume`/`Amount`/`LastClose`/`Bid1`/`Ask1` 等（完整字段表见 §9 F.3）。

```python
snap = tqs.get_market_snapshot("600000.SH", ["Now", "Open", "High", "Low", "Volume", "Amount"])
print(snap)
```

#### 1.5 `get_more_info` — 扩展证券信息

```python
tqs.get_more_info(stock_code: str = "", field_list: list[str] = []) -> dict
```

| 参数           | 必填 | 类型     | 说明                     |
| ------------ | -: | ------ | ---------------------- |
| `stock_code` |  是 | `str`  | 单个证券代码，如 `'688318.SH'` |
| `field_list` |  否 | `list` | 为空返回所有字段               |

返回：`Dict`，获取股票更多扩展信息，包含涨幅系列、主力净额、L2数据、封单、PE/PB/市值等上百个字段（`ZTPrice`/`DTPrice`/`ZAF`/`fHSL`/`Zjl`…），见 §9 F.5。

```python
more = tqs.get_more_info("600000.SH", ["Code", "Name", "Industry"])
print(more)
```

#### 1.6 `get_misc_data` — 个股综合信息情况

按单只证券返回一组已格式化的中文栏目（行业地位、主营业务、所属板块、最新分红、质押与解禁等），数据来自服务端个股综合信息库，**不支持批量**，多只证券请循环调用。

```python
tqs.get_misc_data(stock_code: str = "", field_list: list[str] = None) -> dict
```

| 参数         | 必填 | 类型        | 说明                                                                  |
| ------------ | ---- | ----------- | --------------------------------------------------------------------- |
| `stock_code` | 是   | `str`       | 证券代码，必须带市场后缀，如 `"000001.SZ"`、`"600000.SH"`             |
| `field_list` | 否   | `list[str]` | 按**中文栏目名**筛选，如 `["所属板块", "最新分红"]`；留空返回全部栏目 |

返回：`{栏目名: 内容}`。栏目集合与顺序由服务端模板决定，**不同证券可能不同**（实测 `000001.SZ` 为 12 个栏目，完整栏目见 §9 F.19）；`内容` 均为已格式化的可读文本（如 `2438.48万股`、`0.13%`、`10派2.49元(含税)`），不做数值转换。

`stock_code` 为空时打印 `[错误]` 并返回 `{}`；代码不符合 `<code>.<SH|SZ>` 形式（缺后缀、含空格等）由服务端返回 `Invalid stock code`，同样返回 `{}`；服务端无该证券的综合信息文件时返回 `sim file not found`，同样返回 `{}`。`field_list` 中的栏目名须与返回键完全一致，不存在的栏目会被静默忽略。

```python
data = tqs.get_misc_data(stock_code="000001.SZ")
print(data["所属板块"])
print(data["最新分红"])

# 只取指定栏目
brief = tqs.get_misc_data(stock_code="000001.SZ", field_list=["行业地位", "所属主题"])
print(brief)
```

### 二、证券列表、板块与交易日接口

#### 2.1 `get_stock_list` — 证券列表

```python
tqs.get_stock_list(market: str | None = None, list_type: int = 0) -> list | dict
```

| 参数          | 必填 | 类型    | 说明                                                       |
| ----------- | -: | ----- | -------------------------------------------------------- |
| `market`    |  否 | `str` | 市场/分类，默认为全部A股（`'5'`），完整取值见下                              |
| `list_type` |  否 | `int` | `0`=只返回代码，`1`=返回代码和名称（`[{'Code':..., 'Name':...}, ...]`） |

**market 参数完整说明：**

```
默认为全部A股（market='5'）

0:自选股  1:持仓股
5:所有A股  6:上证指数成份股  7:上证主板  8:深证主板  9:重点指数
10:所有板块指数  11:缺省行业板块  12:概念板块  13:风格板块  14:地区板块
15:缺省行业分类+概念板块  16:研究行业一级  17:研究行业二级  18:研究行业三级
21:含H股  22:含可转债  23:沪深300  24:中证500  25:中证1000
26:国证2000  27:中证2000  28:中证A500
30:REITs  31:ETF基金  32:可转债  33:LOF基金  34:所有可交易基金
35:所有沪深基金  36:T+0基金
49:金融类企业  50:沪深A股  51:创业板  52:科创板  53:北交所 56:沪深股通 57:融资融券
91:ETF追踪的指数
92:国内期货主力合约
101:国内期货  102:港股  103:美股
```

返回：已解除单层 `Value` 包装的证券列表（`list_type=0` 仅代码；`=1` 含 `{"Code","Name"}`）。

```python
stocks = tqs.get_stock_list(market="23", list_type=0)
print(stocks[:2])
```

#### 2.2 `get_sector_list` — 全部 A 股板块

```python
tqs.get_sector_list(list_type: int = 0) -> list | dict
```

获取 A 股全部板块代码列表（相当于 `get_stock_list('10')`）。

```python
sectors = tqs.get_sector_list(list_type=0)
print(sectors[:3])
```

#### 2.3 `get_sector_list_ds` — 扩展市场板块

```python
tqs.get_sector_list_ds(list_type: int = 0) -> list | dict
```

获取扩展市场板块代码列表（相当于 `get_stock_list('110')`），板块代码以 `.UZ` 结尾（对应市场号 70，含港股、美股、新加坡等市场板块）。

```python
sectors = tqs.get_sector_list_ds(list_type=0)
print(sectors[:3])
```

#### 2.4 `get_stock_list_in_sector` — 板块成分股

```python
tqs.get_stock_list_in_sector(block_code: str = "", block_type: int = 0, list_type: int = 0) -> list | dict
```

| 参数           | 必填 | 类型    | 说明                                                                                                |
| ------------ | -: | ----- | ------------------------------------------------------------------------------------------------- |
| `block_code` |  是 | `str` | 板块代码或板块名称，如 `'BK0707'` 或 `'通达信88'`                                                                |
| `block_type` |  否 | `int` | `0`：板块指数代码或板块名称（默认）；`1`：自定义板块简称（加 `BKCODE.` 前缀）；特殊板块：`ZXG`=自选股，`TJG`=临时条件股；`2`：期货板块代码（加 `QH.` 前缀） |
| `list_type`  |  否 | `int` | `0`=只返回代码                                                                                         |

返回：该板块证券列表，已解除 `Value` 包装（`list_type=0` 仅代码；`=1` 含 `{"Code","Name"}`）。

> 官方注意：入参板块只能是自定义板块或 15 个板块指数，**不支持**"全部A股""沪深A股"等系统板块。

```python
members = tqs.get_stock_list_in_sector("880201.SH", 0, 0)
print(members[:3])
```

#### 2.5 `get_trading_dates` — 交易日列表

```python
tqs.get_trading_dates(market: str, start_time: str, end_time: str, count: int = -1) -> list | dict
```

| 参数           | 必填 | 类型    | 说明                                                                      |
| ------------ | -: | ----- | ----------------------------------------------------------------------- |
| `market`     |  是 | `str` | 市场代码（暂固定为 `SH`）                                                         |
| `start_time` |  是 | `str` | 起始日期                                                                    |
| `end_time`   |  是 | `str` | 结束日期                                                                    |
| `count`      |  否 | `int` | 返回最近的 count 个交易日；`-1` 返回全部；`count>0` 时限制返回从结束日期往前最近的 count 个在限定时间段中的交易日 |

> 注意：需要先在客户端下载上证指数（999999）的盘后数据，目前仅支持 A 股。

返回：已解除 DLL `Date` 包装的日期字符串列表（通常 `YYYYMMDD`）。

```python
dates = tqs.get_trading_dates("SH", "2025-01-01", "2025-01-10", -1)
print(dates)
```

#### 2.6 `get_zzgz_stocklist` — 指数成分股

```python
tqs.get_zzgz_stocklist(index_code: str = "", list_type: int = 0) -> list | dict
```

通过 `GetSpecZSGPInStr` 获取指定指数的成分股列表。

| 参数 | 必填 | 类型 | 说明 |
| --- | ---: | --- | --- |
| `index_code` | 是 | `str` | 指数代码，如 `000300.SH` |
| `list_type` | 否 | `int` | 成分股返回类型，默认 `0`；具体枚举以 DLL 数据服务定义为准 |

返回 `list` 或 `dict`。实际验证 `index_code="000300.SH"`、`list_type=0` 返回沪深 300 成分股代码列表，共 300 个元素。

```python
stocks = tqs.get_zzgz_stocklist(
    index_code="000300.SH",
    list_type=0,
)
print(stocks[:10])
```

返回样例：

```python
["000001.SZ", "000002.SZ", "000063.SZ", "000100.SZ"]
```
### 三、专业数据接口

> 所有专业数据字段由通达信专业数据协议定义。生成代码时应由用户给出明确 `field_list`；批量取数建议每次至少请求 5 个实际需要的字段，避免无字段限制地大范围请求。

#### 公共结构

除 `get_financial_data` 外，股票/板块类接口统一返回「外层代码 → 字段 → `{"Date", "Value":[...]}`」结构；市场类接口无外层代码层。详见 §9 F.7。

#### 3.1 `get_financial_data` — 财务数据（时间序列）

```python
tqs.get_financial_data(stock_list: list[str] = [], field_list: list[str] = [],
                        start_time: str = "", end_time: str = "",
                        report_type: str = "report_time") -> dict
```

| 参数            | 必填 | 类型          | 说明                                                   |
| ------------- | -: | ----------- | ---------------------------------------------------- |
| `stock_list`  |  是 | `list[str]` | 股票列表                                                 |
| `field_list`  |  是 | `list[str]` | 财务字段（FN1~FN584），如 `FN001`–`FN005`                    |
| `start_time`  |  否 | `str`       | 开始时间                                                 |
| `end_time`    |  否 | `str`       | 结束时间                                                 |
| `report_type` |  否 | `str`       | `'announce_time'`（按公告时间）或 `'tag_time'`（按标签时间），默认按报告期 |

> 需要客户端下载专业财务数据包。

返回：当 `announce_time`/`tag_time`/各 `FN*` 均为等长列表且已装 pandas 时，按股票代码返回 `pandas.DataFrame`（列含 `announce_time`、`tag_time`、各 `FN*`）；否则保留 DLL 原始字典。

返回字段：`announce_time`（公告日期）、`tag_time`（报告期）+ 所请求的 `FN*` 字段；完整字段（FN1~FN584）见 §9 F.7。

```python
fin = tqs.get_financial_data(["600000.SH"],
    ["FN001","FN002","FN003","FN004","FN005"],
    "2024-01-01", "2025-01-01")
print(fin["600000.SH"])
```

#### 3.2 `get_financial_data_by_date` — 按报告期取财务数据

```python
tqs.get_financial_data_by_date(stock_list: list[str] = [], field_list: list[str] = [],
                               year: int = 0, mmdd: int = 0) -> dict
```

| 参数           | 必填 | 类型          | 说明                                              |
| ------------ | -: | ----------- | ----------------------------------------------- |
| `stock_list` |  是 | `list[str]` | 股票列表                                            |
| `field_list` |  是 | `list[str]` | 财务字段                                            |
| `year`       |  否 | `int`       | 年份（如 `2025`）                                    |
| `mmdd`       |  否 | `int`       | 报告期月日（如 `331`=一季报，`630`=中报，`930`=三季报，`1231`=年报） |

> 官方 year/mmdd 语义：
>
> - `year=0` 且 `mmdd=0`：返回最新的财报
> - `year=0` 且 `mmdd<300`：最近一期向前推 mmdd 期；`mmdd` 为 331/630/930/1231 时表示最近一期的对应季报
> - `mmdd=0` 且 `year>0`：最近一期向前推 year 年的同期数据
> - 季报分界点为 0331、0630、0930、1231
> - 需要先在客户端中下载财务数据包

返回：DLL 顶层 `Value` 的「股票代码 → 字段值」原始字典，不转 DataFrame。

```python
r = tqs.get_financial_data_by_date(["600000.SH"],
    ["FN001","FN002","FN003","FN004","FN005"], 2025, 331)
print(r)
```

#### 3.3 `get_gpjy_value` — 股票交易专业数据

```python
tqs.get_gpjy_value(stock_list: list[str] = [], field_list: list[str] = [],
                   start_time: str = "", end_time: str = "") -> dict
```

| 参数           | 必填 | 类型          | 说明                         |
| ------------ | -: | ----------- | -------------------------- |
| `stock_list` |  是 | `list[str]` | 股票列表                       |
| `field_list` |  是 | `list[str]` | 字段（GP1~GP46），如 `GP1`–`GP5` |
| `start_time` |  否 | `str`       | 开始时间                       |
| `end_time`   |  否 | `str`       | 结束时间                       |

返回：`{"600000.SH": {"Date":[...], "GP1":[...], ...}}`。

完整字段（GP1~GP46）与官方数据可用性备注见 §9 F.7。

```python
r = tqs.get_gpjy_value(["600000.SH"],
    ["GP1","GP2","GP3","GP4","GP5"], "2025-01-01", "2025-01-31")
print(r)
```

#### 3.4 `get_gpjy_value_by_date` — 按日期取股票交易专业数据

```python
tqs.get_gpjy_value_by_date(stock_list: list[str] = [], field_list: list[str] = [],
                            year: int = 0, mmdd: int = 0) -> dict
```

同 3.3，但用 `year`+`mmdd` 定位报告期；返回结构一致。

```python
r = tqs.get_gpjy_value_by_date(["600000.SH"],
    ["GP1","GP2","GP3","GP4","GP5"], 2025, 102)
print(r)
```

#### 3.5 `get_bkjy_value` — 板块交易专业数据

```python
tqs.get_bkjy_value(stock_list: list[str] = [], field_list: list[str] = [],
                   start_time: str = "", end_time: str = "") -> dict
```

| 参数           | 必填 | 类型          | 说明                             |
| ------------ | -: | ----------- | ------------------------------ |
| `stock_list` |  是 | `list[str]` | 板块代码列表，如 `['880201.SH']`       |
| `field_list` |  是 | `list[str]` | 板块交易字段（BK5~BK19），如 `BK5`–`BK8` |
| `start_time` |  否 | `str`       | 开始时间                           |
| `end_time`   |  否 | `str`       | 结束时间                           |

返回：`{"880201.SH": {"Date":[...], "BK1":[...], ...}}`。

完整字段（BK5~BK19）与官方数据可用性备注见 §9 F.7。

```python
r = tqs.get_bkjy_value(["880201.SH"],
    ["BK1","BK2","BK3","BK4","BK5"], "2025-01-01", "2025-01-31")
print(r)
```

#### 3.6 `get_bkjy_value_by_date` — 按日期取板块交易专业数据

```python
tqs.get_bkjy_value_by_date(stock_list: list[str] = [], field_list: list[str] = [],
                            year: int = 0, mmdd: int = 0) -> dict
```

同 3.5，用 `year`+`mmdd` 定位；返回结构一致。

#### 3.7 `get_scjy_value` — 市场交易专业数据

```python
tqs.get_scjy_value(field_list: list[str] = [], start_time: str = "", end_time: str = "") -> dict
```

| 参数           | 必填 | 类型          | 说明                              |
| ------------ | -: | ----------- | ------------------------------- |
| `field_list` |  是 | `list[str]` | 市场交易字段（SC01~SC42），如 `SC1`–`SC5` |
| `start_time` |  否 | `str`       | 开始时间                            |
| `end_time`   |  否 | `str`       | 结束时间                            |

无 `stock_list` 参数；模块内部固定使用市场代码 `999999.SH`。
  
返回：`{"Date":[...], "SC1":[...], ...}`（无外层代码层）。

完整字段（SC01~SC42）与官方数据可用性备注见 §9 F.7。

```python
r = tqs.get_scjy_value(["SC1","SC2","SC3","SC4","SC5"], "2025-01-01", "2025-01-02")
print(r)
```

#### 3.8 `get_scjy_value_by_date` — 按日期取市场交易专业数据

```python
tqs.get_scjy_value_by_date(field_list: list[str] = [], year: int = 0, mmdd: int = 0) -> dict
```

同 3.7，用 `year`+`mmdd` 定位；返回结构一致。

#### 3.9 `get_gp_one_data` — 股票单项专业数据

```python
tqs.get_gp_one_data(stock_list: list[str] = [], field_list: list[str] = []) -> dict
```

| 参数           | 必填 | 类型          | 说明                                          |
| ------------ | -: | ----------- | ------------------------------------------- |
| `stock_list` |  是 | `list[str]` | 股票列表                                        |
| `field_list` |  是 | `list[str]` | 单项字段列表（GO1~GO47，不按时间序列，返回当前值），如 `GO1`–`GO5` |

返回：`{"600000.SH": {"GO_FIELD_A":[...], ...}}`。

完整字段（GO1~GO47）与官方数据可用性备注见 §9 F.7。

```python
r = tqs.get_gp_one_data(["600000.SH"],
    ["GP_FIELD_A","GP_FIELD_B","GP_FIELD_C","GP_FIELD_D","GP_FIELD_E"])
print(r)
```

### 四、关系、可转债、ETF、涨跌停与股本接口

#### 4.1 `get_relation` — 股票所属板块

```python
tqs.get_relation(stock_code: str = "") -> list | dict
```

获取指定股票所属的全部板块信息。

| 参数           | 必填 | 类型    | 说明                   |
| ------------ | -: | ----- | -------------------- |
| `stock_code` |  是 | `str` | 股票代码，如 `'688318.SH'` |

返回：板块关系列表，字段（`BlockCode`/`BlockName`/`BlockType`/`GPNume`）见 §9 F.6。

```python
print(tqs.get_relation("688318.SH"))
```

#### 4.2 `get_kzz_info` — 可转债信息

```python
tqs.get_kzz_info(stock_code: str = "", field_list: list[str] = []) -> dict
```

| 参数           | 必填 | 说明                  |
| ------------ | -: | ------------------- |
| `stock_code` |  是 | 可转债代码，如 `123064.SZ` |
| `field_list` |  否 | 空列表保留全部字段           |

DLL 若返回列表，模块自动取首个对象并解除 `Value` 包装、做字段过滤。字段键名可能用 `KZZCode`/`HSCode`/`ZGPrice` 等同义键（见 §9 F.8），不确定时先不带 `field_list` 调一次。

```python
print(tqs.get_kzz_info("123064.SZ", ["Code", "Name", "Price"]))
```

#### 4.3 `get_trackzs_etf_info` — 跟踪指数 ETF

```python
tqs.get_trackzs_etf_info(zs_code: str = "") -> list | dict
```

| 参数        | 必填 | 类型    | 说明                                                  |
| --------- | -: | ----- | --------------------------------------------------- |
| `zs_code` |  是 | `str` | 指数代码，如 `'000300.CSI'`（沪深300）、`'950162.CSI'`（科创芯片设计） |

返回：ETF 列表/对象，已解除 `Value` 包装（字段见 §9 F.9）。

```python
print(tqs.get_trackzs_etf_info("950162.CSI"))
```

#### 4.4 `get_zdt_data` — 涨跌停数据

```python
tqs.get_zdt_data(stock_list: list[str] = []) -> list | dict
```

| 参数           | 必填 | 类型          | 说明                                        |
| ------------ | -: | ----------- | ----------------------------------------- |
| `stock_list` |  是 | `list[str]` | 股票代码列表；模块自动转换为 2014 协议的 `code/setcode` 对象 |

> 调用前会刷新每只证券所属市场数据。

返回：`Dict`，键为输入股票代码；无可用数据的股票不会出现在结果中。字段随协议与 DLL 版本变化（见 §9 F.10）。

```python
print(tqs.get_zdt_data(["600000.SH", "000001.SZ"]))
```

#### 4.5 `get_gb_info` — 按指定日期取股本数据

```python
tqs.get_gb_info(stock_code: str = "", date_list: list = [], count: int = 1) -> list | dict
```

| 参数           | 必填 | 类型          | 说明                                                       |
| ------------ | -: | ----------- | -------------------------------------------------------- |
| `stock_code` |  是 | `str`       | 股票代码                                                     |
| `date_list`  |  是 | `list[str]` | 日期列表，格式 `YYYYMMDD`；须从小到大排序；`len(date_list)` 必须 ≥ `count` |
| `count`      |  是 | `int`       | 要获取的日期数量，必须 ≥ 1                                          |

返回：2015 协议指定日期股本数据（`Date`/`Zgb`/`Ltgb`）。

```python
print(tqs.get_gb_info("600000.SH", ["20250102", "20250103"], 2))
```

#### 4.6 `get_gb_info_by_date` — 按区间取股本数据

```python
tqs.get_gb_info_by_date(stock_code: str = "", start_date: str = "", end_date: str = "") -> list | dict
```

| 参数           | 必填 | 类型    | 说明   |
| ------------ | -: | ----- | ---- |
| `stock_code` |  是 | `str` | 股票代码 |
| `start_date` |  是 | `str` | 开始日期 |
| `end_date`   |  是 | `str` | 结束日期 |

返回：2015 协议区间股本数据，字段（`Date`/`Zgb`/`Ltgb`）见 §9 F.11。

```python
print(tqs.get_gb_info_by_date("600000.SH", "2025-01-01", "2025-01-31"))
```

#### 4.7 `get_exday_data` — 日线统计数据

```python
tqs.get_exday_data(stock_code: str = "", count: int = 1) -> list
```

通过 `GetEXDAYL2InStr` 获取指定股票最近若干条扩展日线 L2 统计数据，对齐 `tqcenter.get_exday_data`。

| 参数 | 必填 | 类型 | 说明 |
| --- | ---: | --- | --- |
| `stock_code` | 是 | `str` | 单只证券代码，如 `000001.SZ` |
| `count` | 否 | `int` | 返回数据条数，必须大于 `0`，默认 `1` |

返回 `list[dict]`，列表中每个字典对应一个交易日。常见字段包括 `Date`、`Amo`、`Vol`、`VolNum`、`CJBS`、`BOrder`、`SOrder`、`BCancel`、`SCancel`、`BuyAvp`、`SellAvp`、`TotalBOrder`、`TotalSOrder`。

```python
exday_data = tqs.get_exday_data(
    stock_code="000001.SZ",
    count=1,
)
print(exday_data)
```

返回样例：

```python
[
    {
        "Date": "20260825",
        "CJBS": "80966.00",
        "BOrder": "0.00",
        "SOrder": "0.00",
        "BCancel": "0.00",
        "SCancel": "0.00",
        "Amo": [["219778720.00", "266421504.00"]],
        "Vol": [["189684.05", "229996.62"]],
        "VolNum": [["605.63", "487.42"]]
    }
]
```
### 五、分笔数据接口

#### 5.1 `get_tick_data` — 分笔数据

```python
tqs.get_tick_data(stock_code: str, date: str, startxh: int = 0, wantnum: int = 0) -> DataFrame | dict
```

根据单只证券代码、日期和起始序号获取分笔成交数据。

| 参数 | 必填 | 类型 | 说明 |
| --- | ---: | --- | --- |
| `stock_code` | 是 | `str` | 单只证券代码，如 `000001.SZ` |
| `date` | 是 | `str` | 查询日期，支持 `YYYYMMDD`、`YYYY-MM-DD` 或 `YYYY-MM-DD HH:MM:SS` |
| `startxh` | 否 | `int` | 起始分笔序号，从 `0` 开始 |
| `wantnum` | 否 | `int` | 请求条数，建议 `1..2000` |

返回：分笔成交明细。已安装 pandas 时为 DataFrame，列 `Time`/`Price`/`Volume`/`BSFlag`，行索引为默认整数索引，同一行表示同一笔成交，各列均为字符串；无 pandas 或数组长度不一致时原样返回底层 dict，此时这四个字段为长度一致的字符串数组，并多一个 `TotalNum` 表示本次实际返回的记录数。

| 返回字段 | 类型 | 说明 |
| --- | --- | --- |
| `Time` | `str` | 成交时间，格式为 `HHMMSS`，如 `093000` |
| `Price` | `str` | 成交价格 |
| `Volume` | `str` | 成交量 |
| `BSFlag` | `str` | 买卖方向标识，实际枚举含义以数据服务定义为准 |

**分页：** 当需要获取更多数据时，将下一次的 `startxh` 增加本次返回的记录数（DataFrame 用 `len(df)`，dict 用 `TotalNum`）。若本次记录数小于请求的 `wantnum`，表示已到达最后一页。每次最多建议请求 2000 条。

```python
ticks = tqs.get_tick_data(
    stock_code="000001.SZ",
    date="2026-08-21",
    startxh=0,
    wantnum=100,
)
print(ticks)
```

实际返回样例（`000001.SZ`，2026-08-21）：

```text
     Time  Price Volume BSFlag
0  092500  11.36   3049      2
1  093000  11.36   1737      1
2  093003  11.37   1547      0
```

翻页获取全部分笔数据（需已安装 pandas）：

```python
import pandas as pd

def get_all_ticks(stock_code: str, date: str, page_size: int = 2000):
    frames = []
    startxh = 0

    while True:
        page = tqs.get_tick_data(
            stock_code=stock_code,
            date=date,
            startxh=startxh,
            wantnum=page_size,
        )
        if page is None or len(page) == 0:
            break

        frames.append(page)

        startxh += len(page)
        if len(page) < page_size:
            break

    if not frames:
        return None
    return pd.concat(frames, ignore_index=True)
```

### 六、分时数据接口

#### 6.1 `get_minute_data` — 指定日期分时数据

```python
tqs.get_minute_data(
    stock_code: str = "",
    date: str = "",
    field_list: Iterable[str] | None = None,
) -> dict
```

通过 `GetMinuteDataInStr` 获取单只证券指定日期的分时数据。

| 参数 | 必填 | 类型 | 说明 |
| --- | ---: | --- | --- |
| `stock_code` | 是 | `str` | 单只证券代码，如 `000001.SZ` |
| `date` | 是 | `str` | 查询日期，支持 `YYYYMMDD`、`YYYY-MM-DD` 或完整时间 |
| `field_list` | 否 | `Iterable[str] | None` | 字段筛选；传 `None` 或空列表返回全部字段 |

返回 `dict`，当前实际返回的并行数组字段为 `Average`、`Price`、`Time`、`TotalNum`、`Volume`。`Time`、`Price`、`Average`、`Volume` 同一索引位置对应同一条分时记录，`TotalNum` 表示返回记录数。

```python
minute_data = tqs.get_minute_data(
    stock_code="000001.SZ",
    date="2026-08-21",
    field_list=["Time", "Price"],
)
print(minute_data)
```

返回样例：

```python
{
    "Time": ["093000", "093100", "093200"],
    "Price": ["11.36", "11.36", "11.37"]
}
```
### 七、行情订阅接口

#### 7.1 `subscribe` — 订阅实时行情

```python
tqs.subscribe(stock_list: list[str] = [], callback: Callable[[str], object] | None = None) -> None
```

订阅指定证券的实时行情。行情数据通过异步回调返回，`subscribe` 本身返回 `None`。

| 参数 | 必填 | 类型 | 说明 |
| --- | ---: | --- | --- |
| `stock_list` | 是 | `list[str]` | 待订阅证券代码列表，如 `["000001.SZ", "600000.SH"]` |
| `callback` | 是 | `Callable[[str], object]` | 回调函数，只接收一个原始行情 JSON 字符串参数 `data` |

调用后主线程必须持续运行；Python 进程退出后，行情订阅和回调也会停止。调用 `tqs.get_subscribe_hq_stock_list()` 可查询当前进程内记录的订阅代码。

本次盘中实际回调的 JSON 格式如下：

```json
{
  "Error": "",
  "ErrorId": 0,
  "ResultSets": [
    {
      "ColDes": ["code", "decimal", "price", "pre_close", "open", "high", "low", "refresh_time", "volume", "..."],
      "Content": [
        ["000001.SZ", 2, 11.350, 11.400, 11.360, 11.430, 11.340, 95024, 155267, "..."],
        ["600000.SH", 2, 9.100, 9.110, 9.090, 9.150, 9.080, 95025, 100544, "..."]
      ]
    }
  ]
}
```

`ResultSets[0]["ColDes"]` 为字段名数组，`ResultSets[0]["Content"]` 为行情行数组；每一行与字段名数组按相同下标对应。一条回调可以包含多只已订阅证券，也可以只包含其中一只。

| 常见字段 | 说明 |
| --- | --- |
| `code` | 证券代码 |
| `decimal` | 价格小数位数 |
| `price` | 最新价 |
| `pre_close` | 昨收价 |
| `open` | 开盘价 |
| `high` | 最高价 |
| `low` | 最低价 |
| `refresh_time` | 行情刷新时间，格式通常为 `HHMMSS` 数字 |
| `volume` | 成交量 |
| `limit_up` / `limit_down` | 涨停价 / 跌停价 |
| `ask1` 至 `ask5` | 五档卖价 |
| `ask_vol1` 至 `ask_vol5` | 五档卖量 |
| `bid1` 至 `bid5` | 五档买价 |
| `bid_vol1` 至 `bid_vol5` | 五档买量 |

```python
import json
import time

from tdxaidata import tqs

stock_list = ["000001.SZ", "600000.SH", "600519.SH"]

def on_quote(data: str) -> None:
    quote_data = json.loads(data)
    if quote_data.get("ErrorId") not in (0, "0"):
        print("行情服务返回错误:", quote_data)
        return

    for result_set in quote_data.get("ResultSets", []):
        field_names = result_set.get("ColDes", [])
        for row in result_set.get("Content", []):
            quote = dict(zip(field_names, row))
            print(
                "代码=%s 最新价=%s 成交量=%s 刷新时间=%s"
                % (
                    quote.get("code"),
                    quote.get("price"),
                    quote.get("volume"),
                    quote.get("refresh_time"),
                )
            )

subscribed = False
try:
    tqs.subscribe(stock_list=stock_list, callback=on_quote)
    subscribed = True
    print("订阅成功:", tqs.get_subscribe_hq_stock_list())

    while True:
        time.sleep(1)
except KeyboardInterrupt:
    print("收到退出信号")
finally:
    if subscribed:
        tqs.unsubscribe()
```

实际盘中验证：2026-08-21 使用 `000001.SZ`、`600000.SH` 和 `600519.SH` 订阅 30 秒，共收到 96 条有效回调。回调中已确认包含 `Error`、`ErrorId`、`ResultSets`、`ColDes` 和 `Content` 字段。
#### 7.2 `unsubscribe` — 取消订阅

```python
tqs.unsubscribe() -> None
```

无入参；取消**全部**订阅：底层执行一次全局 `GetStockInfo_UnSub()`，同时清空 `tqs` 进程内记录的订阅列表。若需保留其他订阅代码，应用层应重新订阅。

```python
tqs.unsubscribe()
```

#### 7.3 `get_subscribe_hq_stock_list` — 当前订阅列表

```python
tqs.get_subscribe_hq_stock_list() -> list[str]
```

无入参；返回当前 `tqs` 进程内记录的带市场后缀订阅股票代码列表。

```python
print(tqs.get_subscribe_hq_stock_list())
```

---

### 八、集合竞价、批量与排序接口

> 批量接口统一返回「每代码一条记录」`{代码: {字段...}}`；传 `return_df=True`（需已装 pandas）时返回以代码为索引的 `DataFrame`（行=代码，列=字段），无 pandas 时退回字典。
> `stock_list` 必须带市场后缀（如 `'600000.SH'`、`'123071.SZ'`）；`field_list` 为空 `[]` 返回全部字段，非空仅返回指定字段（未知字段静默忽略）。

#### 8.1 `get_call_auction` — 集合竞价数据（单只）

```python
tqs.get_call_auction(stock_code: str, type: int = 1, date: str = "") -> DataFrame | dict
```

| 参数         | 必填 | 类型  | 说明                                     |
| ------------ | ---: | ----- | ---------------------------------------- |
| `stock_code` |   是 | `str` | 单个证券代码，如 `'600000.SH'`           |
| `type`       |   否 | `int` | 竞价范围：0 早盘和尾盘、1 仅早盘（默认） |
| `date`       |   否 | `str` | 查询日期，支持 `YYYYMMDD`、`YYYY-MM-DD`；不传取当日 |

返回：集合竞价时序（虚拟匹配价、匹配量等）。pandas 下为 DataFrame，列 `LeaveQty`/`Price`/`Time`/`Volume`；无 pandas 为 dict。

```python
auction = tqs.get_call_auction("600000.SH")
print(auction)
```


#### 8.2 `get_call_auction_batch` — 批量获取集合竞价数据

```python
tqs.get_call_auction_batch(stock_list: list[str] = [], field_list: list[str] = [], type: int = 1, date: str = "", return_df: bool = False) -> dict | DataFrame
```

| 参数         | 必填 | 类型        | 说明                                     |
| ------------ | ---: | ----------- | ---------------------------------------- |
| `stock_list` |   是 | `list[str]` | 证券代码列表                             |
| `field_list` |   否 | `list[str]` | 为空返回全部字段                         |
| `type`       |   否 | `int`       | 竞价范围：0 早盘和尾盘、1 仅早盘（默认） |
| `date`       |   否 | `str`       | 查询日期，支持 `YYYYMMDD`、`YYYY-MM-DD`；不传取当日 |
| `return_df`  |   否 | `bool`      | `True` 且已装 pandas → DataFrame         |

返回：`{'600000.SH': {'LeaveQty': ..., 'Price': ..., 'Time': ..., 'Volume': ...}, ...}`（字段同 §8.1）；`return_df=True` 时返回上述结构转成的 DataFrame。

```python
data = tqs.get_call_auction_batch(["600000.SH", "600519.SH"], return_df=True)
print(data)
```


#### 8.3 `get_market_snapshot_batch` — 批量获取实时行情快照

```python
tqs.get_market_snapshot_batch(stock_list: list[str] = [], field_list: list[str] = [], return_df: bool = False) -> dict | DataFrame
```

| 参数           | 必填 | 类型          | 说明                    |
| ------------ | -: | ----------- | --------------------- |
| `stock_list` |  是 | `list[str]` | 证券代码列表     |
| `field_list` |  否 | `list[str]` | 为空返回全部字段              |
| `return_df`  |  否 | `bool`      | `True` 且已装 pandas → DataFrame |

返回：`{代码: {字段...}}`，实测 26 个字段（含 `Now`/`LastClose`/`Volume`/`Amount`/`Buyp`/`Sellp`/`Inside`/`Outside` 等，完整字段见 §9 F.3）。

```python
snap = tqs.get_market_snapshot_batch(["600000.SH", "600519.SH"], ["Now", "LastClose", "Volume"], return_df=True)
print(snap)
```

#### 8.4 `get_stock_info_batch` — 批量获取基本财务数据

```python
tqs.get_stock_info_batch(stock_list: list[str] = [], field_list: list[str] = [], return_df: bool = False) -> dict | DataFrame
```

| 参数           | 必填 | 类型          | 说明                    |
| ------------ | -: | ----------- | --------------------- |
| `stock_list` |  是 | `list[str]` | 证券代码列表     |
| `field_list` |  否 | `list[str]` | 为空返回全部字段              |
| `return_df`  |  否 | `bool`      | `True` 且已装 pandas → DataFrame |

返回：`{代码: {字段...}}`，实测 62 个字段（基础信息、股本、财务、每股指标、行业地域等，完整字段见 §9 F.4）。

```python
info = tqs.get_stock_info_batch(["600000.SH", "600519.SH"], ["Name", "ActiveCapital"], return_df=True)
print(info)
```

#### 8.5 `get_more_info_batch` — 批量获取扩展证券信息

```python
tqs.get_more_info_batch(stock_list: list[str] = [], field_list: list[str] = [], return_df: bool = False) -> dict | DataFrame
```

| 参数           | 必填 | 类型          | 说明                    |
| ------------ | -: | ----------- | --------------------- |
| `stock_list` |  是 | `list[str]` | 证券代码列表      |
| `field_list` |  否 | `list[str]` | 为空返回全部字段              |
| `return_df`  |  否 | `bool`      | `True` 且已装 pandas → DataFrame |

返回：`{代码: {字段...}}`，实测 92 个字段（涨幅系列、主力资金、L2、封单、估值、期货相关等，完整字段见 §9 F.5）。

```python
more = tqs.get_more_info_batch(["600000.SH"], ["Zsz", "DynaPE", "VolInStock"], return_df=True)
print(more)
```

#### 8.6 `get_kzz_info_batch` — 批量获取可转债基础信息

```python
tqs.get_kzz_info_batch(stock_list: list[str] = [], field_list: list[str] = [], return_df: bool = False) -> dict | DataFrame
```

| 参数           | 必填 | 类型          | 说明                    |
| ------------ | -: | ----------- | --------------------- |
| `stock_list` |  是 | `list[str]` | 可转债代码列表    |
| `field_list` |  否 | `list[str]` | 为空返回全部字段              |
| `return_df`  |  否 | `bool`      | `True` 且已装 pandas → DataFrame |

返回：`{代码: {字段...}}`，实测 27 个字段（含 `KZZName`/`KZZNow`/`RealValue`/`CurRate`/`ForceRedeem`/`ExpireYield` 等，完整字段见 §9 F.8）。

```python
kzz = tqs.get_kzz_info_batch(["123071.SZ", "123072.SZ"], ["KZZName", "RealValue"], return_df=True)
print(kzz)
```

#### 8.7 `get_domain_sorthq` — 集合排名行情

```python
tqs.get_domain_sorthq(domain: int = 6, colname: str = 'ZAF', sorttype: int = 1, wantcol: list[str] = [], startxh: int = 0, wantnum: int = 100, data_source: int = 0) -> DataFrame | dict
```

| 参数            | 必填 | 类型          | 说明                                                                     |
| ------------- | -: | ----------- | ---------------------------------------------------------------------- |
| `domain`      |  是 | `int`       | 排序域，非负整数。含沪深京市场域、板块域、分类域、扩展行情板块域与指数成分域，完整对照表见 §9 F.15            |
| `colname`     |  是 | `str`       | 排序栏目，取值为英文名字符串，如 `'ZAF'`/`'Now'`/`'Volume'`/`'Amount'`/`'fHSL'`/`'Zsz'`/`'Ltsz'`/`'Zjl'`，完整 36 项见 §9 F.15；**不能用宏定义名（如 `ZQDM`）或栏目号（如 `14`）** |
| `sorttype`    |  否 | `int`       | `0` 不排序（按域内自然顺序）、`1` 降序、`2` 升序，默认 `1`                                   |
| `wantcol`     |  否 | `list[str]` | 附带返回的栏目，取值同 `colname`（英文字符串，完整 36 项见 §9 F.15），**可为空**（不传或传空列表时不返回附加栏目）；有效栏目超过 5 个时只取前 5 个 |
| `startxh`     |  否 | `int`       | 起始序号，从 `0` 开始，上限 `32767`                                                |
| `wantnum`     |  否 | `int`       | 返回条数，取值 `1~200`，默认 `100`                                                |
| `data_source` |  否 | `int`       | `0` 沪深京（默认）、`1` 扩展行情                                                   |

返回：以排名为顺序的 DataFrame，固定列 `Code`/`Name`/`Now`/`Close`/`ZAF`/`Volume`；`colname` 不属于固定栏目时追加 `CurCol`；`wantcol` 中的栏目按服务端规范列名追加。无 pandas 时返回同序的 `{列名: [值...]}` 字典。

```python
# 沪深京A股按涨幅降序取前 20，附加返回成交额与换手率
rank = tqs.get_domain_sorthq(domain=6, colname="ZAF", wantnum=20, wantcol=["Amount", "fHSL"])
print(rank)

# 沪深300成分股按涨幅降序取前 10
hs300 = tqs.get_domain_sorthq(domain=11002, colname="ZAF", wantnum=10, wantcol=["Zsz"])

# 中证1000（399852）成分股按涨幅降序
zz1000 = tqs.get_domain_sorthq(domain=30852, colname="ZAF", wantnum=10, wantcol=["fHSL"])
```

> `domain` 全部取值与 `colname`/`wantcol` 的 36 项合法入参均见 §9 F.15；二者只能按表中「入参值」列的英文字符串原样书写，`colname` 填其他内容会直接报错并返回空，`wantcol` 填其他内容会被忽略。

#### 8.8 `get_global_data` — 市场整体情况

```python
tqs.get_global_data(type: int = 0) -> dict
```

| 参数   | 必填 | 类型  | 说明                                                                       |
| ------ | ---- | ----- | -------------------------------------------------------------------------- |
| `type` | 否   | `int` | 市场范围：`0` 沪深京市场（默认）、`1` 全球市场（港股与境外指数、商品、股指期货） |

返回 `{字段: 值}`，字段随 `type` 不同，完整字段见 §9 F.16。

- `type=0` —— 实测 87 个字段，值为 `float` / `int`，另含 4 个数组字段（`tup` / `tdown` / `tyx` / `tkb`）
- `type=1` —— 实测 53 个字段，取值**均为字符串**

`type` 非 `0` / `1` 或非整数时打印 `[错误]` 并返回 `{}`。

```python
data = tqs.get_global_data(type=0)
print(data)
```

```python
global_ds = tqs.get_global_data(type=1)
print(global_ds["HSI_ZS_Now"], global_ds["HK_UpNum"])
```

#### 8.9 `formula_process_mul_zb` / `formula_process_mul_xg` / `formula_process_mul_exp` — 批量公式计算

三个接口分别执行服务器已有的**指标**公式、**条件选股**公式、**专家系统**公式，入参与返回结构一致，仅 `xg` 没有 `xsflag` 参数。

```python
tqs.formula_process_mul_zb(formula_name: str = "", formula_arg: str = "", return_count: int = 1, return_date: bool = False, xsflag: int = -1, stock_list: list[str] = [], stock_period: str = "1d", start_time: str = "", end_time: str = "", count: int = 0, dividend_type: int = 0, domain: int = 0, data_source: int = 0) -> dict
```

```python
tqs.formula_process_mul_xg(formula_name: str = "", formula_arg: str = "", return_count: int = 1, return_date: bool = False, stock_list: list[str] = [], stock_period: str = "1d", start_time: str = "", end_time: str = "", count: int = 0, dividend_type: int = 0, domain: int = 0, data_source: int = 0) -> dict
```

```python
tqs.formula_process_mul_exp(formula_name: str = "", formula_arg: str = "", return_count: int = 1, return_date: bool = False, xsflag: int = -1, stock_list: list[str] = [], stock_period: str = "1d", start_time: str = "", end_time: str = "", count: int = 0, dividend_type: int = 0, domain: int = 0, data_source: int = 0) -> dict
```

| 参数            | 必填 | 类型        | 说明                                                                                                                                                       |
| --------------- | ---- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `formula_name`  | 是   | `str`       | 服务器已存在的公式英文简称，如 `MA`、`MACD`、`KDJ`                                                                                                         |
| `formula_arg`   | 否   | `str`       | 公式参数，逗号分隔，如 `"12,26,9"`；不传时沿用公式自身默认值                                                                                               |
| `return_count`  | 否   | `int`       | 每只证券最多返回的记录数，`0` 或超过实际条数时返回全部                                                                                                     |
| `return_date`   | 否   | `bool`      | `True` 时每条记录带日期；默认 `False` 返回纯值数组                                                                                                         |
| `xsflag`        | 否   | `int`       | 小数位：`-1` 用公式默认精度（默认）、`0`~`8` 指定小数位；仅 `zb`、`exp` 有该参数                                                                           |
| `stock_list`    | 否   | `list[str]` | 证券代码列表（带市场后缀）；`domain` 为 `0` 时必填                                                                                                         |
| `stock_period`  | 否   | `str`       | 周期，默认 `1d`；支持 `5m`、`15m`、`30m`、`1h`、`1d`、`1w`、`1mon`、`1m`、`10m`、`45d`、`1q`、`1y`、`5s`、`10s`、`15s`、`fst`（分时图）、`auc`（早盘竞价） |
| `start_time`    | 否   | `str`       | 起始时间                                                                                                                                                   |
| `end_time`      | 否   | `str`       | 结束时间                                                                                                                                                   |
| `count`         | 否   | `int`       | 取数条数，服务端默认 `400`、最大 `24000`；本封装默认 `0`，为 `0` 或 `-1` 时必须同时给 `start_time` 与 `end_time`；超过 `24000` 打印 `[错误]` 并返回 `{}`   |
| `dividend_type` | 否   | `int`       | 复权类型：0 不复权、1 前复权、2 后复权                                                                                                                     |
| `domain`        | 否   | `int`       | 市场分类，非 `0` 时按该分类自动取股票池（取值同 §8.7 `get_domain_sorthq` 的 `domain`），此时 `stock_list` 不参与                                           |
| `data_source`   | 否   | `int`       | 数据源：`0` 沪深京（默认）、`1` 扩展行情（取值同 §8.7 `get_domain_sorthq` 的 `data_source`）                                                               |

返回：`{证券代码: {输出线名: [...], 'Time': [...]}}`，`Time` 为该证券各条记录的时间，格式 `HHMMSS`（完整结构见 §9 F.17）。

> 公式不存在、参数越界或结果超过 64KiB 时打印 `[错误]` 并返回 `{}`；结果超限时建议缩小股票范围或分批调用。

```python
# 指标公式：MA 指标，取两只证券最近 10 条
zb = tqs.formula_process_mul_zb(formula_name="MA", formula_arg="5,10,20",
                                stock_list=["600000.SH", "000001.SZ"], count=10)
print(zb)

# 条件选股：只有窗口内至少命中过一次的证券才会出现在结果中
xg = tqs.formula_process_mul_xg(formula_name="UPN", stock_list=["600000.SH"], count=10)
print(xg)

# 专家系统：公式类型 2
exp = tqs.formula_process_mul_exp(formula_name="MA", stock_list=["600000.SH"], count=10)
print(exp)
```

#### 8.10 `formula_by_src` / `formula_by_file` — 临时源码公式计算

两个接口按**临时公式源码**批量计算，公式只在本次调用生效、不保存到客户端；`formula_by_src` 直接传源码文本，`formula_by_file` 从本地文本文件读取。

```python
tqs.formula_by_src(formula_src: str = "", stock_list: list[str] = [], stock_period: str = "1d", start_time: str = "", end_time: str = "", count: int = 0, return_count: int = 1, return_date: bool = False, xsflag: int = -1, dividend_type: int = 0, formula_type: int = 0, domain: int = 0, data_source: int = 0) -> dict
```

```python
tqs.formula_by_file(formula_file: str = "", stock_list: list[str] = [], stock_period: str = "1d", start_time: str = "", end_time: str = "", count: int = 0, return_count: int = 1, return_date: bool = False, xsflag: int = -1, dividend_type: int = 0, formula_type: int = 0, domain: int = 0, data_source: int = 0) -> dict
```

| 参数           | 必填 | 类型  | 说明                                                                                                                                                        |
| -------------- | ---- | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `formula_src`  | 是   | `str` | 公式源码文本                                                                                                                                                |
| `formula_file` | 是   | `str` | 公式源码文件路径，编码支持 UTF-8 与 GB2312                                                                                                                  |
| `formula_type` | 否   | `int` | 公式类型：0 指标（默认）、1 条件选股、2 专家系统                                                                                                            |
| 其余参数       | —    | —     | 同 §8.9：`stock_list`、`stock_period`、`start_time`、`end_time`、`count`、`return_count`、`return_date`、`xsflag`、`dividend_type`、`domain`、`data_source` |

返回：结构同 §8.9（见 §9 F.18）。

> 该接口依赖服务端临时公式源码协议，未上线的服务端会返回 `protocol not supported`，此时模块打印 `[错误]` 并返回 `{}`。

```python
src = "MA5:MA(C,5);MA10:MA(C,10);"
data = tqs.formula_by_src(formula_src=src, stock_list=["600000.SH"], count=10)
print(data)

from_file = tqs.formula_by_file(formula_file=r"D:\formula\my_ma.txt",
                                stock_list=["600000.SH"], count=10)
print(from_file)
```

> 注意：§8 的集合竞价与批量接口依赖动态库中的新协议，未上线的服务端会返回 `{"error": "unknown rpc", "error_code": 4}`，此时模块打印 `[错误]` 并返回 `None` 或 `{}`。可用 `TdxAiData.ini` 的 `[TcHqHost]` 指向已上线的服务端验证。

---

## 禁止生成的接口（tqcenter 独有，tqs 无实现）

生成 Python 文件时遇到下列能力，必须明确提示「不支持」，不得写出虚构调用：

| 类别     | 不支持接口或能力                                                                                                             |
| ------ | -------------------------------------------------------------------------------------------------------------------- |
| 初始化    | `initialize`、`close`                                                                                                 |
| 客户端交互  | `send_message`、`send_file`、`send_warn`、`send_bt_data`、`send_user_block`、`exec_to_tdx`、`print_to_tdx` |
| 公式     | `formula_zb`、`formula_xg`、`formula_exp` 等 `tqcenter` 本地公式方法（批量计算见 §8.9、§8.10）                                            |
| 交易     | `stock_account`、`query_stock_orders`、`query_stock_positions`、`query_stock_asset`、`order_stock`、`cancel_order_stock`  |
| 自选板块   | `get_user_sector`、`create_sector`、`delete_sector`、`rename_sector`、`clear_sector`                                     |
| 暂无原生映射 | `get_ipo_info`、`get_pricevol`、`get_match_stkinfo`                                                                    |
| 缓存     | `refresh_cache`、`refresh_kline`                                                                                      |

---

## 完整可执行脚本模板

```python
# -*- coding: utf-8 -*-
from tdxaidata import TqsError, tqs

def main() -> int:
    try:
        result = tqs.get_market_data(
            field_list=["Close", "Volume"],
            stock_list=["600000.SH", "000001.SZ"],
            period="1d",
            count=100,
            dividend_type="front",
        )
        print("收盘价:")
        print(result["Close"].tail())
        return 0
    except TqsError as exc:
        print(f"[接口错误] {exc}")
        return 1
    except ValueError as exc:
        print(f"[参数错误] {exc}")
        return 2
    except OSError as exc:
        print(f"[DLL 原生错误] {exc}")
        return 3

if __name__ == "__main__":
    raise SystemExit(main())
```

---

## 九、字段参考（完整字段百科）

> 除特别说明外，`tqs` 会移除顶层 `ErrorId`、`Error`、`ResultSetNum`；发生业务错误时抛出 `TqsError`。本节字段表应作为生成代码时筛选 `field_list`、解析返回值和输出结果的依据。生成 `field_list` 或解析返回值时，按本节确认字段代码与 `Value` 数组语义；不确定真实键名时，先不带 `field_list` 调用对应接口一次，按返回的真实键名再筛选。

### F.1 `get_market_data` 出参字段

返回 `Dict`，键为字段名，值为 `pd.DataFrame`（行=时间，列=股票代码）。DataFrame 的索引不是单独返回列：日线索引是 `Date`，分钟线索引由 `Date + Time` 组成；列名为输入的股票代码。

| 字段              | 类型    | 说明                                |
| --------------- | ----- | --------------------------------- |
| `Date`          | `str` | 日期                                |
| `Time`          | `str` | 时间                                |
| `Open`          | `str` | 开盘价（元）                            |
| `High`          | `str` | 最高价（元）                            |
| `Low`           | `str` | 最低价（元）                            |
| `Close`         | `str` | 收盘价（元）                            |
| `Volume`        | `str` | 成交量（股）                            |
| `Amount`        | `str` | 成交额（万元）                           |
| `ForwardFactor` | `str` | 前复权因子（仅 `dividend_type=none` 时有效） |
| `VolInStock`    | `str` | 持仓量（默认返回 N）                       |

> 注：`Date`/`Time` 由 `tqs` 用于构造 DataFrame 索引，不作为返回字典的键；`ForwardFactor` 必须用 DLL 原始值，整列 `0` 时保持全 `0`，不得自行推算。官方注：期货数据时 `Amount` 为 0，非期货数据时 `VolInStock` 为 0。

### F.2 `get_divid_factors` 出参字段

| 字段           | 类型                        | 说明                             |
| ------------ | ------------------------- | ------------------------------ |
| `Date`       | DataFrame 索引 `datetime64` | 权益日期                           |
| `Type`       | `str`                     | 类型：`1`=除权除息、`11`=扩缩股、`15`=重新调整 |
| `Bonus`      | `float`                   | 红利（每10股分红）                     |
| `AllotPrice` | `float`                   | 配股价                            |
| `ShareBonus` | `float`                   | 送股/扩缩股比例                       |
| `Allotment`  | `float`                   | 配股比例                           |

未安装 pandas 时返回 `list[dict]`，每个字典字段同上，且 `Date` 为字符串。

> 官方注：此接口只支持沪深京的分红送配数据。

### F.3 `get_market_snapshot` 出参字段

以下为 `GetRealDATsInStr` 的快照字段。传入 `field_list` 时仅保留对应键（大小写不敏感）。

> 官方提示：买二至买五、卖二至卖五等买卖盘数据，在非报价界面下可能为空。

| 字段              | 类型          | 说明                             |
| --------------- | ----------- | ------------------------------ |
| `ItemNum`       | `str`       | 快照笔数                           |
| `LastClose`     | `str`       | 前收盘价                           |
| `Open`          | `str`       | 开盘价                            |
| `Max`           | `str`       | 最高价                            |
| `Min`           | `str`       | 最低价                            |
| `Now`           | `str`       | 现价                             |
| `Volume`        | `str`       | 总手                             |
| `NowVol`        | `str`       | 现手                             |
| `Amount`        | `str`       | 总成交金额                          |
| `Inside`        | `str`       | 内盘（板块指数时为跌停家数）                 |
| `Outside`       | `str`       | 外盘（板块指数时为涨停家数）                 |
| `TickDiff`      | `str`       | 笔涨跌                            |
| `InOutFlag`     | `str`       | 内外盘标志：0=Buy, 1=Sell, 2=Unknown |
| `Jjjz`          | `str`       | 基金净值                           |
| `Buyp`          | `List[str]` | 五档买价列表                         |
| `Buyv`          | `List[str]` | 五档买盘量列表                        |
| `Sellp`         | `List[str]` | 五档卖价列表                         |
| `Sellv`         | `List[str]` | 五档卖盘量列表                        |
| `UpHome`        | `str`       | 上涨家数（对指数有效）                    |
| `DownHome`      | `str`       | 下跌家数（对指数有效）                    |
| `Before5MinNow` | `str`       | 5分钟前价格                         |
| `Average`       | `str`       | 均价                             |
| `XsFlag`        | `str`       | 小数位数                           |
| `Zangsu`        | `str`       | 涨速                             |
| `ZAFPre3`       | `str`       | 3日涨幅                           |

### F.4 `get_stock_info` 出参字段

`field_list` **不能为空**，必须明确指定字段列表。不同证券类型可能只返回其中一部分。完整返回字段（84个）：

#### 基础信息

| 字段              | 说明                                                                        |
| --------------- | ------------------------------------------------------------------------- |
| `Name`          | 证券名称                                                                      |
| `Unit`          | 交易单位                                                                      |
| `VolBase`       | 量比的基量                                                                     |
| `MinPrice`      | 最小价格变动                                                                    |
| `XsFlag`        | 价格小数位数                                                                    |
| `Fz`            | 开收市时间（4段，8元素列表）                                                           |
| `DelayMin`      | 延时分钟数                                                                     |
| `QHVolBaseRate` | 期货期权的每手乘数                                                                 |
| `HKVolBaseRate` | 港股/日股/新加坡股每手股数                                                            |
| `BelongHS300`   | 是否属于沪深300                                                                 |
| `BelongHasKQZ`  | 是否含可转债                                                                    |
| `BelongRZRQ`    | 是否是融资融券标的                                                                 |
| `BelongHSGT`    | 是否属于沪深股通                                                                  |
| `IsHKGP`        | 是否是港股                                                                     |
| `IsQH`          | 是否是期货                                                                     |
| `IsQQ`          | 是否是期权                                                                     |
| `IsSTGP`        | 是否是ST股票                                                                   |
| `IsQuitGP`      | 是否是退市整理板股票                                                                |
| `TodayDRFlag`   | 当天是否有除权除息(沪深京)                                                            |
| `HSStockKind`   | 沪深京品种类型：0=指数,1=A股主板,2=北证A股,3=创业板,4=科创板,5=B股,6=债券,7=基金,8=权证,9=其它,10=非沪深京品种 |

#### 股本数据

| 字段              | 说明       |
| --------------- | -------- |
| `ActiveCapital` | 流通股本(万股) |
| `J_zgb`         | 总股本(万股)  |
| `J_bg`          | B股(万股)   |
| `J_hg`          | H股(万股)   |

#### 财务数据（万元）

| 字段        | 说明        |
| --------- | --------- |
| `J_zzc`   | 总资产       |
| `J_ldzc`  | 流动资产      |
| `J_gdzc`  | 固定资产      |
| `J_wxzc`  | 无形资产      |
| `J_ldfz`  | 流动负债      |
| `J_cqfz`  | 少数股东权益    |
| `J_zbgjj` | 资本公积金     |
| `J_jzc`   | 股东权益/净资产  |
| `J_yysy`  | 营业收入      |
| `J_yycb`  | 营业成本      |
| `J_yszk`  | 应收账款      |
| `J_yyly`  | 营业利润      |
| `J_tzsy`  | 投资收益      |
| `J_jyxjl` | 经营现金净流量   |
| `J_zxjl`  | 总现金净流量    |
| `J_ch`    | 存货        |
| `J_lyze`  | 利润总额      |
| `J_shly`  | 税后利润      |
| `J_jly`   | 净利润       |
| `J_wfply` | 未分配利益(万元) |

#### 每股指标

| 字段               | 说明              |
| ---------------- | --------------- |
| `J_jyl`          | 净资产收益率          |
| `J_mgwfp`        | 每股未分配           |
| `J_mgsy`         | 每股收益（折算为全年）     |
| `J_mgsy2`        | 季报每股收益（财报中提供）   |
| `J_mggjj`        | 每股公积金           |
| `J_mgjzc`        | 每股净资产           |
| `J_mgjzc2`       | 季报每股净资产（财报中提供）  |
| `J_gdqyb`        | 股东权益比           |
| `J_gdrs`         | 股东人数            |
| `J_HalfYearFlag` | 报告期月份(3,6,9,12) |
| `J_start`        | 上市日期            |

#### 行业与地域

| 字段                | 说明                  |
| ----------------- | ------------------- |
| `tdx_dycode`      | 通达信地域代码             |
| `tdx_dyname`      | 通达信地域               |
| `rs_hycode_sim`   | 通达信行业代码             |
| `rs_hyname`       | 通达信行业               |
| `blockzscode`     | 所属的行业板块指数代码         |
| `underly_setcode` | 标的市场代码（如ETF跟踪的指数市场） |
| `underly_code`    | 标的代码（如ETF跟踪的指数代码）   |

### F.5 `get_more_info` 出参字段

支持大量扩展字段，下列均可用于 `field_list`（100+个）。不同证券、权限和数据包会导致部分字段缺失。

#### 基本信息

| 字段             | 说明               |
| -------------- | ---------------- |
| `MainBusiness` | 主营构成             |
| `SafeValue`    | 安全分              |
| `ShineValue`   | 亮点数              |
| `ShapeValue`   | 短期形态+中期形态+长期形态编号 |
| `TPFlag`       | 停牌标识             |
| `ZTPrice`      | 涨停价              |
| `DTPrice`      | 跌停价              |
| `HqDate`       | 行情日期             |

#### 行情指标

| 字段         | 说明        |
| ---------- | --------- |
| `fHSL`     | 换手率       |
| `fLianB`   | 量比        |
| `Wtb`      | 委比        |
| `Zsz`      | 总市值(亿)    |
| `Ltsz`     | 流通市值(亿)   |
| `vzangsu`  | 量涨速       |
| `Fzhsl`    | 分钟换手率     |
| `FzAmo`    | 2分钟金额(万元) |
| `VOpenZAF` | 抢筹涨幅      |

#### 涨幅系列

| 字段              | 说明      |
| --------------- | ------- |
| `ZAF`           | 涨幅      |
| `ZAFYesterday`  | 昨日涨幅    |
| `ZAFPre2D`      | 前天涨幅    |
| `ZAFPre5`       | 5日涨幅    |
| `ZAFPre10`      | 10日涨幅   |
| `ZAFPre20`      | 20日涨幅   |
| `ZAFPre30`      | 30日涨幅   |
| `ZAFPre60`      | 60日涨幅   |
| `ZAFYear`       | 年初至今涨幅  |
| `ZAFPreMyMonth` | 涨幅(本月来) |
| `ZAFPreOneYear` | 涨幅(一年来) |

#### 主力资金

| 字段       | 说明        |
| -------- | --------- |
| `Zjl`    | 主买净额(万元)  |
| `Zjl_HB` | 主力净流入(万元) |

#### L2数据

| 字段           | 说明      |
| ------------ | ------- |
| `TotalBVol`  | 总买量     |
| `TotalSVol`  | 总卖量     |
| `BCancel`    | 总撤买量    |
| `SCancel`    | 总撤卖量    |
| `L2TicNum`   | L2逐笔成交数 |
| `L2OrderNum` | L2逐笔委托数 |

#### 封单与竞价

| 字段            | 说明                  |
| ------------- | ------------------- |
| `FCAmo`       | 封单额(万元)             |
| `FCb`         | 封成比                 |
| `OpenAmo`     | 开盘金额(万元)(A股和板块指数有效) |
| `OpenZTBuy`   | 竞价涨停买入金额(万元)        |
| `OpenFDE`     | 开盘封单额(万元)           |
| `OpenAmoPre1` | 昨开盘金额(万元)           |
| `OpenVolPre1` | 昨开盘量                |
| `CJJEPre1`    | 昨成交额(万元)            |
| `CJJEPre3`    | 3日成交额(万元)           |
| `FDEPre1`     | 昨封单额(万元)            |
| `FDEPre2`     | 前封单额(万元)            |

#### 涨停相关

| 字段              | 说明        |
| --------------- | --------- |
| `ZTGPNum`       | 板块指数的涨停家数 |
| `LastStartZT`   | 几天        |
| `LastZTHzNum`   | 几板        |
| `EverZTCount`   | 连板天       |
| `ConZAFDateNum` | 连涨天数      |
| `YearZTDay`     | 年涨停天数     |

#### 估值与价格

| 字段             | 说明               |
| -------------- | ---------------- |
| `MA5Value`     | 5日均价             |
| `HisHigh`      | 52周最高            |
| `HisLow`       | 52周最低            |
| `IPO_Price`    | 发行价              |
| `More_YJL`     | ETF,LOF溢价率       |
| `BetaValue`    | 贝塔系数             |
| `DynaPE`       | 动态市盈率            |
| `MorePE`       | 市盈率(港股:动,其他扩展:静) |
| `StaticPE_TTM` | 市盈率(TTM)         |
| `DYRatio`      | 股息率              |
| `PB_MRQ`       | 市净率(MRQ)         |

#### 股票属性

| 字段           | 说明                   |
| ------------ | -------------------- |
| `IsT0Fund`   | 是否是T+0基金             |
| `IsZCZGP`    | 是否是注册制A股             |
| `IsKzz`      | 是否可转债                |
| `Kzz_HSCode` | 可转债对应的正股代码           |
| `QHMainYYMM` | 主力合约关联的月份(期货),主力和次主力 |
| `PreVolIn`   | 昨持仓量(期货期权有效)              |
| `VolInStock` | 持仓量(期货期权有效)                |
| `ClearPrice` | 结算价(期货期权有效)                |
| `PreMinum`   | 基差/溢价                        |

#### 财务扩展

| 字段             | 说明                |
| -------------- | ----------------- |
| `FreeLtgb`     | 自由流通股本(万)         |
| `Yield`        | 应计利息(债券),占款天数(回购) |
| `KfEarnMoney`  | 扣非净利润(万元)         |
| `RDInputFee`   | 研发费用(万元)          |
| `CashZJ`       | 货币资金(万元)          |
| `PreReceiveZJ` | 合同负债(万元)          |
| `OtherQYJzc`   | 其它权益工具(万元)        |
| `StaffNum`     | 员工人数              |

#### 近期事件日期

| 字段                  | 说明         |
| ------------------- | ---------- |
| `RecentGGJYDate`    | 最近北上大额交易日  |
| `RecentHGDate`      | 最近回购预案日    |
| `RecentIncentDate`  | 最近股权激励预案日  |
| `NoticeDate_Recent` | 最近业绩预告日    |
| `RecentReleaseDate` | 最近解禁日      |
| `RecentDZDate`      | 最近定增日      |
| `ReportDate`        | 最近财报公告日期   |
| `ZTDate_Recent`     | 近2年最近涨停板日期 |
| `DTDate_Recent`     | 近2年最近跌停板日期 |
| `TopDate_Recent`    | 近2年最近龙虎榜日期 |
| `StopJYDate_Recent` | 最近停牌日期     |

> 涨停跌停的判断：用 `get_more_info` 取 `FCAmo` 来判断涨停跌停，大于 0 是涨停，小于 0 是跌停。

### F.6 列表与关系接口出参字段

| 接口                         | 顶层返回类型                | 元素字段                                         | 字段说明                                       |
| -------------------------- | --------------------- | -------------------------------------------- | ------------------------------------------ |
| `get_stock_list`           | `list`                | `Code`                                       | `list_type=0` 时的证券代码                       |
| `get_stock_list`           | `list[dict]`          | `Code`、`Name`                                | `list_type=1` 时的证券代码和名称                    |
| `get_sector_list`          | `list` 或 `list[dict]` | `Code`、`Name`                                | A 股板块代码和板块名称，结构与 `get_stock_list('10')` 相同 |
| `get_sector_list_ds`       | `list` 或 `list[dict]` | `Code`、`Name`                                | 扩展市场板块代码与名称（`.UZ` 结尾），结构与 `get_stock_list('110')` 相同 |
| `get_stock_list_in_sector` | `list` 或 `list[dict]` | `Code`、`Name`                                | 板块成分证券代码和名称                                |
| `get_relation`             | `list[dict]`          | `BlockCode`、`BlockName`、`BlockType`、`GPNume` | 股票所属板块的代码、名称、类型与成份股数量                      |
| `get_trading_dates`        | `list[str]`           | 日期字符串                                        | 交易日，通常为 `YYYYMMDD`                         |

### F.7 专业数据公共出参结构

除 `get_financial_data` 外，`get_financial_data_by_date`、`get_gpjy_value`、`get_gpjy_value_by_date`、`get_bkjy_value`、`get_bkjy_value_by_date`、`get_scjy_value`、`get_scjy_value_by_date` 和 `get_gp_one_data` 均保留 DLL 原始专业数据结构。

`get_financial_data` 在每个字段均为等长列表且已安装 pandas 时，返回与 `tq.get_financial_data` 一致的「股票代码 → pandas.DataFrame」结构；否则保留原始字典。

股票或板块接口的通用层级为：

```python
{
    "600000.SH": {
        "GP03": [
            {"Date": "20250102", "Value": ["141405.89", "11113.00"]}
        ]
    }
}
```

市场接口不再包股票代码：

```python
{
    "SC01": [{"Date": "20250102", "Value": ["...", "..."]}],
    "SC02": [{"Date": "20250102", "Value": ["...", "..."]}],
}
```

公共字段：

| 字段        | 类型          | 说明                                     |
| --------- | ----------- | -------------------------------------- |
| 外层股票/板块代码 | `str`       | 股票/板块类接口的键；市场类接口没有这一层                  |
| 字段代码      | `str`       | 用户传入的 `FN*`、`GP*`、`BK*`、`SC*`、`GO*` 字段 |
| `Date`    | `str`       | 记录日期，通常 `YYYYMMDD`                     |
| `Value`   | `list[str]` | 该字段对应的一项或多项原始数值；数组下标语义由字段代码决定          |

#### 财务字段 `FN1` 至 `FN584`（完整）

`get_financial_data` 的最终出参与 `tq` 对齐：当 `announce_time`、`tag_time` 和所请求 `FN*` 字段均为等长列表时，`result[股票代码]` 是 `pandas.DataFrame`，列为这些字段、每行对应一条财务记录。

按时间模式还可能有 `announce_time`（公告日期）和 `tag_time`（报告期）字段；具体是否出现取决于 DLL 数据源。

**完整字段（官方文档 FN1~FN584，类型 `double`）：**

| 字段              | 类型       | 说明                                                                                                                                                  |
| --------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `announce_time` | `int`    | 公告日期                                                                                                                                                |
| `tag_time`      | `int`    | 报告期                                                                                                                                                 |
| `FN1`           | `double` | 基本每股收益                                                                                                                                              |
| `FN2`           | `double` | 扣除非经常性损益每股收益                                                                                                                                        |
| `FN3`           | `double` | 每股未分配利润                                                                                                                                             |
| `FN4`           | `double` | 每股净资产                                                                                                                                               |
| `FN5`           | `double` | 每股资本公积金                                                                                                                                             |
| `FN6`           | `double` | 净资产收益率                                                                                                                                              |
| `FN7`           | `double` | 每股经营现金流量                                                                                                                                            |
| `FN8`           | `double` | 货币资金                                                                                                                                                |
| `FN9`           | `double` | 交易性金融资产                                                                                                                                             |
| `FN10`          | `double` | 应收票据                                                                                                                                                |
| `FN11`          | `double` | 应收账款                                                                                                                                                |
| `FN12`          | `double` | 预付款项                                                                                                                                                |
| `FN13`          | `double` | 其他应收款                                                                                                                                               |
| `FN14`          | `double` | 应收关联公司款                                                                                                                                             |
| `FN15`          | `double` | 应收利息                                                                                                                                                |
| `FN16`          | `double` | 应收股利                                                                                                                                                |
| `FN17`          | `double` | 存货                                                                                                                                                  |
| `FN18`          | `double` | 其中：消耗性生物资产                                                                                                                                          |
| `FN19`          | `double` | 一年内到期的非流动资产                                                                                                                                         |
| `FN20`          | `double` | 其他流动资产                                                                                                                                              |
| `FN21`          | `double` | 流动资产合计                                                                                                                                              |
| `FN22`          | `double` | 可供出售金融资产                                                                                                                                            |
| `FN23`          | `double` | 持有至到期投资                                                                                                                                             |
| `FN24`          | `double` | 长期应收款                                                                                                                                               |
| `FN25`          | `double` | 长期股权投资                                                                                                                                              |
| `FN26`          | `double` | 投资性房地产                                                                                                                                              |
| `FN27`          | `double` | 固定资产                                                                                                                                                |
| `FN28`          | `double` | 在建工程                                                                                                                                                |
| `FN29`          | `double` | 工程物资                                                                                                                                                |
| `FN30`          | `double` | 固定资产清理                                                                                                                                              |
| `FN31`          | `double` | 生产性生物资产                                                                                                                                             |
| `FN32`          | `double` | 油气资产                                                                                                                                                |
| `FN33`          | `double` | 无形资产                                                                                                                                                |
| `FN34`          | `double` | 开发支出                                                                                                                                                |
| `FN35`          | `double` | 商誉                                                                                                                                                  |
| `FN36`          | `double` | 长期待摊费用                                                                                                                                              |
| `FN37`          | `double` | 递延所得税资产                                                                                                                                             |
| `FN38`          | `double` | 其他非流动资产                                                                                                                                             |
| `FN39`          | `double` | 非流动资产合计                                                                                                                                             |
| `FN40`          | `double` | 资产总计                                                                                                                                                |
| `FN41`          | `double` | 短期借款                                                                                                                                                |
| `FN42`          | `double` | 交易性金融负债                                                                                                                                             |
| `FN43`          | `double` | 应付票据                                                                                                                                                |
| `FN44`          | `double` | 应付账款                                                                                                                                                |
| `FN45`          | `double` | 预收款项                                                                                                                                                |
| `FN46`          | `double` | 应付职工薪酬                                                                                                                                              |
| `FN47`          | `double` | 应交税费                                                                                                                                                |
| `FN48`          | `double` | 应付利息                                                                                                                                                |
| `FN49`          | `double` | 应付股利                                                                                                                                                |
| `FN50`          | `double` | 其他应付款                                                                                                                                               |
| `FN51`          | `double` | 应付关联公司款                                                                                                                                             |
| `FN52`          | `double` | 一年内到期的非流动负债                                                                                                                                         |
| `FN53`          | `double` | 其他流动负债                                                                                                                                              |
| `FN54`          | `double` | 流动负债合计                                                                                                                                              |
| `FN55`          | `double` | 长期借款                                                                                                                                                |
| `FN56`          | `double` | 应付债券                                                                                                                                                |
| `FN57`          | `double` | 长期应付款                                                                                                                                               |
| `FN58`          | `double` | 专项应付款                                                                                                                                               |
| `FN59`          | `double` | 预计负债 （非流动负债）                                                                                                                                        |
| `FN60`          | `double` | 递延所得税负债                                                                                                                                             |
| `FN61`          | `double` | 其他非流动负债                                                                                                                                             |
| `FN62`          | `double` | 非流动负债合计                                                                                                                                             |
| `FN63`          | `double` | 负债合计                                                                                                                                                |
| `FN64`          | `double` | 实收资本（或股本）                                                                                                                                           |
| `FN65`          | `double` | 资本公积                                                                                                                                                |
| `FN66`          | `double` | 盈余公积                                                                                                                                                |
| `FN67`          | `double` | 减：库存股                                                                                                                                               |
| `FN68`          | `double` | 未分配利润                                                                                                                                               |
| `FN69`          | `double` | 少数股东权益                                                                                                                                              |
| `FN70`          | `double` | 外币报表折算价差                                                                                                                                            |
| `FN71`          | `double` | 非正常经营项目收益调整                                                                                                                                         |
| `FN72`          | `double` | 所有者权益（或股东权益）合计                                                                                                                                      |
| `FN73`          | `double` | 负债和所有者（或股东权益）合计                                                                                                                                     |
| `FN98`          | `double` | 销售商品、提供劳务收到的现金                                                                                                                                      |
| `FN99`          | `double` | 收到的税费返还                                                                                                                                             |
| `FN100`         | `double` | 收到其他与经营活动有关的现金                                                                                                                                      |
| `FN101`         | `double` | 经营活动现金流入小计                                                                                                                                          |
| `FN102`         | `double` | 购买商品、接受劳务支付的现金                                                                                                                                      |
| `FN103`         | `double` | 支付给职工以及为职工支付的现金                                                                                                                                     |
| `FN104`         | `double` | 支付的各项税费                                                                                                                                             |
| `FN105`         | `double` | 支付其他与经营活动有关的现金                                                                                                                                      |
| `FN106`         | `double` | 经营活动现金流出小计                                                                                                                                          |
| `FN107`         | `double` | 经营活动产生的现金流量净额                                                                                                                                       |
| `FN108`         | `double` | 收回投资收到的现金                                                                                                                                           |
| `FN109`         | `double` | 取得投资收益收到的现金                                                                                                                                         |
| `FN110`         | `double` | 处置固定资产、无形资产和其他长期资产收回的现金净额                                                                                                                           |
| `FN111`         | `double` | 处置子公司及其他营业单位收到的现金净额                                                                                                                                 |
| `FN112`         | `double` | 收到其他与投资活动有关的现金                                                                                                                                      |
| `FN113`         | `double` | 投资活动现金流入小计                                                                                                                                          |
| `FN114`         | `double` | 购建固定资产、无形资产和其他长期资产支付的现金                                                                                                                             |
| `FN115`         | `double` | 投资支付的现金                                                                                                                                             |
| `FN116`         | `double` | 取得子公司及其他营业单位支付的现金净额                                                                                                                                 |
| `FN117`         | `double` | 支付其他与投资活动有关的现金                                                                                                                                      |
| `FN118`         | `double` | 投资活动现金流出小计                                                                                                                                          |
| `FN119`         | `double` | 投资活动产生的现金流量净额                                                                                                                                       |
| `FN120`         | `double` | 吸收投资收到的现金                                                                                                                                           |
| `FN121`         | `double` | 取得借款收到的现金                                                                                                                                           |
| `FN122`         | `double` | 收到其他与筹资活动有关的现金                                                                                                                                      |
| `FN123`         | `double` | 筹资活动现金流入小计                                                                                                                                          |
| `FN124`         | `double` | 偿还债务支付的现金                                                                                                                                           |
| `FN125`         | `double` | 分配股利、利润或偿付利息支付的现金                                                                                                                                   |
| `FN126`         | `double` | 支付其他与筹资活动有关的现金                                                                                                                                      |
| `FN127`         | `double` | 筹资活动现金流出小计                                                                                                                                          |
| `FN128`         | `double` | 筹资活动产生的现金流量净额                                                                                                                                       |
| `FN129`         | `double` | 四、汇率变动对现金的影响                                                                                                                                        |
| `FN130`         | `double` | 四(2)、其他原因对现金的影响                                                                                                                                     |
| `FN131`         | `double` | 五、现金及现金等价物净增加额                                                                                                                                      |
| `FN132`         | `double` | 期初现金及现金等价物余额                                                                                                                                        |
| `FN133`         | `double` | 期末现金及现金等价物余额                                                                                                                                        |
| `FN134`         | `double` | 净利润                                                                                                                                                 |
| `FN135`         | `double` | 加：资产减值准备                                                                                                                                            |
| `FN136`         | `double` | 固定资产折旧、油气资产折耗、生产性生物资产折旧                                                                                                                             |
| `FN137`         | `double` | 无形资产摊销                                                                                                                                              |
| `FN138`         | `double` | 长期待摊费用摊销                                                                                                                                            |
| `FN139`         | `double` | 处置固定资产、无形资产和其他长期资产的损失                                                                                                                               |
| `FN140`         | `double` | 固定资产报废损失                                                                                                                                            |
| `FN141`         | `double` | 公允价值变动损失                                                                                                                                            |
| `FN142`         | `double` | 财务费用                                                                                                                                                |
| `FN143`         | `double` | 投资损失                                                                                                                                                |
| `FN144`         | `double` | 递延所得税资产减少                                                                                                                                           |
| `FN145`         | `double` | 递延所得税负债增加                                                                                                                                           |
| `FN146`         | `double` | 存货的减少                                                                                                                                               |
| `FN147`         | `double` | 经营性应收项目的减少                                                                                                                                          |
| `FN148`         | `double` | 经营性应付项目的增加                                                                                                                                          |
| `FN149`         | `double` | 其他                                                                                                                                                  |
| `FN150`         | `double` | 经营活动产生的现金流量净额2                                                                                                                                      |
| `FN151`         | `double` | 债务转为资本                                                                                                                                              |
| `FN152`         | `double` | 一年内到期的可转换公司债券                                                                                                                                       |
| `FN153`         | `double` | 融资租入固定资产                                                                                                                                            |
| `FN154`         | `double` | 现金的期末余额                                                                                                                                             |
| `FN155`         | `double` | 减：现金的期初余额                                                                                                                                           |
| `FN156`         | `double` | 加：现金等价物的期末余额                                                                                                                                        |
| `FN157`         | `double` | 减：现金等价物的期初余额                                                                                                                                        |
| `FN158`         | `double` | 现金及现金等价物净增加额                                                                                                                                        |
| `FN159`         | `double` | 流动比率(非金融类指标)                                                                                                                                        |
| `FN160`         | `double` | 速动比率(非金融类指标)                                                                                                                                        |
| `FN161`         | `double` | 现金比率(%)(非金融类指标)                                                                                                                                     |
| `FN162`         | `double` | 利息保障倍数(非金融类指标)                                                                                                                                      |
| `FN163`         | `double` | 非流动负债比率(%)(非金融类指标)                                                                                                                                  |
| `FN164`         | `double` | 流动负债比率(%)(非金融类指标)                                                                                                                                   |
| `FN166`         | `double` | 有形资产净值债务率(%)                                                                                                                                        |
| `FN167`         | `double` | 权益乘数(%)                                                                                                                                             |
| `FN168`         | `double` | 股东的权益/负债合计(%)                                                                                                                                       |
| `FN169`         | `double` | 有形资产/负债合计(%)                                                                                                                                        |
| `FN170`         | `double` | 经营活动产生的现金流量净额/负债合计(%)(非金融类指标)                                                                                                                       |
| `FN171`         | `double` | EBITDA/负债合计(%)(非金融类指标)                                                                                                                              |
| `FN172`         | `double` | 应收帐款周转率(非金融类指标)                                                                                                                                     |
| `FN173`         | `double` | 存货周转率(非金融类指标)                                                                                                                                       |
| `FN174`         | `double` | 运营资金周转率(非金融类指标)                                                                                                                                     |
| `FN175`         | `double` | 总资产周转率(非金融类指标)                                                                                                                                      |
| `FN176`         | `double` | 固定资产周转率(非金融类指标)                                                                                                                                     |
| `FN177`         | `double` | 应收帐款周转天数(非金融类指标)                                                                                                                                    |
| `FN178`         | `double` | 存货周转天数(非金融类指标)                                                                                                                                      |
| `FN179`         | `double` | 流动资产周转率(非金融类指标)                                                                                                                                     |
| `FN180`         | `double` | 流动资产周转天数(非金融类指标)                                                                                                                                    |
| `FN181`         | `double` | 总资产周转天数(非金融类指标)                                                                                                                                     |
| `FN182`         | `double` | 股东权益周转率(非金融类指标)                                                                                                                                     |
| `FN183`         | `double` | 营业收入增长率(%)                                                                                                                                          |
| `FN184`         | `double` | 净利润增长率(%)                                                                                                                                           |
| `FN185`         | `double` | 净资产增长率(%)                                                                                                                                           |
| `FN186`         | `double` | 固定资产增长率(%)                                                                                                                                          |
| `FN187`         | `double` | 总资产增长率(%)                                                                                                                                           |
| `FN188`         | `double` | 投资收益增长率(%)                                                                                                                                          |
| `FN189`         | `double` | 营业利润增长率(%)                                                                                                                                          |
| `FN190`         | `double` | 扣非每股收益同比(%)                                                                                                                                         |
| `FN191`         | `double` | 扣非净利润同比(%)                                                                                                                                          |
| `FN192`         | `double` | 暂无                                                                                                                                                  |
| `FN193`         | `double` | 成本费用利润率(%)                                                                                                                                          |
| `FN194`         | `double` | 营业利润率(非金融类指标)                                                                                                                                       |
| `FN195`         | `double` | 营业税金率(非金融类指标)                                                                                                                                       |
| `FN196`         | `double` | 营业成本率(非金融类指标)                                                                                                                                       |
| `FN197`         | `double` | 净资产收益率                                                                                                                                              |
| `FN198`         | `double` | 投资收益率                                                                                                                                               |
| `FN199`         | `double` | 销售净利率(%)                                                                                                                                            |
| `FN200`         | `double` | 总资产净利率                                                                                                                                              |
| `FN201`         | `double` | 净利润率(非金融类指标)                                                                                                                                        |
| `FN202`         | `double` | 销售毛利率(%)(非金融类指标)                                                                                                                                    |
| `FN203`         | `double` | 三费比重(非金融类指标)                                                                                                                                        |
| `FN204`         | `double` | 管理费用率(非金融类指标)                                                                                                                                       |
| `FN205`         | `double` | 财务费用率(非金融类指标)                                                                                                                                       |
| `FN206`         | `double` | 扣除非经常性损益后的净利润                                                                                                                                       |
| `FN207`         | `double` | 息税前利润(EBIT)                                                                                                                                         |
| `FN208`         | `double` | 息税折旧摊销前利润(EBITDA)                                                                                                                                   |
| `FN209`         | `double` | EBITDA/营业总收入(%)(非金融类指标)                                                                                                                             |
| `FN210`         | `double` | 资产负债率(%)                                                                                                                                            |
| `FN211`         | `double` | 流动资产比率(非金融类指标)                                                                                                                                      |
| `FN212`         | `double` | 货币资金比率(非金融类指标)                                                                                                                                      |
| `FN213`         | `double` | 存货比率(非金融类指标)                                                                                                                                        |
| `FN214`         | `double` | 固定资产比率                                                                                                                                              |
| `FN215`         | `double` | 负债结构比(非金融类指标)                                                                                                                                       |
| `FN216`         | `double` | 归属于母公司股东权益/全部投入资本(%)                                                                                                                                |
| `FN217`         | `double` | 股东的权益/带息债务(%)                                                                                                                                       |
| `FN218`         | `double` | 有形资产/净债务(%)                                                                                                                                         |
| `FN219`         | `double` | 每股经营性现金流(元)                                                                                                                                         |
| `FN220`         | `double` | 营业收入现金含量(%)(非金融类指标)                                                                                                                                 |
| `FN221`         | `double` | 经营活动产生的现金流量净额/经营活动净收益(%)                                                                                                                            |
| `FN222`         | `double` | 销售商品提供劳务收到的现金/营业收入(%)                                                                                                                               |
| `FN223`         | `double` | 经营活动产生的现金流量净额/营业收入                                                                                                                                  |
| `FN224`         | `double` | 资本支出/折旧和摊销                                                                                                                                          |
| `FN225`         | `double` | 每股现金流量净额(元)                                                                                                                                         |
| `FN226`         | `double` | 经营净现金比率（短期债务）(非金融类指标)                                                                                                                               |
| `FN227`         | `double` | 经营净现金比率（全部债务）                                                                                                                                       |
| `FN228`         | `double` | 经营活动现金净流量与净利润比率                                                                                                                                     |
| `FN229`         | `double` | 全部资产现金回收率                                                                                                                                           |
| `FN230`         | `double` | 营业收入                                                                                                                                                |
| `FN231`         | `double` | 营业利润                                                                                                                                                |
| `FN232`         | `double` | 归属于母公司所有者的净利润                                                                                                                                       |
| `FN233`         | `double` | 扣除非经常性损益后的净利润                                                                                                                                       |
| `FN234`         | `double` | 经营活动产生的现金流量净额                                                                                                                                       |
| `FN235`         | `double` | 投资活动产生的现金流量净额                                                                                                                                       |
| `FN236`         | `double` | 筹资活动产生的现金流量净额                                                                                                                                       |
| `FN237`         | `double` | 现金及现金等价物净增加额                                                                                                                                        |
| `FN238`         | `double` | 总股本                                                                                                                                                 |
| `FN239`         | `double` | 已上市流通A股                                                                                                                                             |
| `FN240`         | `double` | 已上市流通B股                                                                                                                                             |
| `FN241`         | `double` | 已上市流通H股                                                                                                                                             |
| `FN242`         | `double` | 股东人数(户)                                                                                                                                             |
| `FN243`         | `double` | 第一大股东的持股数量                                                                                                                                          |
| `FN244`         | `double` | 十大流通股东持股数量合计(股)                                                                                                                                     |
| `FN245`         | `double` | 十大股东持股数量合计(股)                                                                                                                                       |
| `FN246`         | `double` | 机构总量（家）                                                                                                                                             |
| `FN247`         | `double` | 机构持股总量(股)                                                                                                                                           |
| `FN248`         | `double` | QFII机构数                                                                                                                                             |
| `FN249`         | `double` | QFII持股量                                                                                                                                             |
| `FN250`         | `double` | 券商机构数                                                                                                                                               |
| `FN251`         | `double` | 券商持股量                                                                                                                                               |
| `FN252`         | `double` | 保险机构数                                                                                                                                               |
| `FN253`         | `double` | 保险持股量                                                                                                                                               |
| `FN254`         | `double` | 基金机构数                                                                                                                                               |
| `FN255`         | `double` | 基金持股量                                                                                                                                               |
| `FN256`         | `double` | 社保机构数                                                                                                                                               |
| `FN257`         | `double` | 社保持股量                                                                                                                                               |
| `FN258`         | `double` | 私募机构数                                                                                                                                               |
| `FN259`         | `double` | 私募持股量                                                                                                                                               |
| `FN260`         | `double` | 财务公司机构数                                                                                                                                             |
| `FN261`         | `double` | 财务公司持股量                                                                                                                                             |
| `FN262`         | `double` | 年金机构数                                                                                                                                               |
| `FN263`         | `double` | 年金持股量                                                                                                                                               |
| `FN264`         | `double` | 十大流通股东持有的流通A股合计(股)[ 注：2019半年报之前，季度报告中，若股东持股除了流通A股、还有流通B股或流通H股，指标264取的是包含流通B股或流通H股的流通股数]                                                             |
| `FN265`         | `double` | 第一大流通股东持股量(股)                                                                                                                                       |
| `FN266`         | `double` | 自由流通股(股)[注：1.自由流通股=已流通A股-持股5%以上股东的流通A股（一致行动人算一起）；2.指标按报告期展示，新股在上市日的下个报告期才有数据]                                                                       |
| `FN267`         | `double` | 受限流通A股(股)                                                                                                                                           |
| `FN268`         | `double` | 一般风险准备(金融类)                                                                                                                                         |
| `FN269`         | `double` | 其他综合收益(利润表)                                                                                                                                         |
| `FN270`         | `double` | 综合收益总额(利润表)                                                                                                                                         |
| `FN271`         | `double` | 归属于母公司股东权益(资产负债表)                                                                                                                                   |
| `FN272`         | `double` | 银行机构数(家)(机构持股)                                                                                                                                      |
| `FN273`         | `double` | 银行持股量(股)(机构持股)                                                                                                                                      |
| `FN274`         | `double` | 一般法人机构数(家)(机构持股)                                                                                                                                    |
| `FN275`         | `double` | 一般法人持股量(股)(机构持股)                                                                                                                                    |
| `FN276`         | `double` | 近一年净利润(元)                                                                                                                                           |
| `FN277`         | `double` | 信托机构数(家)(机构持股)                                                                                                                                      |
| `FN278`         | `double` | 信托持股量(股)(机构持股)                                                                                                                                      |
| `FN279`         | `double` | 特殊法人机构数(家)(机构持股)                                                                                                                                    |
| `FN280`         | `double` | 特殊法人持股量(股)(机构持股)                                                                                                                                    |
| `FN281`         | `double` | 加权净资产收益率(每股指标)                                                                                                                                      |
| `FN282`         | `double` | 扣非每股收益(单季度财务指标)                                                                                                                                     |
| `FN283`         | `double` | 最近一年营业收入(万元)                                                                                                                                        |
| `FN284`         | `double` | 国家队持股数量（万股)[注：本指标统计包含汇金公司、证金公司、外汇管理局旗下投资平台、国家队基金、国开、养老金以及中科汇通等国家队机构持股数量]                                                                            |
| `FN285`         | `double` | 业绩预告-本期归母净利润同比增幅下限%[注：指标285至294展示未来一个报告期的数据。例，3月31日至6月29日这段时间内展示的是中报的数据；如果最新的财务报告后面有多个报告期的业绩预告/快报，只能展示最新的财务报告后面的一个报告期的业绩预告/快报]                      |
| `FN286`         | `double` | 业绩预告-本期归母净利润同比增幅上限%                                                                                                                                 |
| `FN287`         | `double` | 业绩快报-归母净利润                                                                                                                                          |
| `FN288`         | `double` | 业绩快报-扣非净利润                                                                                                                                          |
| `FN289`         | `double` | 业绩快报-总资产                                                                                                                                            |
| `FN290`         | `double` | 业绩快报-净资产                                                                                                                                            |
| `FN291`         | `double` | 业绩快报-每股收益                                                                                                                                           |
| `FN292`         | `double` | 业绩快报-摊薄净资产收益率                                                                                                                                       |
| `FN293`         | `double` | 业绩快报-加权净资产收益率                                                                                                                                       |
| `FN294`         | `double` | 业绩快报-每股净资产                                                                                                                                          |
| `FN295`         | `double` | 应付票据及应付账款(资产负债表)                                                                                                                                    |
| `FN296`         | `double` | 应收票据及应收账款(资产负债表)                                                                                                                                    |
| `FN297`         | `double` | 递延收益(资产负债表-非流动负债)                                                                                                                                   |
| `FN298`         | `double` | 其他综合收益(资产负债表)                                                                                                                                       |
| `FN299`         | `double` | 其他权益工具(资产负债表)                                                                                                                                       |
| `FN300`         | `double` | 其他收益(利润表)                                                                                                                                           |
| `FN301`         | `double` | 资产处置收益(利润表)                                                                                                                                         |
| `FN302`         | `double` | 持续经营净利润(利润表)                                                                                                                                        |
| `FN303`         | `double` | 终止经营净利润(利润表)                                                                                                                                        |
| `FN304`         | `double` | 研发费用(利润表)                                                                                                                                           |
| `FN305`         | `double` | 其中:利息费用(利润表-财务费用)                                                                                                                                   |
| `FN306`         | `double` | 其中:利息收入(利润表-财务费用)                                                                                                                                   |
| `FN307`         | `double` | 近一年经营活动现金流净额                                                                                                                                        |
| `FN308`         | `double` | 近一年归母净利润(万元)                                                                                                                                        |
| `FN309`         | `double` | 近一年扣非净利润(万元)                                                                                                                                        |
| `FN310`         | `double` | 近一年现金净流量(万元)                                                                                                                                        |
| `FN311`         | `double` | 基本每股收益（单季度）                                                                                                                                         |
| `FN312`         | `double` | 营业总收入(单季度)(万元)                                                                                                                                      |
| `FN313`         | `double` | 业绩预告公告日期 [注：本指标展示未来一个报告期的数据。例,3月31日至6月29日这段时间内展示的是中报的数据；如果最新的财务报告后面有多个报告期的业绩预告/快报，只能展示最新的财务报告后面的一个报告期的业绩预告/快报的数据；公告日期格式为YYMMDD，例：190101代表2019年1月1日] |
| `FN314`         | `double` | 财报公告日期 [注：日期格式为YYMMDD,例：190101代表2019年1月1日]                                                                                                          |
| `FN315`         | `double` | 业绩快报公告日期 [注：本指标展示未来一个报告期的数据。例,3月31日至6月29日这段时间内展示的是中报的数据；如果最新的财务报告后面有多个报告期的业绩预告/快报，只能展示最新的财务报告后面的一个报告期的业绩预告/快报的数据；公告日期格式为YYMMDD，例：190101代表2019年1月1日] |
| `FN316`         | `double` | 近一年投资活动现金流净额(万元)                                                                                                                                    |
| `FN317`         | `double` | 业绩预告-本期归母净利润下限(万元)[注：指标317至318展示未来一个报告期的数据。例，3月31日至6月29日这段时间内展示的是中报的数据；如果最新的财务报告后面有多个报告期的业绩预告/快报，只能展示最新的财务报告后面的一个报告期的业绩预告/快报]                       |
| `FN318`         | `double` | 业绩预告-本期归母净利润上限(万元)                                                                                                                                  |
| `FN319`         | `double` | 营业总收入TTM(万元)                                                                                                                                        |
| `FN320`         | `double` | 员工总数(人)                                                                                                                                             |
| `FN321`         | `double` | 每股企业自由现金流                                                                                                                                           |
| `FN322`         | `double` | 每股股东自由现金流                                                                                                                                           |
| `FN323`         | `double` | 近一年营业利润(万元)                                                                                                                                         |
| `FN324`         | `double` | 净利润（单季度）(万元)                                                                                                                                        |
| `FN325`         | `double` | 北上资金数（家）(机构持股）                                                                                                                                      |
| `FN326`         | `double` | 北上资金持股量（股）(机构持股）                                                                                                                                    |
| `FN327`         | `double` | 有息负债率                                                                                                                                               |
| `FN328`         | `double` | 营业成本（单季度）(万元)                                                                                                                                       |
| `FN329`         | `double` | 投入资本回报率（ROIC）(获利能力分析)                                                                                                                               |
| `FN330`         | `double` | 业绩快报-营业收入（本期）                                                                                                                                       |
| `FN331`         | `double` | 业绩快报-营业收入（上期）                                                                                                                                       |
| `FN332`         | `double` | 业绩快报-营业利润（本期）                                                                                                                                       |
| `FN333`         | `double` | 业绩快报-营业利润（上期）                                                                                                                                       |
| `FN334`         | `double` | 业绩快报-利润总额（本期）                                                                                                                                       |
| `FN335`         | `double` | 业绩快报-利润总额（上期）                                                                                                                                       |
| `FN336`         | `double` | 审计意见 [注：0-未审计,1-无保留意见,2-带强调事项段的无保留意见,3-保留意见,4-无法表示意见,5-否定意见及其他]                                                                                     |
| `FN337`         | `double` | 股利支付率（%）                                                                                                                                            |
| `FN338`         | `double` | 近一年营业成本-非金融类(万元)                                                                                                                                    |
| `FN339`         | `double` | 近一年营业成本-金融类(万元)                                                                                                                                     |
| `FN340`         | `double` | 业绩预告-本期扣非后净利润下限(万元)                                                                                                                                 |
| `FN341`         | `double` | 业绩预告-本期扣非后净利润上限(万元)                                                                                                                                 |
| `FN342`         | `double` | 业绩预告-本期扣非后净利润同比增长下限（%）                                                                                                                              |
| `FN343`         | `double` | 业绩预告-本期扣非后净利润同比增长上限（%）                                                                                                                              |
| `FN344`         | `double` | 业绩预告-预告基本每股收益下限(元)                                                                                                                                  |
| `FN345`         | `double` | 业绩预告-预告基本每股收益上限(元)                                                                                                                                  |
| `FN346`         | `double` | 业绩预告-预告基本每股收益同比增长下限（%）                                                                                                                              |
| `FN347`         | `double` | 业绩预告-预告基本每股收益同比增长上限（%）                                                                                                                              |
| `FN348`         | `double` | 业绩预告-预告扣非后基本每股收益下限(元)                                                                                                                               |
| `FN349`         | `double` | 业绩预告-预告扣非后基本每股收益上限(元)                                                                                                                               |
| `FN350`         | `double` | 业绩预告-预告扣非后基本每股收益同比增长下限（%）                                                                                                                           |
| `FN351`         | `double` | 业绩预告-预告扣非后基本每股收益同比增长上限（%）                                                                                                                           |
| `FN352`         | `double` | 业绩预告-预告营业收入下限(万元)                                                                                                                                   |
| `FN353`         | `double` | 业绩预告-预告营业收入上限(万元)                                                                                                                                   |
| `FN354`         | `double` | 业绩预告-预告营业收入同比增长下限（%）                                                                                                                                |
| `FN355`         | `double` | 业绩预告-预告营业收入同比增长上限（%）                                                                                                                                |
| `FN356`         | `double` | 业绩预告-预告扣除后营业收入下限(万元)                                                                                                                                |
| `FN357`         | `double` | 业绩预告-预告扣除后营业收入上限(万元)                                                                                                                                |
| `FN358`         | `double` | 主营业务收入(内销)(万元)                                                                                                                                      |
| `FN359`         | `double` | 主营业务收入(外销)(万元)                                                                                                                                      |
| `FN360`         | `double` | 资管计划机构数(家)                                                                                                                                          |
| `FN361`         | `double` | 资管计划持股量(股)                                                                                                                                          |
| `FN362`         | `double` | 财务总评分                                                                                                                                               |
| `FN401`         | `double` | 专项储备(万元)                                                                                                                                            |
| `FN402`         | `double` | 结算备付金(万元)                                                                                                                                           |
| `FN403`         | `double` | 拆出资金(万元)                                                                                                                                            |
| `FN404`         | `double` | 发放贷款及垫款(万元)(流动资产科目)                                                                                                                                 |
| `FN405`         | `double` | 衍生金融资产(万元)                                                                                                                                          |
| `FN406`         | `double` | 应收保费(万元)                                                                                                                                            |
| `FN407`         | `double` | 应收分保账款(万元)                                                                                                                                          |
| `FN408`         | `double` | 应收分保合同准备金(万元)                                                                                                                                       |
| `FN409`         | `double` | 买入返售金融资产(万元)                                                                                                                                        |
| `FN410`         | `double` | 划分为持有待售的资产(万元)                                                                                                                                      |
| `FN411`         | `double` | 发放贷款及垫款(万元)(非流动资产科目)                                                                                                                                |
| `FN412`         | `double` | 向中央银行借款(万元)                                                                                                                                         |
| `FN413`         | `double` | 吸收存款及同业存放(万元)                                                                                                                                       |
| `FN414`         | `double` | 拆入资金(万元)                                                                                                                                            |
| `FN415`         | `double` | 衍生金融负债(万元)                                                                                                                                          |
| `FN416`         | `double` | 卖出回购金融资产款(万元)                                                                                                                                       |
| `FN417`         | `double` | 应付手续费及佣金(万元)                                                                                                                                        |
| `FN418`         | `double` | 应付分保账款(万元)                                                                                                                                          |
| `FN419`         | `double` | 保险合同准备金(万元)                                                                                                                                         |
| `FN420`         | `double` | 代理买卖证券款(万元)                                                                                                                                         |
| `FN421`         | `double` | 代理承销证券款(万元)                                                                                                                                         |
| `FN422`         | `double` | 划分为持有待售的负债(万元)                                                                                                                                      |
| `FN423`         | `double` | 预计负债(万元) （流动负债）                                                                                                                                     |
| `FN424`         | `double` | 递延收益(万元)（流动负债科目，公告此科目的股票较少，大部分公司没有此数据）                                                                                                              |
| `FN425`         | `double` | 其中:优先股(万元)(非流动负债科目)                                                                                                                                 |
| `FN426`         | `double` | 永续债(万元)(非流动负债科目)                                                                                                                                    |
| `FN427`         | `double` | 长期应付职工薪酬(万元)                                                                                                                                        |
| `FN428`         | `double` | 其中:优先股(万元)(所有者权益科目)                                                                                                                                 |
| `FN429`         | `double` | 永续债(万元)(所有者权益科目)                                                                                                                                    |
| `FN430`         | `double` | 债权投资(万元)                                                                                                                                            |
| `FN431`         | `double` | 其他债权投资(万元)                                                                                                                                          |
| `FN432`         | `double` | 其他权益工具投资(万元)                                                                                                                                        |
| `FN433`         | `double` | 其他非流动金融资产(万元)                                                                                                                                       |
| `FN434`         | `double` | 合同负债(万元)                                                                                                                                            |
| `FN435`         | `double` | 合同资产(万元)                                                                                                                                            |
| `FN436`         | `double` | 其他资产(万元)                                                                                                                                            |
| `FN437`         | `double` | 应收款项融资(万元)                                                                                                                                          |
| `FN438`         | `double` | 使用权资产(万元)                                                                                                                                           |
| `FN439`         | `double` | 租赁负债(万元)                                                                                                                                            |
| `FN440`         | `double` | 发放贷款及垫款(万元) [注：金融类科目]                                                                                                                               |
| `FN441`         | `double` | 应收款项(万元) [注：证券类指标]                                                                                                                                  |
| `FN442`         | `double` | 存出保证金(万元) [注：证券类指标]                                                                                                                                 |
| `FN443`         | `double` | 现金及存放中央银行款项(万元) [注：金融类科目]                                                                                                                           |
| `FN444`         | `double` | 贵金属(万元) [注：金融类科目]                                                                                                                                   |
| `FN445`         | `double` | 以公允价值计量且其变动计入当期损益的金融资产(万元) [注：金融类科目]                                                                                                                |
| `FN446`         | `double` | 代理业务资产(万元) [注：金融类科目]                                                                                                                                |
| `FN447`         | `double` | 应收款项类投资(万元) [注：金融类科目]                                                                                                                               |
| `FN448`         | `double` | 同业及其它金融机构存放款项(万元) [注：金融类科目]                                                                                                                         |
| `FN449`         | `double` | 以公允价值计量且其变动计入当期损益的金融负债(万元) [注：金融类科目]                                                                                                                |
| `FN450`         | `double` | 吸收存款(万元) [注：金融类科目]                                                                                                                                  |
| `FN451`         | `double` | 代理业务负债(万元) [注：金融类科目]                                                                                                                                |
| `FN452`         | `double` | 其他负债(万元) [注：金融类科目]                                                                                                                                  |
| `FN453`         | `double` | 发放贷款及垫款(万元) [注：金融类科目]                                                                                                                               |
| `FN501`         | `double` | 稀释每股收益(元)                                                                                                                                           |
| `FN502`         | `double` | 营业总收入(万元)                                                                                                                                           |
| `FN503`         | `double` | 汇兑收益(万元)                                                                                                                                            |
| `FN504`         | `double` | 其中:归属于母公司综合收益(万元)                                                                                                                                   |
| `FN505`         | `double` | 其中:归属于少数股东综合收益(万元)                                                                                                                                  |
| `FN506`         | `double` | 利息收入(万元)                                                                                                                                            |
| `FN507`         | `double` | 已赚保费(万元)                                                                                                                                            |
| `FN508`         | `double` | 手续费及佣金收入(万元)                                                                                                                                        |
| `FN509`         | `double` | 利息支出(万元)                                                                                                                                            |
| `FN510`         | `double` | 手续费及佣金支出(万元)                                                                                                                                        |
| `FN511`         | `double` | 退保金(万元)                                                                                                                                             |
| `FN512`         | `double` | 赔付支出净额(万元)                                                                                                                                          |
| `FN513`         | `double` | 提取保险合同准备金净额(万元)                                                                                                                                     |
| `FN514`         | `double` | 保单红利支出(万元)                                                                                                                                          |
| `FN515`         | `double` | 分保费用(万元)                                                                                                                                            |
| `FN516`         | `double` | 其中:非流动资产处置利得(万元)                                                                                                                                    |
| `FN517`         | `double` | 信用减值损失(万元)                                                                                                                                          |
| `FN518`         | `double` | 净敞口套期收益(万元)                                                                                                                                         |
| `FN519`         | `double` | 营业总成本(万元)                                                                                                                                           |
| `FN520`         | `double` | 信用减值损失(万元、2019格式)                                                                                                                                   |
| `FN521`         | `double` | 资产减值损失(万元、2019格式)                                                                                                                                   |
| `FN522`         | `double` | 其他业务收入(万元) [注：金融类科目]                                                                                                                                |
| `FN523`         | `double` | 业务及管理费(万元) [注：金融类科目]                                                                                                                                |
| `FN524`         | `double` | 其他业务成本(万元) [注：金融类科目]                                                                                                                                |
| `FN561`         | `double` | 加:其他原因对现金的影响2(万元)(现金的期末余额科目)                                                                                                                        |
| `FN562`         | `double` | 客户存款和同业存放款项净增加额(万元)                                                                                                                                 |
| `FN563`         | `double` | 向中央银行借款净增加额(万元)                                                                                                                                     |
| `FN564`         | `double` | 向其他金融机构拆入资金净增加额(万元)                                                                                                                                 |
| `FN565`         | `double` | 收到原保险合同保费取得的现金(万元)                                                                                                                                  |
| `FN566`         | `double` | 收到再保险业务现金净额(万元)                                                                                                                                     |
| `FN567`         | `double` | 保户储金及投资款净增加额(万元)                                                                                                                                    |
| `FN568`         | `double` | 处置以公允价值计量且其变动计入当期损益的金融资产净增加额(万元)                                                                                                                    |
| `FN569`         | `double` | 收取利息、手续费及佣金的现金(万元)                                                                                                                                  |
| `FN570`         | `double` | 拆入资金净增加额(万元)                                                                                                                                        |
| `FN571`         | `double` | 回购业务资金净增加额(万元)                                                                                                                                      |
| `FN572`         | `double` | 客户贷款及垫款净增加额(万元)                                                                                                                                     |
| `FN573`         | `double` | 存放中央银行和同业款项净增加额(万元)                                                                                                                                 |
| `FN574`         | `double` | 支付原保险合同赔付款项的现金(万元)                                                                                                                                  |
| `FN575`         | `double` | 支付利息、手续费及佣金的现金(万元)                                                                                                                                  |
| `FN576`         | `double` | 支付保单红利的现金(万元)                                                                                                                                       |
| `FN577`         | `double` | 其中:子公司吸收少数股东投资收到的现金(万元)                                                                                                                             |
| `FN578`         | `double` | 其中:子公司支付给少数股东的股利、利润(万元)                                                                                                                             |
| `FN579`         | `double` | 投资性房地产的折旧及摊销(万元)                                                                                                                                    |
| `FN580`         | `double` | 信用减值损失(万元)                                                                                                                                          |
| `FN581`         | `double` | 使用权资产折旧（万元）                                                                                                                                         |
| `FN582`         | `double` | 收取利息和手续费净增加额(万元) [注：金融类科目]                                                                                                                          |
| `FN583`         | `double` | 支付手续费的现金(万元) [注：金融类科目]                                                                                                                              |
| `FN584`         | `double` | 发行债券支付的现金(万元) [注：金融类科目]                                                                                                                             |

#### 股票交易字段 `GP01` 至 `GP46`

| 字段     | `Value` 数组含义                      |
| ------ | --------------------------------- |
| `GP01` | 股东人数 股东户数(户)                      |
| `GP02` | 龙虎榜 买入总计(万元) 卖出总计(万元)             |
| `GP03` | 融资融券1 融资余额(万元) 融券余量(股)            |
| `GP04` | 大宗交易 成交均价(元) 成交额(万元)              |
| `GP05` | 增减持1 成交均价(元) 变动股数(股)              |
| `GP06` | 陆股通持股量 持股数量(股)                    |
| `GP07` | 陆股通市场成交净额 陆股通市场净买入(万元)            |
| `GP08` | 龙虎榜机构(卖方)数据 卖方机构个数 机构卖出金额(万元)     |
| `GP09` | 龙虎榜机构(买方)数据 买方机构个数 机构买入金额(万元)     |
| `GP10` | 近3月机构调研情况 近3月机构调研次数 近3月调研机构数量     |
| `GP11` | 融资融券2 融资买入额(万元) 融资偿还额(万元)         |
| `GP12` | 融资融券3 融券卖出量(股) 融券偿还量(股)           |
| `GP13` | 融资融券4 融资净买入(万元) 融券净卖出(股)          |
| `GP14` | 涨停数据 涨停金额(万元) 开板次数                |
| `GP15` | 涨跌停 涨跌停状态 封单金额(万元)                |
| `GP16` | 总市值 总市值(万元)                       |
| `GP17` | 龙虎榜营业部数据 买入金额(万元) 卖出金额(万元)        |
| `GP18` | 龙虎榜沪深股通数据 买入金额(万元) 卖出金额(万元)       |
| `GP19` | 每周股票质押数量 无限售股份质押数(万) 有限售股份质押数(万)  |
| `GP20` | 每周股票质押比例 质押比例(%)                  |
| `GP21` | 股息率 股息率(%)                        |
| `GP22` | 涨跌停 封成比 封流比                       |
| `GP23` | 拟增减持 拟增持数量(万股) 拟减持数量(万股)          |
| `GP24` | 涨停 首次涨停时间 涨停最大封单额(万)              |
| `GP25` | 盘前盘后成交量 开盘成交量(手) 盘后固定成交量(手)       |
| `GP26` | 拟增减持金额 拟增持金额(万元) 拟减持金额(万元)        |
| `GP27` | 人气排名 市场人气排名 行业人气排名                |
| `GP28` | 股票回购 回购均价(元) 回购数量(万股)             |
| `GP29` | 证券信息 是否复牌日 是否更名日                  |
| `GP30` | 分红送转 派息金额(万元) 送转数量(股)             |
| `GP31` | 转融券 期初余量(股) 期末余量(股)               |
| `GP32` | 转融券 融出数量(股) 融出市值(元)               |
| `GP33` | 跌停数据 跌停金额(万元) 开板次数                |
| `GP34` | 跌停 首次跌停时间 跌停最大封单额(万)              |
| `GP35` | 增减持2 增持数量(股) 减持数量(股)              |
| `GP36` | 竞价涨停买 买入金额(万元)                    |
| `GP37` | 龙虎榜2 上榜类型连续交易日(天)                 |
| `GP38` | 涨停相关1 近1年涨停次数 近1年溢价5%次数           |
| `GP39` | 涨停相关2 近1年首板封板率(%) 近1年次日红盘率(%)     |
| `GP40` | 涨停相关3 近1年连板率(%) 最后涨停时间            |
| `GP41` | 股权登记日 配股股权登记日                     |
| `GP42` | 龙虎榜专业机构买卖净额 买方成交净额(万元) 卖方成交净额(万元) |
| `GP43` | 配股实施 配股价格(元) 配股数量(万股)             |
| `GP44` | 股票评分 综合评分                         |
| `GP45` | 评级系数 评级系数                         |
| `GP46` | 拟询价转让 拟转让股数(万股) 拟转让占总股本(%)        |

> 官方数据可用性备注（GP 字段）：
>
> - `GP02`：展示 20230717 之后的数据；`GP06`：展示 20170317 之后的数据；`GP07`：官方只公布了每日前十名数据
> - `GP14`/`GP22`：展示 20180319 之后的数据；`GP19`/`GP20`：展示 20180316 之后的数据；`GP36`：展示 20241101 之后的数据
> - `GP15`：涨跌停状态取值 涨停=2、曾涨停=1、跌停=-2、曾跌停=-1；跌停/曾跌停时封单金额取负值；展示 20160926 之后的数据
> - `GP24`：首次涨停时间展示 20160301 之后，涨停最大封单额展示 20200730 之后；`GP34`：首次跌停时间展示 20160301 之后，跌停最大封单额展示 20200730 之后
> - `GP25`：盘后固定成交量只包含科创板和创业板；`GP27`：行业排名为通达信二级研究行业排名
> - `GP29`：是否复牌日 0=不是复牌日，n(n>0)=停牌 n 个交易日之后的复牌日；是否更名日 0=未更名，1=常规更名，2=加ST，3=加*ST，4=摘帽，5=其他
> - `GP30`：对应展示日期为除权除息日；`GP33`：暂无跌停金额数据；`GP37`：上榜类型中指代的连续交易日类型

#### 板块交易字段 `BK5` 至 `BK19`

| 字段     | `Value` 数组含义                     |
| ------ | -------------------------------- |
| `BK5`  | 市盈率TTM 整体法 算术平均                  |
| `BK6`  | 市净率MRQ 整体法 算术平均                  |
| `BK7`  | 市销率TTM 整体法 算术平均                  |
| `BK8`  | 市现率TTM 整体法 算术平均                  |
| `BK9`  | 涨跌数 上涨家数 下跌家数                    |
| `BK10` | 板块总市值(亿元) 整体法 算术平均               |
| `BK11` | 板块流通市值(亿元) 整体法 算术平均              |
| `BK12` | 涨停数 涨停家数 曾涨停家数                   |
| `BK13` | 跌停数 跌停家数 曾跌停家数                   |
| `BK14` | 涨停数据 市场高度(不含ST股和未开板新股) 2板及以上涨停个数 |
| `BK15` | 融资融券 沪深京融资余额(万元) 沪深京融券余额(万元)     |
| `BK16` | 陆股通资金流入 沪股通流入金额(亿元) 深股通流入金额(亿元)  |
| `BK17` | 开盘成交数 开盘成交额(万元) 开盘成交量(万股)        |
| `BK18` | 板块股息率(%) 算数平均 整体法                |
| `BK19` | 板块自由流通市值(亿元) 整体法 算术平均            |

> 官方数据可用性备注（BK 字段）：
>
> - `BK12`/`BK13`：展示 20160926 之后的数据；`BK14`：市场高度与 2 板及以上涨停个数均不含 ST 股和未开板新股，展示 20180319 之后的数据；`BK16`：展示 20170320 之后的数据

#### 市场交易字段 `SC01` 至 `SC42`

| 字段     | `Value` 数组含义                       |
| ------ | ---------------------------------- |
| `SC01` | 融资融券 沪深京融资余额(万元) 沪深京融券余额(万元)       |
| `SC02` | 陆股通资金流入 沪股通流入金额(亿元) 深股通流入金额(亿元)    |
| `SC03` | 沪深京涨停股个数 涨停股个数 曾涨停股个数              |
| `SC04` | 沪深京跌停股个数 跌停股个数 曾跌停股个数              |
| `SC05` | 上证50股指期货 净持仓(手)                    |
| `SC06` | 沪深300股指期货 净持仓(手)                   |
| `SC07` | 中证500股指期货 净持仓(手)                   |
| `SC08` | ETF基金规模份额数据 ETF基金规模(亿份) ETF净申赎(亿份) |
| `SC09` | 沪月新开A股账户 沪月新开A股账户(万户)              |
| `SC10` | 增减持统计 增持额(万元) 减持额(万元)              |
| `SC11` | 大宗交易 溢价的大宗交易额(万元) 折价的大宗交易额(万元)     |
| `SC12` | 限售解禁 限售解禁计划额(亿元) 限售解禁股份实际上市金额(亿元)  |
| `SC13` | 分红 市场总分红额(亿元)                      |
| `SC14` | 募资 市场总募资额(亿元)                      |
| `SC15` | 打板资金 封板成功资金(亿元) 封板失败资金(亿元)         |
| `SC16` | 龙虎榜 买入总金额(亿元) 卖出总金额(亿元)            |
| `SC17` | 龙虎榜机构数据 买入金额(亿元) 卖出金额(亿元)          |
| `SC18` | 龙虎榜营业部数据 买入金额(亿元) 卖出金额(亿元)         |
| `SC19` | 龙虎榜沪深股通数据 买入金额(亿元) 卖出金额(亿元)        |
| `SC20` | 陆股通净买入 沪股通净买入额(亿元) 深股通净买入额(亿元)     |
| `SC21` | 每周无限售质押率 深市质押率(%) 沪市质押率(%)         |
| `SC22` | 每周有限售质押率 深市质押率(%) 沪市质押率(%)         |
| `SC23` | 连板家数 连板股个数(包含ST) 连板股个数(不含ST)       |
| `SC24` | 沪深京涨跌停股个数 涨停股个数(不含ST) 跌停股个数（不含ST）  |
| `SC25` | 融资融券 沪深京融资买入额(万元) 沪深京融券卖出量(万股)     |
| `SC26` | 每周市场质押比 每周市场质押比例(%)                |
| `SC27` | 央行公开市场净投放 央行公开市场净投放(亿元)            |
| `SC28` | 历史A股新高新低数 历史新高A股个数 历史新低A股个数        |
| `SC29` | 120天A股新高新低数 120天新高A股个数 120天新低A股个数  |
| `SC30` | 涨停数据 市场高度 2板以上涨停个数                 |
| `SC31` | 涨跌家数 涨家数 跌家数                       |
| `SC32` | 20天A股新高新低数 20天新高A股个数 20天新低A股个数     |
| `SC33` | 市场总封单金额 涨停封单金额(亿元) 跌停封单金额(亿元)      |
| `SC34` | 涨跌股成交量 上涨股成交量(万手) 下跌股成交量(万手)       |
| `SC35` | 涨停数据 换手板家数 回封率(%)                  |
| `SC36` | 曾涨跌停股个数 曾涨停股个数 曾跌停股个数              |
| `SC37` | 转融券 融出市值(亿元) 期末余额(亿元)              |
| `SC38` | ETF基金规模金额数据 ETF基金规模(亿元) ETF净申赎(亿元) |
| `SC39` | 涨跌5%家数 涨幅大于等于5%家数 跌幅大于等于5%家数       |
| `SC40` | 陆股通成交 陆股通成交总额(亿元) 陆股通成交总笔(万笔)      |
| `SC41` | 中证1000股指期货 净持仓(手)                  |
| `SC42` | 沪深股通成交金额 沪股通成交总额(亿元) 深股通成交总额(亿元)   |

> 官方数据可用性备注（SC 字段）：
>
> - `SC02`：沪股通限制展示 2000 条数据，深股通展示自 20161205 以后的数据；`SC03`/`SC04`/`SC15`/`SC24`/`SC33`/`SC36`：展示 20160926 之后的数据
> - `SC05`/`SC06`/`SC07`：展示 20171009 之后的数据；`SC41`：展示 20220722 之后的数据；`SC21`/`SC22`：展示 20180128 之后的数据；`SC26`：展示 20180316 之后的数据
> - `SC10`：部分公司公告滞后，每天查看的数据可能不同；`SC12`：展示 201802 月之后的数据，部分股票解禁日期延后可能使计划额变化
> - `SC13`：除权派息日的 A 股市场总分红额；`SC14`：发行/除权日期的首发、配股和增发的总募资额
> - `SC23`：连板股个数区分"包含ST和未开板新股"/"不含ST股和未开板新股"，展示 20180319 之后的数据
> - `SC28`/`SC29`/`SC32`：新高新低均为上市满一年的股票；`SC31`：涨跌家数均剔除停牌
> - `SC35`：换手板家数与回封率均剔除未开板新股，换手板家数展示 20190605 之后，回封率展示 20180927 之后；`SC36`：曾涨停剔除 ST 股和未开板新股，曾跌停剔除 ST 股

#### 股票单项字段 `GO1` 至 `GO47`

#### 基本信息

| 字段    | 说明        |
| ----- | --------- |
| `GO1` | 发行价(元)    |
| `GO2` | 总发行数量(万股) |

#### 一致预期

| 字段     | 说明                |
| ------ | ----------------- |
| `GO3`  | 一致预期目标价(元)        |
| `GO4`  | 一致预期T年度           |
| `GO5`  | 一致预期T年每股收益        |
| `GO6`  | 一致预期T+1年每股收益      |
| `GO7`  | 一致预期T+2年每股收益      |
| `GO8`  | 一致预期T年净利润(万元)     |
| `GO9`  | 一致预期T+1年净利润(万元)   |
| `GO10` | 一致预期T+2年净利润(万元)   |
| `GO11` | 一致预期T年营业收入(万元)    |
| `GO12` | 一致预期T+1年营业收入(万元)  |
| `GO13` | 一致预期T+2年营业收入(万元)  |
| `GO14` | 一致预期T年营业利润(万元)    |
| `GO15` | 一致预期T+1年营业利润(万元)  |
| `GO16` | 一致预期T+2年营业利润(万元)  |
| `GO17` | 一致预期T年每股净资产(元)    |
| `GO18` | 一致预期T+1年每股净资产(元)  |
| `GO19` | 一致预期T+2年每股净资产(元)  |
| `GO20` | 一致预期T年净资产收益率(%)   |
| `GO21` | 一致预期T+1年净资产收益率(%) |
| `GO22` | 一致预期T+2年净资产收益率(%) |
| `GO23` | 一致预期T年PE          |
| `GO24` | 一致预期T+1年PE        |
| `GO25` | 一致预期T+2年PE        |

#### 解禁与持股

| 字段     | 说明              |
| ------ | --------------- |
| `GO26` | 最新解禁日(YYMMDD格式) |
| `GO27` | 最新解禁数量（万股）      |
| `GO28` | 下一报告期的预约披露时间    |
| `GO29` | 最新持股机构家数        |
| `GO30` | 最新机构持股总量（万股）    |
| `GO31` | 最新持股基金家数        |
| `GO32` | 最新基金持股量（万股）     |
| `GO33` | 最新总股本（万股）       |
| `GO34` | 最新实际流通A股（万股）    |

#### 业绩预告

| 字段     | 说明                       |
| ------ | ------------------------ |
| `GO35` | 最新业绩预告 报告期               |
| `GO36` | 最新业绩预告 本期归母净利润下限（万元）     |
| `GO37` | 最新业绩预告 本期归母净利润上限（万元）     |
| `GO38` | 最新业绩预告 本期归母净利润预计同比增减幅下限% |
| `GO39` | 最新业绩预告 本期归母净利润预计同比增减幅上限% |
| `GO40` | 最新业绩快报 报告期               |
| `GO41` | 最新业绩快报 归母净利润（万元）         |

#### 分红募资

| 字段     | 说明            |
| ------ | ------------- |
| `GO42` | 分红募资 派现总额（万元） |
| `GO43` | 分红募资 募资总额（万元） |

#### 扣非业绩预告

| 字段     | 说明                       |
| ------ | ------------------------ |
| `GO44` | 最新业绩预告 本期扣非净利润下限(万元)     |
| `GO45` | 最新业绩预告 本期扣非净利润上限(万元)     |
| `GO46` | 最新业绩预告 本期扣非净利润预计同比增减幅下限% |
| `GO47` | 最新业绩预告 本期扣非净利润预计同比增减幅上限% |

> 官方数据可用性备注（GO 字段）：
>
> - `GO3`：一致预期值均为近半年内各家机构预测数值的平均值；`GO26`：最新解禁日为 YYMMDD 格式；`GO35`：最新业绩预告报告期为 YYMMDD 格式

### F.8 `get_kzz_info` 出参字段

| 字段            | 说明         |
| ------------- | ---------- |
| `SetCode`     | 证券市场       |
| `KZZCode`     | 可转债代码      |
| `HSCode`      | 正股代码       |
| `ZGPrice`     | 转股价格       |
| `CurRate`     | 当期利率       |
| `RestScope`   | 剩余规模（万）    |
| `PutBack`     | 回售触发价      |
| `ForceRedeem` | 强赎触发价      |
| `ZGDate`      | 转股日        |
| `EndPrice`    | 到期价        |
| `EndDate`     | 到期日期       |
| `ZGRate`      | 转股比率%      |
| `RealValue`   | 纯债价值（暂无）   |
| `ExpireYield` | 到期收益率%（暂无） |
| `KZZScore`    | 可转债评级      |
| `HSScore`     | 主体评级       |
| `RedeemDate`  | 赎回登记日期     |
| `RedeemPrice` | 赎回价格       |
| `PutDate`     | 回售申报起始日期   |
| `PutPrice`    | 回售价格       |
| `ZGCode`      | 转股代码       |
| `AGPrice`     | 正股当前价格     |
| `KZZPrice`    | 可转债当前价格    |
| `KZZYj`       | 溢价率        |
| `ZGValue`     | 转股价值       |

实际 DLL 版本还可能使用 `Code`、`Name`、`Price` 等同义键。生成代码筛选字段前应先不带 `field_list` 调用一次，按真实键名确定筛选列表。

### F.9 `get_trackzs_etf_info` 出参字段

| 字段         | 类型    | 说明     |
| ---------- | ----- | ------ |
| `Code`     | `str` | ETF 代码 |
| `Name`     | `str` | ETF 名称 |
| `NowPrice` | `str` | 现价     |
| `PreClose` | `str` | 昨收     |
| `IOPV`     | `str` | 净值     |
| `Zgb`      | `str` | 净额（万份） |
| `Sz`       | `str` | 规模（亿元） |

### F.10 `get_zdt_data` 出参字段

返回结果可为股票对象列表或按代码组织的对象。已知字段如下；数值字段通常为字符串：

> 官方注：此函数需要通达信客户端有短线宝权限（普及增强版、财富版、专业版等）。

| 字段                                  | 类型          | 说明                                                             |
| ----------------------------------- | ----------- | -------------------------------------------------------------- |
| `Code`                              | `str`       | 标准证券代码                                                         |
| `TimeNow`                           | `str`       | 当前时间                                                           |
| `ZDTStatusNow`                      | `str`       | 当前状态：1 自然涨停、2 一字涨停、3 曾涨停、4 自然跌停、5 一字跌停、6 曾跌停、7 涨超 10%、8 跌超 10% |
| `ZDTStatusOri`                      | `str`       | 历史状态，3 曾涨停、6 曾跌停                                               |
| `FirstTimeZT` / `FirstTimeDT`       | `str`       | 首次涨停 / 跌停时间                                                    |
| `LastOpenTimeZT` / `LastOpenTimeDT` | `str`       | 最后打开涨停 / 跌停时间                                                  |
| `LastTimeZT` / `LastTimeDT`         | `str`       | 最近涨停 / 跌停时间                                                    |
| `OpenTimesZT` / `OpenTimesDT`       | `str`       | 涨停 / 跌停打开次数                                                    |
| `FDVolMaxZT` / `FDVolMaxDT`         | `str`       | 最大涨停 / 跌停封单量                                                   |
| `VolZT`                             | `str`       | 涨停时累计成交量                                                       |
| `DaysZTSeries` / `DaysDTSeries`     | `str`       | 连续涨停 / 跌停天数                                                    |
| `ZtCauseAmount`                     | `str`       | 导致涨停的金额                                                        |
| `DateWrite`                         | `str`       | 本地数据写入日期，`YYYYMMDD`                                            |
| `LastStartZT` / `LastZTHzNum`       | `str`       | 最近 30 个交易日首涨停距今天数 / 该段涨停数                                      |
| `ZDTStatusDays`                     | `list[str]` | 近 3 天涨跌停状态，依次为今天、昨天、前天                                         |
| `ZTRecentDays`                      | `str`       | 最近 20 个交易日涨停数                                                  |
| `FirstTimeFZT` / `FirstTimeFDT`     | `str`       | 首次封涨停 / 封跌停时间                                                  |
| `HasCauseData`                      | `str`       | 是否包含导致涨停金额等原因数据                                                |

### F.11 `get_gb_info` 与 `get_gb_info_by_date` 出参字段

| 字段     | 类型       | 说明   |
| ------ | -------- | ---- |
| `Date` | `double` | 日期   |
| `Zgb`  | `double` | 总股本  |
| `Ltgb` | `double` | 流通股本 |

> 官方注：`get_gb_info_by_date` 须通过客户端或 `refresh_kline` 下载对应股票的日 K 线数据。

### F.12 `get_tick_data` 出参字段

`TdxAiData.h` 未给出 `GetTickDATsInStr` 的稳定列定义。已安装 pandas 时 `tqs` 把 DLL 返回的等长数组字段转为 DataFrame（列 `Time`/`Price`/`Volume`/`BSFlag`，行索引为默认整数索引，各列为字符串），标量字段不进入 DataFrame；无 pandas 或数组长度不一致时返回 DLL 原始 dict。生成解析代码前必须打印一次真实返回对象并按实际键处理。常见字段如下：

| 字段                      | 常见类型      | 说明                               |
| ------------------------- | ------------- | ---------------------------------- |
| `TotalNum`                | `int` / `str` | 本次或可用逐笔记录数，仅 dict 返回 |
| `Time`                    | `str`         | 成交时间，通常 `HHMMSS`            |
| `Price`                   | `str`         | 成交价                             |
| `Volume`                  | `str`         | 成交量                             |
| `Amount`                  | `str`         | 成交额                             |
| `BuyOrSell` / `InOutFlag` | `str`         | 买卖方向或内外盘标志               |

不得假定所有 DLL 版本必定含有 `Content`、`Price` 或 `BuyOrSell`；分笔分页推进必须优先使用真实记录条数（DataFrame 用 `len(df)`，dict 用 `TotalNum`）。

### F.13 订阅接口出参字段

| 接口                            | 返回或回调字段                   | 类型     | 说明                              |
| ----------------------------- | ------------------------- | ------ | ------------------------------- |
| `subscribe`                   | 函数返回值                     | `None` | 成功注册异步回调后立即返回                   |
| `subscribe`                   | `callback(data)` 的 `data` | `str`  | DLL 原始行情字符串；当前 `tqs` 不负责解析为固定字典 |
| `unsubscribe`                 | 函数返回值                     | `None` | 已调用底层全局取消订阅                     |
| `get_subscribe_hq_stock_list` | 列表元素                      | `str`  | 当前 Python 进程记录的带市场后缀股票代码        |

### F.14 集合竞价与批量接口出参字段

批量接口返回的字段集合与对应单只接口一致。

| 接口                                          | 实测字段数 | 字段说明位置               |
| --------------------------------------------- | ---------: | -------------------------- |
| `get_call_auction` / `get_call_auction_batch` |          4 | 见下方集合竞价字段表       |
| `get_market_snapshot_batch`                   |         26 | §9 F.3（实时行情快照字段） |
| `get_stock_info_batch`                        |         62 | §9 F.4（证券基础信息字段） |
| `get_more_info_batch`                         |         92 | §9 F.5（扩展证券信息字段） |
| `get_kzz_info_batch`                          |         27 | §9 F.8（可转债信息字段）   |

集合竞价（`get_call_auction` / `get_call_auction_batch`）字段：

| 字段       | 类型  | 说明       |
| ---------- | ----- | ---------- |
| `Time`     | `str` | 时间       |
| `Price`    | `str` | 虚拟匹配价 |
| `Volume`   | `str` | 虚拟匹配量 |
| `LeaveQty` | `str` | 未匹配量   |

> 集合竞价返回中不含 `InOutFlag`（内外盘 / 方向标志）字段。
> 字段含义以 DLL 实际返回为准；`LeaveQty` 等语义头文件未给出枚举定义。
> `type=0` 返回早盘与尾盘两段记录（`600000.SH` 实测 129 条，09:15:00~14:59:57）；`type=1` 仅早盘（同标的 70 条，09:15:00~09:24:57）。


### F.15 `get_domain_sorthq` 出参字段

固定返回列（不受 `wantcol` 影响）：

| 字段       | 类型    | 说明            |
| -------- | ----- | ------------- |
| `Code`   | `str` | 证券代码，带市场后缀    |
| `Name`   | `str` | 证券名称          |
| `Now`    | `str` | 最新价           |
| `Close`  | `str` | 昨收价           |
| `ZAF`    | `str` | 涨幅（%）         |
| `Volume` | `str` | 成交量           |

条件返回列：

| 字段             | 返回条件                     | 说明                                   |
| -------------- | ------------------------ | ------------------------------------ |
| `CurCol`       | `colname` 不是上述固定栏目时      | 当前排序栏目的值                             |
| `wantcol` 对应列  | `wantcol` 中存在有效栏目时       | 按规范名追加，如 `Amount`、`fHSL`、`Ltsz`、`Zsz` |

`domain` 支持下列六类取值。**沪深京市场域、板块域、分类域在 `data_source=0` 下使用；扩展行情板块域须传 `data_source=1`。**

**一、沪深京市场域**

| domain | 域           | 实测条数         |
| -----: | ----------- | -----------: |
|      0 | 上证A股        |         2320 |
|      1 | 上证B股        |           41 |
|      2 | 深证A股        |         2908 |
|      3 | 深证B股        |           38 |
|      4 | 上证债券        |        21441 |
|      5 | 深证债券        |        19828 |
|      6 | 沪深京A股       |         5578 |
|      7 | 沪深B股        |           79 |
|      8 | 科创板         |          618 |
|      9 | 沪深基金        |         2503 |
|     10 | 全品种（含债券、基金） | ≥32767（受分页上限约束） |
|     11 | 沪深京指数       |          559 |
|     12 | 北交所         |          350 |
|     13 | 无效域         |            0 |
|     14 | 创业板         |         1412 |

**二、沪深京板块域**

| domain | 分类                                            |
| -----: | --------------------------------------------- |
|  10000 | 综合板块（缺省行业分类 + 概念板块 + 地区板块 + 风格板块）             |
|  10001 | 板块数据（缺省行业分类）                                  |
|  10002 | 板块数据（缺省细分行业分类）                                |
|  10004 | 概念板块                                          |
|  10005 | 风格板块                                          |
|  10006 | 地区板块                                          |
|  10007 | 缺省行业分类 + 概念板块                                 |
|  10008 | 研究行业一级                                        |
|  10009 | 研究行业二级                                        |
|  10010 | 研究行业三级                                        |
|  20000 | 前缀 `880` 板块成份股（`20000` + 代码后三位）               |
|  21000 | 前缀 `881` 板块成份股（`21000` + 代码后三位）               |
|  22000 | 前缀 `882` 板块成份股（`22000` + 代码后三位）               |
|  30000 | 指数板块成份股起始序号（深 `30XXX` / 沪 `31XXX` / 北 `32XXX`） |

**三、沪深京分类域**（`11000` 系列，共 36 个）

| domain | 板块             | domain | 板块           |
| -----: | ---------------- | -----: | -------------- |
|  11000 | 沪深主板         |  11033 | 期货ETF        |
|  11001 | 沪股通           |  11034 | 跨市场ETF      |
|  11002 | 沪深300成份股    |  11043 | 跟踪指数ETF    |
|  11003 | 融资融券标的     |  11044 | 科技ETF        |
|  11004 | 沪深重点指数     |  11045 | 商品LOF        |
|  11005 | 沪深ETF基金      |  11046 | 双创基金       |
|  11006 | 新股次新股       |  11047 | REITs基金      |
|  11007 | 风险警示板（ST） |  11048 | T+0基金        |
|  11008 | 退市整理板       |  11050 | 可转债标的     |
|  11009 | 深股通           |  11051 | 沪深股通ETF    |
|  11010 | CDR              |  11052 | 沪深主要指数   |
|  11012 | 沪深LOF基金      |  11053 | 中证500成份股  |
|  11023 | 可转换债券       |  11054 | 中证1000成份股 |
|  11027 | 债券类ETF        |  11055 | 富时A50成份股  |
|  11028 | 货币类ETF        |  11057 | 中证2000成份股 |
|  11029 | 黄金类ETF        |  11058 | 中证A500成份股 |
|  11030 | T+0货币基金      |  11059 | 贵金属主题基金 |
|  11031 | 跨境ETF和QDII    |  11080 | 科创成长层     |

**四、扩展行情—期货期权板块域**（须 `data_source=1`）

| domain | 分类       |
| -----: | -------- |
|      8 | 上海个股期权   |
|      9 | 深圳个股期权   |
|     28 | 郑州期货     |
|     29 | 大连期货     |
|     30 | 上海期货     |
|     47 | 中金所期货    |
|     60 | 主力期货合约   |
|     66 | 广州期货     |

**五、扩展行情—港股美股板块域**（须 `data_source=1`）

| domain | 分类                      | domain | 分类             |
| -----: | ----------------------- | -----: | -------------- |
|  10000 | 港股综合板块（行业分类 + 概念板块）     |  12027 | 港股通ETF        |
|  10001 | 港股板块（行业分类）              |  12030 | 港交所REITs      |
|  10002 | 港股板块（概念板块）              |  12035 | 蓝筹股           |
|  10003 | 美股综合板块（行业分类 + 概念板块）     |  13001 | 美股-中概股        |
|  10004 | 美股板块（行业分类）              |  13002 | 美股-明星股        |
|  10005 | 美股板块（概念板块）              |  13003 | 道琼斯成份股        |
|  20000 | 前缀 `HK` 板块成份股（+ 代码后四位）  |  13004 | 标普成份股         |
|  30000 | 前缀 `US` 板块成份股（+ 代码后四位）  |  13005 | 纳斯达克100       |
|  12001 | 恒指成份股                   |  13006 | 美股-热门ETF      |
|  12002 | 红筹股                     |  13007 | 美股-次新股        |
|  12004 | 国企股                     |  13008 | 美股-REITs      |
|  12005 | 国企A+H股                  |  13009 | 美股-费城半导体      |
|  12006 | 港股通标的                   |        |                |
|  12007 | 国指成份股                   |        |                |
|  12008 | 红筹成份股                   |        |                |
|  12009 | 港股-次新股                  |        |                |
|  12012 | 恒生科技成份股                 |        |                |

**六、指数成分域**（取任意指数的成分股）

编码规则：`30XXX` 深证、`31XXX` 上证、`32XXX` 北交所，其中 `XXX` 为**指数代码的后三位**。

| domain | 目标指数        | 指数代码    |
| -----: | ----------- | ------- |
|  30001 | 深证成指        | 399001  |
|  30006 | 创业板指        | 399006  |
|  30905 | 中证500       | 399905  |
|  31016 | 上证50        | 000016  |
|  32050 | 北证50        | —       |

> 例：取中证1000（代码 `399852`）成分股，传 `domain=30852`。

> 实测说明：`11008` 退市整理板返回空属正常（当前无该状态标的）；`10001`/`10002`/`10004`/`10005` 在 `data_source=0` 下即为上表「沪深京板块域」的板块数据 / 概念板块 / 风格板块，返回对象是板块指数（如 `10001` 首条为 `881418.SH 房地产开发`），传 `data_source=1` 时才对应港股、美股板块；`120xx`、`130xx` 系列在 `data_source=0` 下返回空，须传 `data_source=1`。

`colname` 与 `wantcol` 共用同一套栏目取值，共 36 个。**下表「入参值」列是唯一合法的写法，须按原样书写**：

| 栏目 | 入参值 | 适用范围 |
| :--- | :--- | :--- |
| 代码 | `Code` | 扩展也支持 |
| 昨收 | `LastClose` | 扩展也支持 |
| 现价 | `Now` | 扩展也支持 |
| 总手 | `Volume` | 扩展也支持 |
| 总金额 | `Amount` | 万元，扩展也支持 |
| 涨幅 | `ZAF` | 扩展也支持 |
| 振幅 | `ZEF` | 扩展也支持 |
| 市盈率 / 期货期权持仓量 | `Syl` | 扩展也支持 |
| 委比 | `Wtb` | 扩展也支持 |
| 外盘（板块指数为涨停家数） | `ZTGPNum` | — |
| 封单额 | `FCAmo` | 万元 |
| 开盘成交额 | `OpenAmo` | 万元 |
| 开盘换手率 | `OpenHSL` | — |
| 量比 | `fLianB` | 扩展也支持 |
| 换手率 | `fHSL` | 扩展也支持 |
| 流通市值 | `Ltsz` | 亿元，扩展也支持 |
| 总市值 | `Zsz` | 亿元，扩展也支持 |
| 涨速 | `Zangsu` | 扩展也支持 |
| 活跃度 | `ItemNum` | 扩展也支持 |
| 量涨速 | `vzangsu` | — |
| 主买净额 | `Zjl` | 万元，扩展也支持 |
| 主力净额 | `Zjl_HB` | 万元 |
| 竞价涨停委买额 | `OpenZTBuy` | 万元 |
| 开盘昨比% | `OpenLastB` | — |
| 2 分钟金额 | `FzAmo` | 万元 |
| 连涨天数 | `ConZAFDateNum` | 扩展也支持 |
| 连续涨停天数 | `ZTDateNum` | — |
| 连续跌停天数 | `DTDateNum` | — |
| 昨日涨幅% | `ZAFYesterday` | 扩展也支持 |
| 3 日涨幅% | `ZAFPre3` | 扩展也支持 |
| 最近 5 日的涨幅% | `ZAFPre5` | — |
| 最近 10 日的涨幅% | `ZAFPre10` | — |
| 盘后成交额 | `PHAmo` | 万元 |
| 20 日涨幅% | `ZAFPre20` | 扩展也支持 |
| 60 日涨幅% | `ZAFPre60` | 扩展也支持 |
| 年初至今% | `ZAFYear` | 扩展也支持 |

> `Code`、`LastClose`、`Now`、`Volume`、`ZAF` 已在固定返回列中体现（对应返回列名 `Code`、`Close`、`Now`、`Volume`、`ZAF`），写进 `wantcol` 不占附加栏目名额；`wantcol` 有效栏目超过 5 个时只取前 5 个。

---
### F.16 `get_global_data` 出参字段

字段随 `type` 不同：`type=0` 返回 87 个字段，`type=1` 返回 53 个字段，两组字段名无交集。

**`type=0` 沪深京市场（87 个字段）**

标量统计字段（27 个）

| 字段               | 类型    | 说明                       |
| ------------------ | ------- | -------------------------- |
| `CurDate`          | `int`   | 交易日，形如 `20261023`    |
| `CurHms`           | `int`   | 行情时间，形如 `153100`    |
| `UpNum`            | `int`   | 上涨家数                   |
| `DownNum`          | `int`   | 下跌家数                   |
| `LevelNum`         | `int`   | 平盘家数                   |
| `ZtNum`            | `int`   | 涨停家数                   |
| `DtNum`            | `int`   | 跌停家数                   |
| `ZrztZAF`          | `float` | 昨日涨停股今日涨幅（%）    |
| `Zjlx_Main`        | `float` | 主力净额（万元）           |
| `Zjlx_JBuy`        | `float` | 主买净额（万元）           |
| `EverZTNum`        | `int`   | 炸板数                     |
| `LBNum`            | `int`   | 连板数                     |
| `MaxLBDay`         | `int`   | 最高连板数                 |
| `AG_Score1`        | `int`   | 全部 A 股得分 1            |
| `AG_Score2`        | `int`   | 全部 A 股得分 2            |
| `AG_LB`            | `float` | 全部 A 股量能              |
| `A999999_MA5Diff`  | `float` | 上证指数距 5 日均线（%）   |
| `MA5_UP`           | `int`   | 位于 5 日均线上方的家数    |
| `MA5_Down`         | `int`   | 位于 5 日均线下方的家数    |
| `Year_UP`          | `int`   | 创一年来新高的家数         |
| `Year_Down`        | `int`   | 创一年来新低的家数         |
| `HYBK_NowUP`       | `int`   | 二级研究行业中上涨家数     |
| `HYBK_NowDown`     | `int`   | 二级研究行业中下跌家数     |
| `GNBK_NowUP`       | `int`   | 概念板块中上涨家数         |
| `GNBK_NowDown`     | `int`   | 概念板块中下跌家数         |
| `tsum`             | `int`   | 当日总家数                 |
| `tlevel`           | `int`   | 横盘家数                   |

指数与市场字段（56 个）：由「指数前缀 + 后缀」组成，前缀对应指数，后缀对应指标。

| 后缀          | 类型    | 说明                       |
| ------------- | ------- | -------------------------- |
| `_Now`        | `float` | 指数点位                   |
| `_ZAF`        | `float` | 涨幅（%）                  |
| `_Amo`        | `float` | 成交额（万元）             |
| `_LastAmo`    | `float` | 昨日成交额（万元）         |
| `_LastCurAmo` | `float` | 昨日同一时刻成交额（万元） |
| `_Zjl`        | `float` | 主力净额（万元）           |

| 指数前缀   | 指数名称   | 具备的后缀                                                       | 字段数 |
| ---------- | ---------- | ---------------------------------------------------------------- | ------ |
| `A999999`  | 上证指数   | `_Now` `_ZAF` `_Amo` `_LastAmo` `_LastCurAmo` `_Zjl`             | 6      |
| `A399001`  | 深证成指   | 同上                                                             | 6      |
| `A899050`  | 北证50     | 同上                                                             | 6      |
| `A880096`  | ETF 指数   | 同上                                                             | 6      |
| `A000300`  | 沪深300    | 同上                                                             | 6      |
| `A000905`  | 中证500    | 同上                                                             | 6      |
| `A000852`  | 中证1000   | 同上                                                             | 6      |
| `A880823`  | 微盘股     | 同上                                                             | 6      |
| `A399006`  | 创业板指   | `_Now` `_ZAF` `_Zjl`                                             | 3      |
| `A000688`  | 科创50     | `_Now` `_ZAF` `_Zjl`                                             | 3      |
| `A880008`  | 全A等权    | `_Now` `_ZAF`                                                    | 2      |

数组字段（4 个）

| 字段     | 类型        | 说明                                                                                       |
| -------- | ----------- | ------------------------------------------------------------------------------------------ |
| `tup`    | `List[int]` | 上涨家数分布，11 个元素：`[0]` 为涨停，`[1]` 为涨幅 ≥9%，`[2]`~`[9]` 依次为 8%~9% 至 1%~2%，`[10]` 为 0~1% |
| `tdown`  | `List[int]` | 下跌家数分布，11 个元素：`[0]` 为跌幅 0~1%，`[1]` 为 1%~2%，`[2]`~`[9]` 依次至 ≥9%，`[10]` 为跌停 |
| `tyx`    | `List[int]` | 当日阴阳线，2 个元素：`[0]` 阳线家数、`[1]` 阴线家数                                       |
| `tkb`    | `List[int]` | 开板数，2 个元素：`[0]` 涨停开板数、`[1]` 跌停开板数                                       |

**`type=1` 全球市场（53 个字段）**

由「品种代码 + 后缀」组成（51 个），另有 2 个港股涨跌家数字段。

| 后缀        | 类型  | 说明           |
| ----------- | ----- | -------------- |
| `_ZS_Now`   | `str` | 点位           |
| `_ZS_ZAF`   | `str` | 涨幅（%）      |
| `_ZS_Amo`   | `str` | 成交额（万元） |

| 品种代码 | 名称                 | 品种代码 | 名称                   |
| -------- | -------------------- | -------- | ---------------------- |
| `HSI`    | 恒生指数             | `GC00W`  | COMEX 黄金             |
| `HSCEI`  | 恒生国企指数         | `SI00W`  | COMEX 白银             |
| `HSTECH` | 恒生科技指数         | `HG00W`  | COMEX 铜               |
| `IXIC`   | 纳斯达克             | `CL00W`  | NYMEX 原油             |
| `HXC`    | 纳指金龙中国指数     | `NG00W`  | NYMEX 天然气           |
| `DAX`    | 德国 DAX 指数        | `IFL8`   | 沪深主连               |
| `CNY0`   | 富时 A50 期指连续    | `IHL8`   | 上证主连               |
| `USDCNH` | 美元兑离岸人民币     | `ICL8`   | 中证主连               |
| `IML8`   | 中千主连             |          |                        |

| 字段         | 类型  | 说明           |
| ------------ | ----- | -------------- |
| `HK_UpNum`   | `int` | 港股上涨家数   |
| `HK_DownNum` | `int` | 港股下跌家数   |

> `type=1` 除 `HK_UpNum` / `HK_DownNum` 为整数外，其余取值**均为字符串**（如 `"23093.37"`），需要数值时自行转换；`type=0` 的取值为 `float` / `int`。
> `_Amo` / `_ZS_Amo` 单位为万元；`_ZAF` / `_ZS_ZAF` 取值本身即为百分数（`0.89` 表示 0.89%）。
> `_LastAmo` / `_LastCurAmo` 为上一交易日口径的统计值。
> 无数据的品种返回 `"0.00"`（实测 `IXIC`、`HXC` 全为 0，`DAX` / `CNY0` / `USDCNH` 的成交额为 0）；市场整体无数据时返回空结果。

### F.17 批量公式接口出参字段

`formula_process_mul_zb` / `formula_process_mul_xg` / `formula_process_mul_exp` 返回 `{证券代码: {输出线名: [...], 'Time': [...]}}`。

| 键       | 类型   | 说明                                                                                            |
| -------- | ------ | ----------------------------------------------------------------------------------------------- |
| 证券代码 | `str`  | 带市场后缀，如 `600000.SH`                                                                      |
| 输出线名 | `list` | 公式输出线的取值序列；`return_date=False` 为纯值数组，`True` 为 `[{"Date": ..., "Value": ...}]` |
| `Time`   | `list` | 各条记录的时间，格式 `HHMMSS`                                                                   |

> 输出线名由公式本身决定（如 `MA` 指标输出 `MA5`、`MA10`）；`formula_process_mul_xg`（条件选股）只在窗口内至少命中过一条结果的证券才会出现在返回中。
> 公式不存在、参数越界或结果超过 64KiB 时返回 `{}`。

### F.18 `formula_by_src` / `formula_by_file` 出参字段

返回结构与 F.17 相同，输出线名由传入的临时公式源码决定；公式不存在、参数越界、结果超过 64KiB 或服务端不支持临时公式源码协议时返回 `{}`。

### F.19 `get_misc_data` 出参字段

返回 `{栏目名: 内容}`，栏目名与内容均为字符串。栏目由服务端模板决定，**集合与顺序随证券变化**，不可假定固定数量；实测 `000001.SZ` 为下列 12 项。

| 栏目名       | 说明                         | 取值示例                                              |
| ------------ | ---------------------------- | ----------------------------------------------------- |
| `行业地位`   | 公司在行业中的定位与控股背景 | 由平安集团控股的一家跨区域经营的股份制商业银行        |
| `行业地域`   | 行业与经营地域归类           | 全国性银行                                            |
| `地区`       | 注册地                       | 广东-深圳市                                           |
| `主营业务`   | 经营范围全文                 | （一）吸收公众存款；（二）发放短期、中期和长期贷款；… |
| `所属板块`   | 逗号分隔的板块标签           | 跨境支付CIPS,周期股,大盘股,…,MSCI成份                 |
| `所属主题`   | 逗号分隔的主题标签           | 平安保险持股,不可减持(新规),优先股,…                  |
| `最新分红`   | 最近一期分红方案             | 10派2.49元(含税)                                      |
| `最近解禁`   | 近期限售解禁情况             | 未来三个月无                                          |
| `总质押数`   | 质押股份数量                 | 2438.48万股                                           |
| `占总股本比` | 质押股份占总股本比例         | 0.13%                                                 |
| `减持计划`   | 股东减持计划                 | 无减持计划                                            |
| `财报披露`   | 下一期财报披露安排           | 2026年三季报于10月24日披露                            |

> `内容` 是**已格式化文本**而非裸数值：`2438.48万股`、`0.13%`、`10派2.49元(含税)` 需要数值时应自行解析；无数据时服务端直接给出 `未来三个月无`、`无减持计划` 等文案，不会留空。
> 取值请按栏目名（`data["所属板块"]`），不要按位置索引；`field_list` 须与返回键完全一致，否则该项被忽略。

## 下载文件 `download_file`

### 接口签名

```python
download_file(
    stock_code: str = "",
    down_time: str = "",
    down_type: int = 1,
) -> Optional[str]
```

### 功能说明

根据通达信下载规则获取文件，保存到 `tdxaidata` 包安装目录（`site-packages\tdxaidata`）下的 `data` 子目录，成功返回格式为 `[下载成功]路径:<本地文件绝对路径>`，失败返回 `None`。该接口使用 HTTP 下载，不依赖 TdxAiData 动态库。

### 入参

| 参数 | 是否必选 | 类型 | 说明 |
|:-----|:---------|:-----|:-----|
| `stock_code` | N | `str` | 股票或 ETF 代码，例如 `688318.SH`；为空时使用 `688318.SH`。请求 URL 中自动去除市场后缀。 |
| `down_time` | N | `str` | `down_type=1/5` 时取年份，`down_type=2` 时取日期；支持数字日期、带分隔符日期和完整时间。为空时使用当前时间。 |
| `down_type` | N | `int` | 下载类型，取值 `1` 到 `7`。默认值为 `1`。 |

### `down_type` 取值

| 值 | 内容 | URL 参数 | 本地文件名 |
|:---|:-----|:---------|:-----------|
| `1` | 十大股东 | `type=gp&code=代码&rq=年份` | `holders{Code}_{年份}.json` |
| `2` | ETF 申赎清单 | `type=etf&code=代码&rq=YYYYMMDD` | `etfpcf{Code}_{日期}.json` |
| `3` | 最新舆情信息 | `type=yq` | `sentiment.json` |
| `4` | 综合信息 | `type=gpinfo` | `miscinfo.json` |
| `5` | 经营分析 | `type=mainbusi&code=代码&rq=年份` | `mainbusi{Code}_{年份}.json` |
| `6` | 最近龙虎榜 | `type=lhb&code=代码` | `lhb{Code}.json` |
| `7` | 限售解禁数据 | `type=lockshare&code=代码` | `lockshare{Code}.json` |

### 调用示例

```python
from tdxaidata import tqs

path = tqs.download_file(
    stock_code="688318.SH",
    down_time="2025-01-01",
    down_type=1,
)
print(path)
```

### 返回示例

```text
[下载成功]路径:...\tdxaidata\data\holders688318_2025.json
```

> `tdxaidata` 当前不返回 `tqcenter` 的 RPC JSON 字符串，而是返回带下载成功标记的本地文件路径；调用者可从 `路径:` 后取得实际文件路径并读取文件内容。