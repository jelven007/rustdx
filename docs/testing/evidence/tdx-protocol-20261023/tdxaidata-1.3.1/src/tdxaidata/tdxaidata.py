'''
    Version: 1.3.1
    2026-10-23
'''

from __future__ import annotations

import ctypes
import json
import os
import sys
import threading
from datetime import datetime
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen
from ctypes import POINTER, c_char_p, c_int, c_long
from typing import Any, Callable, Dict, Iterable, List, Optional

try:
    import numpy as np
except ImportError:
    np = None

try:
    import pandas as pd
except ImportError:
    pd = None


# Windows 回调在头文件中声明为 __stdcall，macOS 和 Linux 动态库使用 C 调用约定。
if os.name == "nt":
    AsyncReceiveDataCallBack = ctypes.WINFUNCTYPE(c_int, c_char_p, c_int, c_int)
else:
    AsyncReceiveDataCallBack = ctypes.CFUNCTYPE(c_int, c_char_p, c_int, c_int)


class TqsError(RuntimeError):
    """TdxAiData返回数据处理失败。"""


def resolve_fields(requested_fields: Iterable[str], candidates: Iterable[str]) -> List[str]:
    """将请求字段解析为候选集中的真实字段名，优先精确匹配，其次不区分大小写匹配。

    未命中的请求字段会被忽略；返回顺序与 requested_fields 一致。
    """
    candidates = list(candidates)
    lowered = {}
    for key in candidates:
        lowered.setdefault(str(key).lower(), key)
    result: List[str] = []
    for field in requested_fields:
        if field in candidates:
            key = field
        else:
            key = lowered.get(str(field).lower())
        if key is not None and key not in result:
            result.append(key)
    return result


