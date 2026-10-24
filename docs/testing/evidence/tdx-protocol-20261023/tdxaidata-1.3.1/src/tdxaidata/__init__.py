"""tdxaidata —— 通达信 TdxAiData Python 数据接口（跨平台）。

基于 TdxAiData 原生动态库（Windows: ``TdxAiData.dll``、macOS: ``libTdxAiData.dylib``、
Linux: ``libTdxAiData.so``）提供行情、基础信息、专业数据、分笔与实时订阅能力，
对齐 tqcenter 命名，不依赖 tqcenter / TPyth / 通达信客户端。

用法::

    from tdxaidata import tqs

    snap = tqs.get_market_snapshot("600000.SH", ["Name", "Price", "Close"])
    print(snap)
"""

from .tdxaidata import TqsError, tqs

__all__ = ["tqs", "TqsError"]
__version__ = "1.3.1"
