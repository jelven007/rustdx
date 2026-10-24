"""Intraday minute-chart models."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime


@dataclass(frozen=True, slots=True)
class MinutePoint:
    index: int
    time_label: str
    time: datetime | None
    price: float
    price_milli: int
    volume: int
    price_field: int | None = None
    avg_field: int | None = None
    avg_price: float | None = None
    price_raw: int | None = None
    avg_raw: int | None = None
    price_delta_raw: int | None = None
    aux_delta_raw: int | None = None
    record_hex: str = ""


@dataclass(frozen=True, slots=True)
class MinuteSeries:
    exchange: str
    market_id: int
    code: str
    trading_date: date | None
    points: tuple[MinutePoint, ...]
    reserved_zero: int | None = None
    prev_close: float | None = None
    open_price: float | None = None
    date_selector_raw: int | None = None
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.points)

    @property
    def volume_sum(self) -> int:
        return sum(point.volume for point in self.points)


@dataclass(frozen=True, slots=True)
class MinuteAuxPoint:
    index: int
    time_label: str
    series_a: float | int
    series_b: float | int
    buy_commission: int | None = None
    sell_commission: int | None = None
    previous_day_cumulative_volume: float | None = None
    current_day_cumulative_volume: float | None = None
    cumulative_volume: float | None = None
    record_hex: str = ""


@dataclass(frozen=True, slots=True)
class MinuteAuxSeries:
    exchange: str
    market_id: int
    code: str
    selector_raw: int
    kind: str
    points: tuple[MinuteAuxPoint, ...]
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.points)


@dataclass(frozen=True, slots=True)
class SparklineSeries:
    exchange: str
    market_id: int
    code: str
    selector_raw: int
    selector_echo: int
    window_or_count_raw: int
    max_count_raw: int
    base_price: float
    prices: tuple[float, ...]
    reserved_param_u32: int
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.prices)