class _NativeTdxAiData:
    """TdxAiData 原生动态库 ctypes 封装，支持 Windows、macOS 和 Linux。"""

    _instance = None
    _instance_lock = threading.Lock()
    _start_lock = threading.Lock()
    _callback = None

    def __new__(cls, dll_path: Optional[str] = None):
        with cls._instance_lock:
            if cls._instance is None:
                obj = super().__new__(cls)
                obj._initialized = False
                cls._instance = obj
        return cls._instance

    def __init__(self, dll_path: Optional[str] = None):
        if self._initialized:
            return

        if os.name == "nt":
            library_name = "TdxAiData.dll"
        elif sys.platform == "darwin":
            library_name = "libTdxAiData.dylib"
        else:
            library_name = "libTdxAiData.so"
        self._dll_path = dll_path or os.environ.get("TDX_AI_DATA_LIB")
        if not self._dll_path:
            # 发行包内动态库位于包目录的 lib/ 子目录；也兼容源码目录（与模块同目录）。
            base_dir = os.path.dirname(os.path.abspath(__file__))
            for sub_dir in ("", "lib"):
                candidate = os.path.join(base_dir, sub_dir, library_name)
                if os.path.exists(candidate):
                    self._dll_path = candidate
                    break
            if not self._dll_path:
                self._dll_path = os.path.join(base_dir, library_name)
        if not os.path.exists(self._dll_path):
            print("[错误] TdxAiData 动态库文件不存在: %s" % self._dll_path)
            self._dll = None
            self._is_started = False
            self._initialized = True
            return

        try:
            if os.name == "nt":
                # 让同目录的 TdxAsioComm64.dll 等依赖库可被系统加载。
                if hasattr(os, "add_dll_directory"):
                    os.add_dll_directory(os.path.dirname(self._dll_path))
                self._dll = ctypes.WinDLL(self._dll_path)
            else:
                # macOS 和 Linux 依赖库应与动态库同目录；macOS 可通过 DYLD_LIBRARY_PATH、Linux 可通过 LD_LIBRARY_PATH 提前配置。
                self._dll = ctypes.CDLL(
                    self._dll_path,
                    mode=getattr(ctypes, "RTLD_GLOBAL", 0),
                )
        except OSError as exc:
            print("[错误] 加载 TdxAiData 动态库失败: %s" % exc)
            self._dll = None
            self._is_started = False
            self._initialized = True
            return

        self._is_started = False
        self._setup_functions()
        self._initialized = True
    def _setup_functions(self):
        """绑定 TdxAiData.h 中需要使用的函数签名。"""
        dll = self._dll
        dll.start.argtypes = [c_char_p]
        dll.start.restype = c_int
        dll.stop.argtypes = []
        dll.stop.restype = c_int
        dll.FreeResult.argtypes = [c_char_p]
        dll.FreeResult.restype = None

        dll.GetStockInfo_Sub.argtypes = [c_char_p, AsyncReceiveDataCallBack]
        dll.GetStockInfo_Sub.restype = c_int
        dll.GetStockInfo_UnSub.argtypes = []
        dll.GetStockInfo_UnSub.restype = c_int
        dll.GetRealDATsInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetRealDATsInStr.restype = c_int
        dll.GetHISDATsInStr.argtypes = [c_char_p, c_char_p, c_char_p, c_char_p, c_char_p, c_int, POINTER(c_char_p)]
        dll.GetHISDATsInStr.restype = c_int
        dll.GetTickDATsInStr.argtypes = [c_char_p, c_char_p, c_int, c_int, POINTER(c_char_p)]
        dll.GetTickDATsInStr.restype = c_int
        dll.GetStockListInStr.argtypes = [c_char_p, c_int, POINTER(c_char_p)]
        dll.GetStockListInStr.restype = c_int
        dll.GetBlockStocksInStr.argtypes = [c_char_p, c_int, POINTER(c_char_p)]
        dll.GetBlockStocksInStr.restype = c_int
        dll.GetGPBlockInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetGPBlockInStr.restype = c_int
        dll.GetSTOCKInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetSTOCKInStr.restype = c_int
        dll.GetMoreInfoInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetMoreInfoInStr.restype = c_int
        dll.GetTradeCalendarInStr.argtypes = [c_char_p, c_char_p, c_char_p, c_int, POINTER(c_char_p)]
        dll.GetTradeCalendarInStr.restype = c_int
        dll.GetProDataInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetProDataInStr.restype = c_int
        dll.GetCWDATAInStr.argtypes = [c_char_p, c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetCWDATAInStr.restype = c_int
        dll.GetTrackZsETFInfoInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetTrackZsETFInfoInStr.restype = c_int
        dll.GetCBINFOInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetCBINFOInStr.restype = c_int
        dll.GetZDTDataInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetZDTDataInStr.restype = c_int
        dll.GetGbInfoInStr.argtypes = [c_char_p, c_int, c_int, c_char_p, c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetGbInfoInStr.restype = c_int
        dll.GetSpecZSGPInStr.argtypes = [c_char_p, c_int, POINTER(c_char_p)]
        dll.GetSpecZSGPInStr.restype = c_int
        dll.GetMinuteDataInStr.argtypes = [c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetMinuteDataInStr.restype = c_int
        dll.GetTodayMinuteDataInStr.argtypes = [c_char_p, c_int, POINTER(c_char_p)]
        dll.GetTodayMinuteDataInStr.restype = c_int
        dll.GetEXDAYL2InStr.argtypes = [c_char_p, c_int, POINTER(c_char_p)]
        dll.GetEXDAYL2InStr.restype = c_int
        dll.GetTdxDataStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetTdxDataStr.restype = c_int

        # 新增协议：集合竞价 / 批量取数
        dll.GetCallAuctionInStr.argtypes = [c_char_p, c_int, c_char_p, POINTER(c_char_p)]
        dll.GetCallAuctionInStr.restype = c_int
        dll.GetBatchCallAuctionInStr.argtypes = [c_char_p, c_char_p, c_int, c_char_p, POINTER(c_char_p)]
        dll.GetBatchCallAuctionInStr.restype = c_int
        dll.GetBatchRealDATsInStr.argtypes = [c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetBatchRealDATsInStr.restype = c_int
        dll.GetBatchSTOCKInStr.argtypes = [c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetBatchSTOCKInStr.restype = c_int
        dll.GetBatchMoreInfoInStr.argtypes = [c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetBatchMoreInfoInStr.restype = c_int
        dll.GetBatchCBINFOInStr.argtypes = [c_char_p, c_char_p, POINTER(c_char_p)]
        dll.GetBatchCBINFOInStr.restype = c_int
        # 域排序行情（RPC 2023）
        dll.GetBlockSortDataInStr.argtypes = [c_int, c_char_p, c_char_p, c_int, c_char_p, c_int, c_int, POINTER(c_char_p)]
        dll.GetBlockSortDataInStr.restype = c_int
        # 批量公式计算（服务器已有公式；domain 与 stock_list 二选一）
        dll.GetBatchFormulaInStr.argtypes = [
            c_char_p, c_char_p, c_int, c_char_p, c_char_p, c_int, c_int,
            c_char_p, c_char_p, c_int, c_int, c_int, c_char_p, c_int, POINTER(c_char_p),
        ]
        dll.GetBatchFormulaInStr.restype = c_int
        # 临时源码公式计算（RPC 3026；domain 与 stock_list 二选一）
        dll.GetSourceFormulaInStr.argtypes = [
            c_char_p, c_char_p, c_int, c_char_p, c_char_p, c_int, c_int,
            c_char_p, c_char_p, c_int, c_int, c_int, c_char_p, c_int, POINTER(c_char_p),
        ]
        dll.GetSourceFormulaInStr.restype = c_int
        # 市场强弱（RPC 2030；type 目前仅支持 0：沪深京市场）
        dll.GetMarketStrengthInStr.argtypes = [c_int, POINTER(c_char_p)]
        dll.GetMarketStrengthInStr.restype = c_int
        # 个股综合信息（RPC 2025；codestr 需带市场后缀，如 000001.SZ）
        dll.GetStockComprehensiveInfoInStr.argtypes = [c_char_p, POINTER(c_char_p)]
        dll.GetStockComprehensiveInfoInStr.restype = c_int

    @classmethod
    def get_instance(cls):
        return cls()

    @staticmethod
    def _encode(value: str) -> bytes:
        return str(value or "").encode("gb2312")

    def _ensure_started(self):
        """在 DLL 所在目录启动，确保 NewTc.dat 等配置可被正确读取。"""
        if self._is_started:
            return
        if self._dll is None:
            print("[错误] TdxAiData 动态库未加载，跳过启动")
            return
        with self._start_lock:
            if self._is_started:
                return
            if self._dll is None:
                print("[错误] TdxAiData 动态库未加载，跳过启动")
                return
            # 动态库按进程 cwd 查找 TdxAiData.ini / NewTc.dat，且每次数据请求都会重新读取，
            # 所以不能只在启动期间切换目录。这里把库目录直接传给 start()，动态库会记住该目录，
            # 后续请求不再依赖进程 cwd，调用方的工作目录也不会被改动。
            lib_dir = os.path.dirname(os.path.abspath(self._dll_path))
            result = self._dll.start(self._encode(lib_dir))
            if result in (0, 1):
                self._is_started = True
                return

    def _call_with_free(self, func, *args) -> str:
        """调用返回 char** 的接口，并释放 DLL 分配的结果内存。"""
        self._ensure_started()
        if self._dll is None:
            return ""
        result_ptr = c_char_p()
        func(*args, ctypes.byref(result_ptr))
        if not result_ptr.value:
            return ""
        try:
            return result_ptr.value.decode("gb2312", errors="ignore")
        finally:
            self._dll.FreeResult(result_ptr)

    def get_tdx_data(self, json_str: str) -> str:
        """调用通用 GetTdxDataStr 接口。"""
        return self._call_with_free(self._dll.GetTdxDataStr, self._encode(json_str))

    def get_his_dats(self, code, start, end, period, dividend, count):
        return self._call_with_free(
            self._dll.GetHISDATsInStr,
            self._encode(code), self._encode(start), self._encode(end),
            self._encode(period), self._encode(dividend), int(count),
        )

    def get_cw_data(self, code, start, end):
        return self._call_with_free(self._dll.GetCWDATAInStr, self._encode(code), self._encode(start), self._encode(end))

    def get_stock_info(self, code):
        return self._call_with_free(self._dll.GetSTOCKInStr, self._encode(code))

    def get_real_dats(self, code):
        return self._call_with_free(self._dll.GetRealDATsInStr, self._encode(code))

    def get_more_info(self, code):
        return self._call_with_free(self._dll.GetMoreInfoInStr, self._encode(code))

    def get_stock_list(self, market, list_type):
        return self._call_with_free(self._dll.GetStockListInStr, self._encode(market), int(list_type))

    def get_block_from_stock(self, block, list_type):
        return self._call_with_free(self._dll.GetBlockStocksInStr, self._encode(block), int(list_type))

    def get_trade_calendar(self, start, end, market, count):
        return self._call_with_free(self._dll.GetTradeCalendarInStr, self._encode(start), self._encode(end), self._encode(market), int(count))

    def get_pro_data(self, request):
        return self._call_with_free(self._dll.GetProDataInStr, self._encode(request))

    def get_relation(self, code):
        return self._call_with_free(self._dll.GetGPBlockInStr, self._encode(code))

    def get_kzz_info(self, code):
        return self._call_with_free(self._dll.GetCBINFOInStr, self._encode(code))

    def get_trackzs_etf_info(self, code):
        return self._call_with_free(self._dll.GetTrackZsETFInfoInStr, self._encode(code))

    def get_zdt_data(self, codes):
        """通过原生 GetZDTDataInStr 获取涨跌停数据。"""
        return self._call_with_free(self._dll.GetZDTDataInStr, self._encode(codes))

    def get_gb_info(self, code, gb_type, count, date_list, start_date, end_date):
        """通过原生 GetGbInfoInStr 获取股本数据。"""
        return self._call_with_free(
            self._dll.GetGbInfoInStr,
            self._encode(code), int(gb_type), int(count), self._encode(date_list),
            self._encode(start_date), self._encode(end_date),
        )

    def get_zzgz_stocklist(self, index_code, list_type):
        """通过原生 GetSpecZSGPInStr 获取指数成分股。"""
        return self._call_with_free(
            self._dll.GetSpecZSGPInStr,
            self._encode(index_code), int(list_type),
        )

    def get_minute_data(self, codestr, date):
        """通过原生 GetMinuteDataInStr 获取指定日期分时数据。"""
        return self._call_with_free(
            self._dll.GetMinuteDataInStr,
            self._encode(codestr), self._encode(date),
        )

    def get_today_minute_data(self, codestr, minute):
        """通过原生 GetTodayMinuteDataInStr 获取当日分时数据。"""
        return self._call_with_free(
            self._dll.GetTodayMinuteDataInStr,
            self._encode(codestr), int(minute),
        )

    def get_exday_data(self, codestr, count):
        """通过原生 GetEXDAYL2InStr 获取日线统计数据。"""
        return self._call_with_free(
            self._dll.GetEXDAYL2InStr,
            self._encode(codestr), int(count),
        )
    def get_tick_data(self, code, date, startxh, wantnum):
        return self._call_with_free(self._dll.GetTickDATsInStr, self._encode(code), self._encode(date), int(startxh), int(wantnum))

    def get_call_auction(self, code, type, date):
        """通过原生 GetCallAuctionInStr 获取集合竞价数据（单只）。"""
        return self._call_with_free(self._dll.GetCallAuctionInStr, self._encode(code), int(type), self._encode(date))

    def get_call_auction_batch(self, codes, field_list, type, date):
        """通过原生 GetBatchCallAuctionInStr 批量获取集合竞价数据。"""
        return self._call_with_free(
            self._dll.GetBatchCallAuctionInStr, self._encode(codes), self._encode(field_list), int(type),
            self._encode(date)
        )

    def get_market_snapshot_batch(self, codes, field_list):
        """通过原生 GetBatchRealDATsInStr 批量获取实时行情快照。"""
        return self._call_with_free(
            self._dll.GetBatchRealDATsInStr, self._encode(codes), self._encode(field_list)
        )

    def get_stock_info_batch(self, codes, field_list):
        """通过原生 GetBatchSTOCKInStr 批量获取股票基本财务数据。"""
        return self._call_with_free(
            self._dll.GetBatchSTOCKInStr, self._encode(codes), self._encode(field_list)
        )

    def get_more_info_batch(self, codes, field_list):
        """通过原生 GetBatchMoreInfoInStr 批量获取股票扩展数据。"""
        return self._call_with_free(
            self._dll.GetBatchMoreInfoInStr, self._encode(codes), self._encode(field_list)
        )

    def get_kzz_info_batch(self, codes, field_list):
        """通过原生 GetBatchCBINFOInStr 批量获取可转债基础信息。"""
        return self._call_with_free(
            self._dll.GetBatchCBINFOInStr, self._encode(codes), self._encode(field_list)
        )

    def get_domain_sorthq(self, domain, colname, sorttype, wantcol, startxh, wantnum, data_source=0):
        """通过原生 GetBlockSortDataInStr 获取域排序行情。"""
        return self._call_with_free(
            self._dll.GetBlockSortDataInStr,
            int(data_source), self._encode(str(domain)), self._encode(colname), int(sorttype),
            self._encode(wantcol), int(startxh), int(wantnum),
        )

    def get_batch_formula(self, stock_list, formula_name, formula_type, formula_arg, stock_period,
                          dividend_type, count, start_time, end_time, return_count, return_date,
                          xsflag, domain="", data_source=0):
        """通过原生 GetBatchFormulaInStr 批量计算服务器已有公式。

        domain 为空串时按 stock_list 取数，为非空数字串时按该市场分类自动取股票列表；
        data_source 为数据源，0 沪深京、1 扩展行情。
        """
        return self._call_with_free(
            self._dll.GetBatchFormulaInStr,
            self._encode(stock_list), self._encode(formula_name), int(formula_type),
            self._encode(formula_arg), self._encode(stock_period), int(dividend_type),
            int(count), self._encode(start_time), self._encode(end_time),
            int(return_count), int(return_date), int(xsflag), self._encode(domain),
            int(data_source),
        )

    def get_source_formula(self, stock_list, formula_src, formula_type, formula_arg, stock_period,
                           dividend_type, count, start_time, end_time, return_count, return_date,
                           xsflag, domain="", data_source=0):
        """通过原生 GetSourceFormulaInStr 执行临时源码公式。

        domain 为空串时按 stock_list 取数，为非空数字串时按该市场分类自动取股票列表；
        data_source 为数据源，0 沪深京、1 扩展行情。
        """
        return self._call_with_free(
            self._dll.GetSourceFormulaInStr,
            self._encode(stock_list), self._encode(formula_src), int(formula_type),
            self._encode(formula_arg), self._encode(stock_period), int(dividend_type),
            int(count), self._encode(start_time), self._encode(end_time),
            int(return_count), int(return_date), int(xsflag), self._encode(domain),
            int(data_source),
        )

    def get_market_strength(self, type):
        """通过原生 GetMarketStrengthInStr 获取市场强弱统计。"""
        return self._call_with_free(self._dll.GetMarketStrengthInStr, int(type))

    def get_misc_data(self, codestr):
        """通过原生 GetStockComprehensiveInfoInStr 获取个股综合信息。"""
        return self._call_with_free(self._dll.GetStockComprehensiveInfoInStr, self._encode(codestr))

    def subscribe(self, codes: str, callback: Callable[[str, int, int], int]):
        self._ensure_started()

        def wrapped(data, datanum, datatype):
            try:
                text = data.decode("gb2312", errors="ignore") if data else ""
                return int(callback(text, datanum, datatype) or 0)
            except Exception:
                return 0

        self._callback = AsyncReceiveDataCallBack(wrapped)
        self._dll.GetStockInfo_Sub(self._encode(codes), self._callback)

    def unsubscribe(self):
        self._ensure_started()
        self._dll.GetStockInfo_UnSub()
        self._callback = None


class tqs:

    _sub_lock = threading.RLock()
    _sub_codes: List[str] = []
    _sub_callbacks: Dict[str, Callable[[str], Any]] = {}

    _period_map = {
        "1m": "1m", "5m": "5m", "10m": "10m", "15m": "15m", "30m": "30m",
        "1h": "1h", "1d": "1d", "45d": "45d", "1w": "1w", "1mon": "1mon",
        "1q": "1q", "1y": "1y",
    }

    # 未指定 field_list 时，K 线默认返回的字段顺序
    _standard_kline_fields = [
        "Open", "Low", "Close", "High", "Volume", "Amount", "VolInStock", "ForwardFactor",
    ]

    # 域排序行情固定返回的栏目顺序
    _sorthq_base_fields = ["Code", "Name", "Now", "Close", "ZAF", "Volume"]

    @classmethod
    def _tdx(cls):
        return _NativeTdxAiData.get_instance()

    @staticmethod
    def _items(value: Optional[Iterable[str]]) -> List[str]:
        if value is None:
            return []
        if isinstance(value, str):
            return [value] if value else []
        return [str(item) for item in value if str(item)]

    @staticmethod
    def _data_source(value: Any) -> int:
        """校验数据源：0 沪深京、1 扩展行情；非法时打印错误并返回 -1。"""
        try:
            source = int(value)
        except (TypeError, ValueError):
            source = -1
        if source not in (0, 1):
            print("[错误] data_source 取值 0（沪深京）或 1（扩展行情）")
            return -1
        return source

    @staticmethod
    def _code(value: str) -> str:
        """保留 TdxAiData 原生接口要求的市场后缀。"""
        return str(value or "").strip()

    @staticmethod
    def _time(value: str) -> str:
        if not value:
            return ""
        text = str(value).strip()
        if len(text) == 8 and text.isdigit():
            return "%s-%s-%s 00:00:00" % (text[:4], text[4:6], text[6:8])
        if len(text) == 14 and text.isdigit():
            return "%s-%s-%s %s:%s:%s" % (text[:4], text[4:6], text[6:8], text[8:10], text[10:12], text[12:14])
        if len(text) == 10 and text[4] == "-" and text[7] == "-":
            return text + " 00:00:00"
        return text

    @staticmethod
    def _loads(raw: str) -> Any:
        if not raw:
            print("TdxAiData 返回空数据")
            return None
        try:
            return json.loads(raw)
        except (TypeError, json.JSONDecodeError) as exc:
            print("TdxAiData 返回无效 JSON: %s" % raw[:200])
            return None

    @classmethod
    def _clean(cls, raw: str) -> Any:
        value = cls._loads(raw)
        if isinstance(value, dict):
            error_id = value.get("ErrorId")
            if error_id is not None and str(error_id) != "0":
                print("[错误] %s" % str(value.get("Error", "未知错误")))
                return None
            if "error" in value:
                print("[错误] %s (error_code=%s)" % (value.get("error"), value.get("error_code")))
                return None
            return {key: item for key, item in value.items() if key not in ("ErrorId", "Error", "ResultSetNum")}
        return value

    @classmethod
    def _generic_data(cls, payload: Dict[str, Any]) -> Any:
        """通过本文件内置的 get_tdx_data 调用通用协议。"""
        raw = cls._tdx().get_tdx_data(json.dumps(payload, ensure_ascii=False))
        value = cls._clean(raw)
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value

    @staticmethod
    def _frame(value: Any) -> Any:
        """将 {字段: 等长数组} 的字典转为 DataFrame，标量字段不参与转换。"""
        if pd is None or not isinstance(value, dict):
            return value
        arrays = {key: item for key, item in value.items() if isinstance(item, list)}
        lengths = [len(item) for item in arrays.values()]
        if lengths and len(set(lengths)) == 1:
            return pd.DataFrame(arrays)
        return value

    @staticmethod
    def _match_key(source: Any, field: str) -> Optional[str]:
        """在 source 的键中按精确、再按不区分大小写查找 field，返回命中的真实键名。"""
        if isinstance(source, dict):
            keys = list(source.keys())
        elif isinstance(source, (str, bytes)):
            return None
        else:
            try:
                keys = list(source)
            except TypeError:
                return None
        if field in keys:
            return field
        lowered = field.lower()
        for key in keys:
            if str(key).lower() == lowered:
                return key
        return None

    @staticmethod
    def _filter(value: Any, field_list: Optional[Iterable[str]]) -> Any:
        """按 field_list 筛选字段，优先精确匹配，其次不区分大小写匹配。"""
        fields = tqs._items(field_list)
        if not fields or not isinstance(value, dict):
            return value
        result = {}
        for field in fields:
            key = tqs._match_key(value, field)
            if key is not None:
                result[key] = value[key]
        return result

    @staticmethod
    def _resolve_fields(requested_fields: List[str], candidates: Iterable[str], default_fields: List[str]) -> List[str]:
        """将请求字段解析为候选集中的真实字段名；未指定时用默认字段。"""
        if not requested_fields:
            fields = []
            for field in default_fields:
                key = tqs._match_key(candidates, field)
                if key is not None and key not in fields:
                    fields.append(key)
            return fields
        return resolve_fields(requested_fields, candidates)

    @staticmethod
    def _join(values: Optional[Iterable[str]]) -> str:
        """将代码或字段列表转换为接口要求的逗号分隔字符串。"""
        return ",".join(tqs._items(values))

    @staticmethod
    def _batch_records(value: Any, stocks: List[str]) -> Dict[str, Any]:
        """将批量接口的并行数组整理为 {代码: {字段: 值}} 结构。"""
        if not isinstance(value, dict):
            return {}
        rows = {key: item for key, item in value.items() if key != "TotalNum"}
        result: Dict[str, Any] = {}
        for index, stock in enumerate(stocks):
            record: Dict[str, Any] = {}
            for key, item in rows.items():
                if isinstance(item, list):
                    record[key] = item[index] if index < len(item) else None
                else:
                    record[key] = item
            result[stock] = record
        return result

    @staticmethod
    def _records_frame(records: Any, return_df: bool) -> Any:
        """return_df=True 且 pandas 可用时，把 {代码: {字段}} 转为以代码为索引的 DataFrame。"""
        if not return_df:
            return records
        if pd is None:
            print("[提示] 未安装 pandas，return_df 不生效，仍返回字典")
            return records
        if not isinstance(records, dict) or not records:
            return pd.DataFrame()
        return pd.DataFrame.from_dict(records, orient="index")

    @classmethod
    def _sorthq_frame(cls, value: Any) -> Any:
        """按固定栏目顺序整理域排序行情，并剔除数量不足时的补位空记录。"""
        if not isinstance(value, dict):
            return pd.DataFrame() if pd is not None else {}
        arrays = {key: item for key, item in value.items() if isinstance(item, list)}
        if not arrays:
            return pd.DataFrame() if pd is not None else {}
        columns = [field for field in cls._sorthq_base_fields if field in arrays]
        columns += [key for key in arrays if key not in columns]
        length = min(len(arrays[key]) for key in columns)
        rows = []
        for index in range(length):
            record = {key: arrays[key][index] for key in columns}
            if not str(record.get("Code") or "").split(".")[0].strip():
                continue
            rows.append(record)
        if pd is None:
            return {key: [row[key] for row in rows] for key in columns}
        return pd.DataFrame(rows, columns=columns)

    @classmethod
    def _batch_query(cls, method: str, stock_list: List[str], field_list: List[str],
                     return_df: bool = False) -> Any:
        """调用批量接口并按代码整理返回结果；return_df=True 时返回 DataFrame。"""
        stocks = cls._items(stock_list)
        if not stocks:
            print("[错误] stock_list 不能为空")
            return cls._records_frame({}, return_df)
        raw = getattr(cls._tdx(), method)(cls._join(stocks), cls._join(field_list))
        return cls._records_frame(cls._batch_records(cls._clean(raw), stocks), return_df)

    @staticmethod
    def _market_code(code: str) -> int:
        text = str(code or "").strip().upper()
        if ".SH" in text or ".SS" in text:
            return 1
        if ".BJ" in text:
            return 2
        if ".SZ" in text:
            return 0
        plain = text.split(".", 1)[0]
        if plain.startswith(("6", "68", "69")):
            return 1
        if plain.startswith(("0", "3")):
            return 0
        if plain.startswith(("4", "8")):
            return 2
        print("[错误] 无法从股票代码推导市场代码: %s" % code)
        return -1

    @classmethod
    def _zdt_codes(cls, stock_list: Iterable[Any]) -> str:
        """将股票代码列表转换为 GetZDTDataInStr 使用的分号分隔字符串。"""
        values = list(stock_list or [])
        if not values:
            print("[错误] stock_list 不能为空")
            return ""

        result = []
        suffix_map = {0: ".SZ", 1: ".SH", 2: ".BJ"}
        for item in values:
            if isinstance(item, dict):
                code = str(item.get("code", "")).strip().upper()
                if not code or "setcode" not in item:
                    print("[错误] stock_list 元素必须包含 code 和 setcode")
                    return ""
                if "." not in code:
                    suffix = suffix_map.get(int(item["setcode"]))
                    if not suffix:
                        print("[错误] 不支持的 setcode: %s" % item["setcode"])
                        return ""
                    code += suffix
            else:
                code = str(item or "").strip().upper()
                if not code:
                    print("[错误] stock_list 不能包含空股票代码")
                    return ""
            result.append(code)
        return ";".join(result)
    @classmethod
    def _format_pro_result(cls, request_type: int, value: Any) -> Any:
        """将财务时序数据转换为与 tq.get_financial_data 一致的 DataFrame。"""
        if request_type != 1 or pd is None or not isinstance(value, dict):
            return value

        list_lengths = [len(item) for item in value.values() if isinstance(item, list)]
        if list_lengths and len(set(list_lengths)) == 1:
            return pd.DataFrame(value)
        return value

    @classmethod
    def _pro(cls, request_type: int, stock_list: List[str], field_list: List[str], **extra: Any) -> Dict[str, Any]:
        """通过 GetProDataInStr 查询专业数据，不再依赖市场代码推导。"""
        stocks = cls._items(stock_list)
        fields = cls._items(field_list)
        if not stocks:
            print("[错误] stock_list 不能为空")
            return {}
        if not fields:
            print("[错误] field_list 不能为空")
            return {}

        result: Dict[str, Any] = {}
        for stock in stocks:
            code = cls._code(stock)
            payload: Dict[str, Any] = {
                "type": int(request_type),
                "code": code,
                "table_list": fields,
            }
            if request_type in (1, 3, 5, 7):
                payload["start_time"] = extra.get("start_time", "")
                payload["end_time"] = extra.get("end_time", "")
            elif request_type in (2, 4, 6, 8):
                payload["year"] = int(extra.get("year", 0))
                payload["mmdd"] = int(extra.get("mmdd", 0))
            if request_type == 1:
                payload["report_type"] = extra.get("report_type", "report_time")

            request = json.dumps(payload, ensure_ascii=False)
            value = cls._clean(cls._tdx().get_pro_data(request))
            if isinstance(value, list) and len(value) == 1 and isinstance(value[0], dict):
                value = value[0]
            if isinstance(value, dict) and "Value" in value:
                value = value["Value"]
            result[stock] = cls._format_pro_result(request_type, value)
        return result

    @classmethod
    def get_market_data(cls, field_list: List[str] = [], stock_list: List[str] = [], period: str = "",
                        start_time: str = "", end_time: str = "", count: int = -1,
                        dividend_type: Optional[str] = None, fill_data: bool = True) -> Dict[str, Any]:
        """获取 K 线"""
        stocks = cls._items(stock_list)
        if not stocks or period.lower() not in cls._period_map:
            print("[错误] stock_list 不能为空，period 必须是合法 K 线周期")
            return {}
        requested_fields = cls._items(field_list)
        # tqcenter 在 count 模式下只使用 end_time，忽略 start_time
        request_start = "" if count > 0 else cls._time(start_time)
        request_end = cls._time(end_time)
        all_data: Dict[str, Dict[str, Any]] = {}
        for stock in stocks:
            value = cls._clean(cls._tdx().get_his_dats(
                cls._code(stock), request_start, request_end, cls._period_map[period.lower()],
                dividend_type or "none", count,
            ))
            data = value.get("Value", value) if isinstance(value, dict) else value
            if not isinstance(data, dict):
                continue
            all_data[stock] = data
        # 本股票集的全部可用字段（去掉由索引承载的 Date/Time 与元信息）
        available_fields = set().union(*(d.keys() for d in all_data.values())) - {"Date", "Time", "ErrorId", "Value"} if all_data else set()
        wanted_fields = cls._resolve_fields(requested_fields, available_fields, cls._standard_kline_fields)
        if pd is None or np is None:
            return {field: {stock: list(data.get(field, []) or []) for stock, data in all_data.items()} for field in wanted_fields}
        timestamps = set()
        for data in all_data.values():
            dates = data.get("Date", []) or []
            times = data.get("Time", []) or []
            for index, date in enumerate(dates):
                time_value = times[index] if index < len(times) else "0"
                time_text = str(time_value)
                if time_text not in ("0", "000000", "0000"):
                    timestamps.add("%s%06d" % (date, int(time_value)))
                else:
                    timestamps.add(str(date))
        if not timestamps:
            return {}
        sorted_timestamps = sorted(timestamps)
        time_index = pd.DatetimeIndex([pd.to_datetime(item, format="%Y%m%d%H%M%S" if len(item) > 8 else "%Y%m%d", errors="coerce") for item in sorted_timestamps])
        timestamp_index = {item: index for index, item in enumerate(sorted_timestamps)}
        n_time = len(time_index)
        # 每只股票各行在矩阵中的行号（各字段复用，避免重复解析时间戳）
        stock_pos: Dict[str, Any] = {}
        for stock in stocks:
            data = all_data.get(stock)
            if not isinstance(data, dict):
                continue
            dates = data.get("Date", []) or []
            if not dates:
                continue
            times = data.get("Time", []) or []
            n_rec = len(dates)
            pos = np.full(n_rec, -1, dtype=np.int64)
            for i in range(n_rec):
                time_value = times[i] if i < len(times) else "0"
                time_text = str(time_value)
                ts = "%s%06d" % (dates[i], int(time_value)) if time_text not in ("0", "000000", "0000") else str(dates[i])
                row_index = timestamp_index.get(ts)
                if row_index is not None:
                    pos[i] = row_index
            stock_pos[stock] = (data, pos)
        result: Dict[str, Any] = {}
        for field in wanted_fields:
            data_arr = np.full((n_time, len(stocks)), np.nan, dtype=np.float64)
            for s_idx, stock in enumerate(stocks):
                plan = stock_pos.get(stock)
                if plan is None:
                    continue
                data, pos = plan
                values = data.get(field)
                if not values:
                    continue
                n_use = min(len(pos), len(values))
                if n_use <= 0:
                    continue
                pos_use = pos[:n_use]
                valid = pos_use >= 0
                if not valid.any():
                    continue
                # 一次完成空值/非数值->NaN 与数值转换
                conv = pd.to_numeric(pd.Series(list(values[:n_use]), dtype=object),
                                     errors="coerce").to_numpy(dtype=np.float64, na_value=np.nan)
                keep = valid & ~np.isnan(conv)
                if not keep.any():
                    continue
                data_arr[pos_use[keep], s_idx] = conv[keep]
                if fill_data:
                    col = data_arr[:, s_idx]
                    mask = ~np.isnan(col)
                    if mask.any():
                        idx_arr = np.where(mask, np.arange(len(col)), 0)
                        np.maximum.accumulate(idx_arr, out=idx_arr)
                        col[:] = col[idx_arr]
            result[field] = pd.DataFrame(data_arr, index=time_index, columns=stocks)
        return result

    @classmethod
    def get_divid_factors(cls, stock_code: str, start_time: str = "", end_time: str = "") -> Any:
        """获取除权除息数据并转换为 tqcenter 的 DataFrame 结构。"""
        value = cls._clean(cls._tdx().get_cw_data(cls._code(stock_code), cls._time(start_time), cls._time(end_time)))
        if not isinstance(value, dict):
            return pd.DataFrame() if pd is not None else []
        dates = value.get("Date", [])
        types = value.get("Type", [])
        values = value.get("Value", [])
        start_date = cls._time(start_time)[:10].replace("-", "")
        end_date = cls._time(end_time)[:10].replace("-", "")
        rows = []
        for index, item in enumerate(values):
            date = str(dates[index] if index < len(dates) else "").strip()
            date_key = "".join(char for char in date if char.isdigit())[:8]
            # 底层接口可能忽略日期范围，Python 层按闭区间再次筛选
            if len(date_key) != 8 or (start_date and date_key < start_date) or (end_date and date_key > end_date):
                continue
            item = list(item or [])
            item += ["0"] * (4 - len(item))
            rows.append({
                "Date": date,
                "Type": types[index] if index < len(types) else "",
                "Bonus": float(item[0] or 0),
                "AllotPrice": float(item[1] or 0),
                "ShareBonus": float(item[2] or 0),
                "Allotment": float(item[3] or 0),
            })
        if pd is None:
            return rows
        frame = pd.DataFrame(rows, columns=["Date", "Type", "Bonus", "AllotPrice", "ShareBonus", "Allotment"])
        if not frame.empty:
            frame["Date"] = pd.to_datetime(frame["Date"], format="%Y%m%d", errors="coerce")
            frame = frame.set_index("Date")
        return frame
    @classmethod
    def get_stock_info(cls, stock_code: str, field_list: List[str] = []) -> Any:
        return cls._filter(cls._clean(cls._tdx().get_stock_info(cls._code(stock_code))), field_list)

    @classmethod
    def get_market_snapshot(cls, stock_code: str, field_list: List[str] = []) -> Any:
        return cls._filter(cls._clean(cls._tdx().get_real_dats(cls._code(stock_code))), field_list)

    @classmethod
    def get_more_info(cls, stock_code: str = "", field_list: List[str] = []) -> Any:
        """获取扩展证券信息，解开 Value 后按 field_list 筛选返回字段。"""
        value = cls._clean(cls._tdx().get_more_info(cls._code(stock_code)))
        if isinstance(value, dict) and "Value" in value:
            value = value["Value"]
        return cls._filter(value, field_list)


    @classmethod
    def get_call_auction(cls, stock_code: str, type: int = 1, date: str = "") -> Any:
        """获取单只证券的集合竞价数据（虚拟匹配价/量等时序）；date 为空时取当日。"""
        value = cls._clean(cls._tdx().get_call_auction(cls._code(stock_code), type, date))
        if isinstance(value, dict):
            value.pop("InOutFlag", None)
        return cls._frame(value)

    @classmethod
    def get_call_auction_batch(cls, stock_list: List[str] = [], field_list: List[str] = [],
                               type: int = 1, date: str = "", return_df: bool = False) -> Any:
        """批量获取集合竞价数据，返回 {代码: {字段...}}；date 为空时取当日，return_df=True 时返回 DataFrame。"""
        stocks = cls._items(stock_list)
        if not stocks:
            print("[错误] stock_list 不能为空")
            return cls._records_frame({}, return_df)
        raw = cls._tdx().get_call_auction_batch(cls._join(stocks), cls._join(field_list), type, date)
        value = cls._clean(raw)
        if isinstance(value, dict) and isinstance(value.get("Value"), list):
            items = value["Value"]
            records = {stock: (items[index] if index < len(items) else None)
                       for index, stock in enumerate(stocks)}
        else:
            records = cls._batch_records(value, stocks)
        for record in records.values():
            if isinstance(record, dict):
                record.pop("InOutFlag", None)
        return cls._records_frame(records, return_df)

    @classmethod
    def get_market_snapshot_batch(cls, stock_list: List[str] = [], field_list: List[str] = [],
                                  return_df: bool = False) -> Any:
        """批量获取实时行情快照，返回 {代码: {字段...}}；return_df=True 时返回 DataFrame。"""
        return cls._batch_query("get_market_snapshot_batch", stock_list, field_list, return_df)

    @classmethod
    def get_stock_info_batch(cls, stock_list: List[str] = [], field_list: List[str] = [],
                             return_df: bool = False) -> Any:
        """批量获取股票基本财务数据，返回 {代码: {字段...}}；return_df=True 时返回 DataFrame。"""
        return cls._batch_query("get_stock_info_batch", stock_list, field_list, return_df)

    @classmethod
    def get_more_info_batch(cls, stock_list: List[str] = [], field_list: List[str] = [],
                            return_df: bool = False) -> Any:
        """批量获取股票扩展数据，返回 {代码: {字段...}}；return_df=True 时返回 DataFrame。"""
        return cls._batch_query("get_more_info_batch", stock_list, field_list, return_df)

    @classmethod
    def get_kzz_info_batch(cls, stock_list: List[str] = [], field_list: List[str] = [],
                           return_df: bool = False) -> Any:
        """批量获取可转债基础信息，返回 {代码: {字段...}}；return_df=True 时返回 DataFrame。"""
        return cls._batch_query("get_kzz_info_batch", stock_list, field_list, return_df)

    @classmethod
    def get_domain_sorthq(cls, domain: int = 6, colname: str = "ZAF", sorttype: int = 1,
                          wantcol: List[str] = [], startxh: int = 0,
                          wantnum: int = 100, data_source: int = 0) -> Any:
        """获取集合排名行情"""
        try:
            query_domain = int(domain)
        except (TypeError, ValueError):
            print("[错误] domain 必须是非负整数")
            return None
        if query_domain < 0:
            print("[错误] domain 必须是非负整数")
            return None

        column = str(colname or "").strip()
        if not column:
            print("[错误] colname 不能为空")
            return None

        try:
            order = int(sorttype)
        except (TypeError, ValueError):
            order = -1
        if order not in (0, 1, 2):
            print("[错误] sorttype 取值 0（不排序）、1（降序）、2（升序）")
            return None

        fields = cls._items(wantcol)

        try:
            offset = int(startxh)
            size = int(wantnum)
        except (TypeError, ValueError):
            print("[错误] startxh 和 wantnum 必须是整数")
            return None
        if offset < 0 or not 1 <= size <= 200:
            print("[错误] startxh 不能为负，wantnum 取值 1~200")
            return None

        try:
            source = int(data_source)
        except (TypeError, ValueError):
            source = -1
        if source not in (0, 1):
            print("[错误] data_source 取值 0（沪深京）或 1（扩展主站）")
            return None

        value = cls._clean(cls._tdx().get_domain_sorthq(
            query_domain, column, order, cls._join(fields), offset, size, source))
        if not isinstance(value, dict):
            return cls._sorthq_frame({})
        return cls._sorthq_frame(value)


    @classmethod
    def _formula_batch(cls, formula_type: int, formula_name: str = "", formula_arg: str = "",
                       return_count: int = 1, return_date: bool = False, xsflag: int = -1,
                       stock_list: List[str] = [], stock_period: str = "1d",
                       start_time: str = "", end_time: str = "", count: int = 0,
                       dividend_type: int = 0, domain: int = 0, data_source: int = 0) -> Any:
        """批量执行服务器已有公式，返回 {证券代码: {输出线名: [...], Time: [...]}}。
        """
        try:
            market = int(domain)
        except (TypeError, ValueError):
            print("[错误] domain 必须是整数")
            return {}
        if market < 0:
            print("[错误] domain 不能为负")
            return {}
        stocks = cls._items(stock_list)
        if market:
            # domain 非 0 时由市场分类决定股票池，stock_list 不参与
            stocks = []
        elif not stocks:
            print("[错误] domain 为 0 时必须指定 stock_list")
            return {}
        name = str(formula_name or "").strip()
        if not name:
            print("[错误] formula_name 不能为空")
            return {}
        if not str(stock_period or "").strip():
            print("[错误] stock_period 不能为空")
            return {}
        if count in (0, -1) and not (str(start_time or "").strip() and str(end_time or "").strip()):
            print("[错误] count 为 0 或 -1 时必须同时指定 start_time 与 end_time")
            return {}
        if int(count) > 24000:
            print("[错误] count 不能大于 24000（-1 表示加载全部，实际超过 24000 条会报该股票错误）")
            return {}
        datasrc = cls._data_source(data_source)
        if datasrc < 0:
            return {}
        raw = cls._tdx().get_batch_formula(
            cls._join(stocks), name, int(formula_type), str(formula_arg or ""),
            str(stock_period).strip(), int(dividend_type), int(count),
            cls._time(start_time), cls._time(end_time), int(return_count),
            1 if return_date else 0, int(xsflag), str(market) if market else "", datasrc,
        )
        value = cls._clean(raw)
        if not isinstance(value, dict):
            return {}
        if value.get("cacheid"):
            print("[错误] 本次结果超过 64KiB，服务器返回缓存分页信息（pagenum=%s），动态库未提供取页入参，请缩小股票范围"
                  % value.get("pagenum"))
            return {}
        items = value.get("Value")
        if not isinstance(items, list):
            return {}
        result: Dict[str, Any] = {}
        for index, item in enumerate(items):
            if not isinstance(item, dict):
                continue
            record = {key: val for key, val in item.items() if key != "Index"}
            code = str(record.pop("Code", "") or (stocks[index] if index < len(stocks) else ""))
            result[code] = record
        return result

    @classmethod
    def formula_process_mul_zb(cls, formula_name: str = "", formula_arg: str = "",
                               return_count: int = 1, return_date: bool = False, xsflag: int = -1,
                               stock_list: List[str] = [], stock_period: str = "1d",
                               start_time: str = "", end_time: str = "", count: int = 0,
                               dividend_type: int = 0, domain: int = 0,
                               data_source: int = 0) -> Any:
        """批量执行服务器已有指标公式（formula_type=0）；domain 非 0 时按市场分类取股票池，
        data_source 非 0 时按扩展行情取数。"""
        return cls._formula_batch(
            0, formula_name=formula_name, formula_arg=formula_arg, return_count=return_count,
            return_date=return_date, xsflag=xsflag, stock_list=stock_list,
            stock_period=stock_period, start_time=start_time, end_time=end_time,
            count=count, dividend_type=dividend_type, domain=domain, data_source=data_source,
        )

    @classmethod
    def formula_process_mul_xg(cls, formula_name: str = "", formula_arg: str = "",
                               return_count: int = 1, return_date: bool = False,
                               stock_list: List[str] = [], stock_period: str = "1d",
                               start_time: str = "", end_time: str = "", count: int = 0,
                               dividend_type: int = 0, domain: int = 0,
                               data_source: int = 0) -> Any:
        """批量执行服务器已有条件选股公式（formula_type=1）；domain 非 0 时按市场分类取股票池，
        data_source 非 0 时按扩展行情取数。"""
        return cls._formula_batch(
            1, formula_name=formula_name, formula_arg=formula_arg, return_count=return_count,
            return_date=return_date, stock_list=stock_list, stock_period=stock_period,
            start_time=start_time, end_time=end_time, count=count, dividend_type=dividend_type,
            domain=domain, data_source=data_source,
        )

    @classmethod
    def formula_process_mul_exp(cls, formula_name: str = "", formula_arg: str = "",
                                return_count: int = 1, return_date: bool = False, xsflag: int = -1,
                                stock_list: List[str] = [], stock_period: str = "1d",
                                start_time: str = "", end_time: str = "", count: int = 0,
                                dividend_type: int = 0, domain: int = 0,
                                data_source: int = 0) -> Any:
        """批量执行服务器已有专家系统公式（formula_type=2）；domain 非 0 时按市场分类取股票池，
        data_source 非 0 时按扩展行情取数。"""
        return cls._formula_batch(
            2, formula_name=formula_name, formula_arg=formula_arg, return_count=return_count,
            return_date=return_date, xsflag=xsflag, stock_list=stock_list,
            stock_period=stock_period, start_time=start_time, end_time=end_time,
            count=count, dividend_type=dividend_type, domain=domain, data_source=data_source,
        )

    @classmethod
    def _formula_source(cls, formula_type: int, formula_src: str = "", stock_list: List[str] = [],
                        stock_period: str = "1d", start_time: str = "", end_time: str = "",
                        count: int = 0, return_count: int = 1, return_date: bool = False,
                        xsflag: int = -1, dividend_type: int = 0, domain: int = 0,
                        data_source: int = 0) -> Any:
        """按临时公式源码批量计算，返回 {证券代码: {输出线名: [...], Time: [...]}}。
        """
        try:
            market = int(domain)
        except (TypeError, ValueError):
            print("[错误] domain 必须是整数")
            return {}
        if market < 0:
            print("[错误] domain 不能为负")
            return {}
        source = str(formula_src or "").strip()
        if not source:
            print("[错误] formula_src 不能为空")
            return {}
        stocks = cls._items(stock_list)
        if market:
            # domain 非 0 时由市场分类决定股票池，stock_list 不参与
            stocks = []
        elif not stocks:
            print("[错误] domain 为 0 时必须指定 stock_list")
            return {}
        if not str(stock_period or "").strip():
            print("[错误] stock_period 不能为空")
            return {}
        if count in (0, -1) and not (str(start_time or "").strip() and str(end_time or "").strip()):
            print("[错误] count 为 0 或 -1 时必须同时指定 start_time 与 end_time")
            return {}
        if int(count) > 24000:
            print("[错误] count 不能大于 24000（-1 表示加载全部，实际超过 24000 条会报该股票错误）")
            return {}
        datasrc = cls._data_source(data_source)
        if datasrc < 0:
            return {}
        raw = cls._tdx().get_source_formula(
            cls._join(stocks), source, int(formula_type), "", str(stock_period).strip(),
            int(dividend_type), int(count), cls._time(start_time), cls._time(end_time),
            int(return_count), 1 if return_date else 0, int(xsflag),
            str(market) if market else "", datasrc,
        )
        value = cls._clean(raw)
        if not isinstance(value, dict):
            return {}
        if value.get("cacheid"):
            print("[错误] 本次结果超过 64KiB，服务器返回缓存分页信息（pagenum=%s），动态库未提供取页入参，请缩小股票范围"
                  % value.get("pagenum"))
            return {}
        items = value.get("Value")
        if not isinstance(items, list):
            return {}
        result: Dict[str, Any] = {}
        for index, item in enumerate(items):
            if not isinstance(item, dict):
                continue
            record = {key: val for key, val in item.items() if key != "Index"}
            code = str(record.pop("Code", "") or (stocks[index] if index < len(stocks) else ""))
            result[code] = record
        return result

    @classmethod
    def formula_by_src(cls, formula_src: str = "", stock_list: List[str] = [],
                       stock_period: str = "1d", start_time: str = "", end_time: str = "",
                       count: int = 0, return_count: int = 1, return_date: bool = False,
                       xsflag: int = -1, dividend_type: int = 0, formula_type: int = 0,
                       domain: int = 0, data_source: int = 0) -> Any:
        """按临时公式源码批量计算，公式只在本次调用临时生效、不保存到客户端。
        domain 为 0（默认）时
        按 stock_list 取数，非 0 时按该市场分类自动取股票列表（取值同 get_domain_sorthq 的 domain）；
        data_source 非 0 时按扩展行情取数。
        """
        if xsflag < -1:
            print("[错误] xsflag 不能小于 -1")
            return {}
        if dividend_type not in (0, 1, 2):
            print("[错误] dividend_type 只能是 0、1 或 2")
            return {}
        if formula_type not in (0, 1, 2):
            print("[错误] formula_type 只能是 0、1 或 2")
            return {}
        if count < -2:
            print("[错误] count 不能小于 -2")
            return {}
        return cls._formula_source(
            formula_type, formula_src=formula_src, stock_list=stock_list,
            stock_period=stock_period, start_time=start_time, end_time=end_time,
            count=count, return_count=max(0, return_count), return_date=return_date,
            xsflag=xsflag, dividend_type=dividend_type, domain=domain, data_source=data_source,
        )

    @classmethod
    def formula_by_file(cls, formula_file: str = "", stock_list: List[str] = [],
                        stock_period: str = "1d", start_time: str = "", end_time: str = "",
                        count: int = 0, return_count: int = 1, return_date: bool = False,
                        xsflag: int = -1, dividend_type: int = 0, formula_type: int = 0,
                        domain: int = 0, data_source: int = 0) -> Any:
        """从本地文本文件读取临时公式源码批量计算，公式不保存到客户端。
        domain 为 0（默认）时
        按 stock_list 取数，非 0 时按该市场分类自动取股票列表（取值同 get_domain_sorthq 的 domain）；
        data_source 非 0 时按扩展行情取数。
        """
        path = str(formula_file or "").strip()
        if not path:
            print("[错误] formula_file 不能为空")
            return {}
        if not os.path.isfile(path):
            print("[错误] formula_file 文件不存在: %s" % path)
            return {}
        try:
            content = Path(path).read_bytes()
        except OSError as exc:
            print("[错误] formula_file 读取失败: %s" % exc)
            return {}
        source = ""
        for encoding in ("utf-8", "gb2312"):
            try:
                source = content.decode(encoding)
                break
            except UnicodeDecodeError:
                continue
        if not source.strip():
            print("[错误] formula_file 内容为空或编码无法识别（请使用 UTF-8 或 GB2312）")
            return {}
        if xsflag < -1:
            print("[错误] xsflag 不能小于 -1")
            return {}
        if dividend_type not in (0, 1, 2):
            print("[错误] dividend_type 只能是 0、1 或 2")
            return {}
        if formula_type not in (0, 1, 2):
            print("[错误] formula_type 只能是 0、1 或 2")
            return {}
        if count < -2:
            print("[错误] count 不能小于 -2")
            return {}
        return cls._formula_source(
            formula_type, formula_src=source, stock_list=stock_list,
            stock_period=stock_period, start_time=start_time, end_time=end_time,
            count=count, return_count=max(0, return_count), return_date=return_date,
            xsflag=xsflag, dividend_type=dividend_type, domain=domain, data_source=data_source,
        )


    @classmethod
    def get_stock_list(cls, market: Optional[str] = None, list_type: int = 0) -> Any:
        """获取证券列表，移除 DLL 的 Value 外层包装以对齐 tqcenter。"""
        value = cls._clean(cls._tdx().get_stock_list(str(market or "上证主板"), list_type))
        if isinstance(value, dict) and isinstance(value.get("Value"), list):
            return value["Value"]
        return value


    @classmethod
    def get_stock_list_in_sector(cls, block_code: str = "", block_type: int = 0, list_type: int = 0) -> Any:
        """获取板块成分股，移除 DLL 的 Value 外层包装以对齐 tqcenter。"""
        value = cls._clean(cls._tdx().get_block_from_stock(cls._code(block_code), list_type))
        if isinstance(value, dict) and isinstance(value.get("Value"), list):
            return value["Value"]
        return value


    @classmethod
    def get_trading_dates(cls, market: str, start_time: str, end_time: str, count: int = -1) -> Any:
        """获取交易日列表，移除 DLL 的 Date 外层包装以对齐 tqcenter。"""
        value = cls._clean(cls._tdx().get_trade_calendar(cls._time(start_time), cls._time(end_time), market, count))
        if isinstance(value, dict) and isinstance(value.get("Date"), list):
            return value["Date"]
        return value


    @classmethod
    def get_financial_data(cls, stock_list: List[str] = [], field_list: List[str] = [], start_time: str = "", end_time: str = "", report_type: str = "report_time") -> Dict[str, Any]:
        return cls._pro(1, stock_list, field_list, start_time=cls._time(start_time), end_time=cls._time(end_time), report_type=report_type)

    @classmethod
    def get_financial_data_by_date(cls, stock_list: List[str] = [], field_list: List[str] = [], year: int = 0, mmdd: int = 0) -> Dict[str, Any]:
        return cls._pro(2, stock_list, field_list, year=year, mmdd=mmdd)

    @classmethod
    def get_gpjy_value(cls, stock_list: List[str] = [], field_list: List[str] = [], start_time: str = "", end_time: str = "") -> Dict[str, Any]:
        return cls._pro(3, stock_list, field_list, start_time=cls._time(start_time), end_time=cls._time(end_time))

    @classmethod
    def get_gpjy_value_by_date(cls, stock_list: List[str] = [], field_list: List[str] = [], year: int = 0, mmdd: int = 0) -> Dict[str, Any]:
        return cls._pro(4, stock_list, field_list, year=year, mmdd=mmdd)

    @classmethod
    def get_bkjy_value(cls, stock_list: List[str] = [], field_list: List[str] = [], start_time: str = "", end_time: str = "") -> Dict[str, Any]:
        return cls._pro(5, stock_list, field_list, start_time=cls._time(start_time), end_time=cls._time(end_time))

    @classmethod
    def get_bkjy_value_by_date(cls, stock_list: List[str] = [], field_list: List[str] = [], year: int = 0, mmdd: int = 0) -> Dict[str, Any]:
        return cls._pro(6, stock_list, field_list, year=year, mmdd=mmdd)

    @classmethod
    def get_scjy_value(cls, field_list: List[str] = [], start_time: str = "", end_time: str = "") -> Dict[str, Any]:
        """获取市场交易数据，内部使用固定市场代码而不暴露 stock_list。"""
        result = cls._pro(7, ["999999.SH"], field_list, start_time=cls._time(start_time), end_time=cls._time(end_time))
        return result.get("999999.SH", {})


    @classmethod
    def get_scjy_value_by_date(cls, field_list: List[str] = [], year: int = 0, mmdd: int = 0) -> Dict[str, Any]:
        """按日期获取市场交易数据，内部使用固定市场代码。"""
        result = cls._pro(8, ["999999.SH"], field_list, year=year, mmdd=mmdd)
        return result.get("999999.SH", {})


    @classmethod
    def get_global_data(cls, type: int = 0) -> Any:
        """获取市场整体情况（市场强弱统计），返回 {字段: 值} 结构。

        type 为 0 时取沪深京市场整体统计，为 1 时取港股与全球指数、期货行情。
        """
        try:
            query_type = int(type)
        except (TypeError, ValueError):
            print("[错误] type 必须是整数")
            return {}
        if query_type not in (0, 1):
            print("[错误] type 取值 0（沪深京市场）或 1（全球市场）")
            return {}
        return cls._native_value(cls._tdx().get_market_strength(query_type))


    @classmethod
    def get_misc_data(cls, stock_code: str = "", field_list: Optional[Iterable[str]] = None) -> Dict[str, Any]:
        """获取个股综合信息情况，返回 {栏目名: 内容} 结构。

        stock_code 需带市场后缀，如 "000001.SZ"；
        field_list 按中文栏目名筛选，如 ["所属板块", "最新分红"]，留空返回全部栏目。
        """
        code = str(stock_code or "").strip()
        if not code:
            print("[错误] stock_code 不能为空")
            return {}
        value = cls._clean(cls._tdx().get_misc_data(code))
        if isinstance(value, list):
            value = value[0] if value else {}
        if not isinstance(value, dict):
            return {}
        header = value.get("colheader") or []
        rows = value.get("data") or []
        if "title" not in header or "content" not in header:
            return {}
        title_index = header.index("title")
        content_index = header.index("content")
        result: Dict[str, Any] = {}
        for row in rows:
            if not isinstance(row, (list, tuple)) or len(row) <= max(title_index, content_index):
                continue
            title = str(row[title_index]).strip()
            if not title or title in result:
                continue
            result[title] = row[content_index]
        return cls._filter(result, field_list)


    @classmethod
    def get_gp_one_data(cls, stock_list: List[str] = [], field_list: List[str] = []) -> Dict[str, Any]:
        """获取股票单个专业数据。"""
        return cls._pro(9, stock_list, field_list)


    @classmethod
    def get_relation(cls, stock_code: str = "") -> Any:
        """获取股票所属板块，解开 DLL 的 Value 外层包装。"""
        value = cls._clean(cls._tdx().get_relation(cls._code(stock_code)))
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value

    @classmethod
    def get_kzz_info(cls, stock_code: str = "", field_list: List[str] = []) -> Any:
        """获取可转债信息，解开 DLL 的 Value 列表并按 field_list 筛选。"""
        value = cls._clean(cls._tdx().get_kzz_info(cls._code(stock_code)))
        if isinstance(value, dict) and "Value" in value:
            payload = value["Value"]
            if isinstance(payload, list):
                value = payload[0] if payload else {}
            else:
                value = payload
        return cls._filter(value, field_list)


    @classmethod
    def get_trackzs_etf_info(cls, zs_code: str = "") -> Any:
        """获取跟踪指数 ETF 列表，解开 DLL 的 Value 外层包装。"""
        value = cls._clean(cls._tdx().get_trackzs_etf_info(cls._code(zs_code)))
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value

    @classmethod
    def get_tick_data(cls, stock_code: str, date: str, startxh: int = 0, wantnum: int = 0) -> Any:
        """获取分笔成交数据；已安装 pandas 时返回 DataFrame，否则返回原始字典。"""
        value = cls._clean(cls._tdx().get_tick_data(cls._code(stock_code), cls._time(date), startxh, wantnum))
        return cls._frame(value)

    @classmethod
    def subscribe(cls, stock_list: List[str] = [], callback: Optional[Callable[[str], Any]] = None) -> None:
        stocks = cls._items(stock_list)
        if not stocks or callback is None:
            print("[错误] stock_list 和 callback 不能为空")
            return None
        with cls._sub_lock:
            cls._sub_codes = list(dict.fromkeys(cls._sub_codes + stocks))
            for stock in stocks:
                cls._sub_callbacks[stock] = callback
            codes = ";".join(cls._sub_codes)

        def on_data(data: str, datanum: int, datatype: int) -> int:
            with cls._sub_lock:
                callbacks = list(cls._sub_callbacks.values())
            for item in callbacks:
                try:
                    item(data)
                except Exception:
                    pass
            return 1

        cls._tdx().subscribe(codes, on_data)

    @classmethod
    def unsubscribe(cls, stock_list: List[str] = []) -> None:
        stocks = set(cls._items(stock_list))
        if not stocks:
            print("[错误] stock_list 不能为空")
            return None
        with cls._sub_lock:
            cls._sub_codes = [stock for stock in cls._sub_codes if stock not in stocks]
            for stock in stocks:
                cls._sub_callbacks.pop(stock, None)
        cls._tdx().unsubscribe()

    @classmethod
    def get_subscribe_hq_stock_list(cls) -> List[str]:
        with cls._sub_lock:
            return list(cls._sub_codes)

    @classmethod
    def get_zdt_data(cls, stock_list: List[str] = []) -> Any:
        """通过原生 GetZDTDataInStr 批量查询股票涨跌停数据。"""
        codes = cls._zdt_codes(stock_list)
        if not codes:
            return None
        value = cls._clean(cls._tdx().get_zdt_data(codes))
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value

    @classmethod
    def get_gb_info(cls, stock_code: str = "", date_list: List[Any] = [], count: int = 1) -> Any:
        """通过原生 GetGbInfoInStr 按指定日期列表查询股本数据。"""
        code = str(stock_code or "").strip()
        try:
            query_count = int(count)
        except (TypeError, ValueError):
            query_count = 0
        if not code or query_count <= 0:
            print("[错误] stock_code 和 count 参数无效")
            return None

        dates = list(date_list or [])[:query_count]
        dates.extend([0] * (query_count - len(dates)))
        value = cls._clean(cls._tdx().get_gb_info(
            cls._code(code), 0, query_count,
            json.dumps(dates, ensure_ascii=False), "", "",
        ))
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value

    @classmethod
    def get_gb_info_by_date(cls, stock_code: str = "", start_date: str = "", end_date: str = "") -> Any:
        """通过原生 GetGbInfoInStr 按日期区间查询股本数据。"""
        code = str(stock_code or "").strip()
        if not code or not str(start_date or "").strip() or not str(end_date or "").strip():
            print("[错误] stock_code、start_date 和 end_date 不能为空")
            return None

        value = cls._clean(cls._tdx().get_gb_info(
            cls._code(code), 1, 0, "",
            cls._time(start_date), cls._time(end_date),
        ))
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value
    @classmethod
    def _native_value(cls, raw: str) -> Any:
        """解析原生 DLL 返回结果并移除顶层 Value 包装。"""
        value = cls._clean(raw)
        if isinstance(value, dict) and "Value" in value:
            return value["Value"]
        return value

    @classmethod
    def get_zzgz_stocklist(cls, index_code: str = "", list_type: int = 0) -> Any:
        """获取指定指数的成分股列表。"""
        code = str(index_code or "").strip()
        if not code:
            print("[错误] index_code 不能为空")
            return []
        try:
            query_type = int(list_type)
        except (TypeError, ValueError):
            print("[错误] list_type 必须是整数")
            return []
        return cls._native_value(cls._tdx().get_zzgz_stocklist(code, query_type))

    @classmethod
    def get_minute_data(
        cls,
        stock_code: str = "",
        date: str = "",
        field_list: Optional[Iterable[str]] = None,
    ) -> Any:
        """获取指定股票指定日期的分时数据，并按 field_list 筛选字段。"""
        code = str(stock_code or "").strip()
        query_date = str(date or "").strip()
        if not code or not query_date:
            print("[错误] stock_code 和 date 不能为空")
            return []
        value = cls._native_value(
            cls._tdx().get_minute_data(cls._code(code), cls._time(query_date))
        )
        return cls._filter(value, field_list)
    @classmethod
    def _get_today_minute_data(cls, stock_code: str = "", minute: int = 1) -> Any:
        """获取指定股票当日指定分钟周期的分时数据。"""
        code = str(stock_code or "").strip()
        try:
            minute_value = int(minute)
        except (TypeError, ValueError):
            minute_value = 0
        if not code or minute_value <= 0:
            print("[错误] stock_code 和 minute 参数无效")
            return []
        return cls._native_value(cls._tdx().get_today_minute_data(cls._code(code), minute_value))

    @classmethod
    def get_exday_data(cls, stock_code: str = "", count: int = 1) -> Any:
        """获取日线统计数据，对齐 tqcenter.get_exday_data。"""
        code = str(stock_code or "").strip()
        try:
            count_value = int(count)
        except (TypeError, ValueError):
            count_value = 0
        if not code or count_value <= 0:
            print("[错误] stock_code 和 count 参数无效")
            return []
        return cls._native_value(cls._tdx().get_exday_data(cls._code(code), count_value))
    @classmethod
    def download_file(
        cls,
        stock_code: str = "",
        down_time: str = "",
        down_type: int = 1,
    ) -> Optional[str]:
        """按 tqcenter 规则下载文件并返回本地文件路径。"""
        try:
            option = int(down_type)
        except (TypeError, ValueError):
            print("[错误] down_type 必须是 1 到 7 的整数")
            return None
        if option not in range(1, 8):
            print("[错误] down_type 必须是 1 到 7 的整数")
            return None

        code = str(stock_code or "").strip().upper()
        if not code:
            code = "688318.SH"
        code_value = code.split(".", 1)[0]
        if not down_time:
            down_time = datetime.now().strftime("%Y%m%d%H%M%S")
        digits = "".join(char for char in str(down_time).strip() if char.isdigit())
        if option in (1, 5) and len(digits) < 4:
            print("[错误] down_time 必须包含有效年份")
            return None
        if option == 2 and len(digits) < 8:
            print("[错误] down_time 必须包含有效日期")
            return None

        file_names = {
            1: "holders%s_%s.json" % (code_value, digits[:4]),
            2: "etfpcf%s_%s.json" % (code_value, digits[:8]),
            3: "sentiment.json",
            4: "miscinfo.json",
            5: "mainbusi%s_%s.json" % (code_value, digits[:4]),
            6: "lhb%s.json" % code_value,
            7: "lockshare%s.json" % code_value,
        }
        query = {3: {"type": "yq"}, 4: {"type": "gpinfo"}}
        if option == 1:
            query[option] = {"type": "gp", "code": code_value, "rq": int(digits[:4])}
        elif option == 2:
            query[option] = {"type": "etf", "code": code_value, "rq": int(digits[:8])}
        elif option == 5:
            query[option] = {"type": "mainbusi", "code": code_value, "rq": int(digits[:4])}
        elif option == 6:
            query[option] = {"type": "lhb", "code": code_value}
        elif option == 7:
            query[option] = {"type": "lockshare", "code": code_value}

        try:
            url = "http://www.tdx.com.cn/fastapi/api/quantload/downdata?%s" % urlencode(query[option])
            request = Request(url, headers={"User-Agent": "tdxaidata/1.0"})
            with urlopen(request, timeout=600) as response:
                content = response.read()
            data_dir = Path(__file__).resolve().parent / "data"
            data_dir.mkdir(parents=True, exist_ok=True)
            output_path = data_dir / file_names[option]
            output_path.write_bytes(content)
            return "[下载成功]路径:%s" % output_path
        except Exception as exc:
            print("[错误] 下载文件失败: %s" % exc)
            return None
    @classmethod
    def get_sector_list(cls, list_type: int = 0) -> Any:
        """获取全部 A 股板块，底层等同于 get_stock_list(market="10")。"""
        return cls.get_stock_list(market="10", list_type=list_type)
    @classmethod
    def get_sector_list_ds(cls, list_type: int = 0) -> Any:
        """获取扩展市场板块列表，底层等同于 get_stock_list(market="110")。"""
        return cls.get_stock_list(market="110", list_type=list_type)
    @staticmethod
    def _unsupported(name: str) -> None:
        print("[错误] TdxAiData.h 当前没有与 tqcenter.%s 对应的实现" % name)
        return None

    get_user_sector = classmethod(lambda cls: cls._unsupported("get_user_sector"))
    get_ipo_info = classmethod(lambda cls, ipo_type=0, ipo_date=0: cls._unsupported("get_ipo_info"))
    get_pricevol = classmethod(lambda cls, stock_list=[]: cls._unsupported("get_pricevol"))
    get_match_stkinfo = classmethod(lambda cls, key_word="": cls._unsupported("get_match_stkinfo"))
