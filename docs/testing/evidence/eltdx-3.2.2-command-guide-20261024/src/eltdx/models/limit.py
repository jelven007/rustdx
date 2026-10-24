"""Special limit-price models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class SpecialLimitRecord:
    exchange: str
    market_id: int
    code_num: int
    code: str
    upper_price_raw_f32: float
    lower_price_raw_f32: float
    record_hex: str = ""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def limit_up_price(self) -> float:
        return self.upper_price_raw_f32

    @property
    def limit_down_price(self) -> float:
        return self.lower_price_raw_f32


@dataclass(frozen=True, slots=True)
class SpecialLimitPage:
    start_index: int
    records: tuple[SpecialLimitRecord, ...]
    raw_payload: bytes = b""

    @property
    def count(self) -> int:
        return len(self.records)
