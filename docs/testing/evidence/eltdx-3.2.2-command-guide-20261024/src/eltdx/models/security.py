"""Security code table models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class SecurityCode:
    exchange: str
    market_id: int
    code: str
    name: str
    multiple: int
    decimal: int
    previous_close_price: float
    volume_ratio_base: float
    unknown0_raw: bytes
    previous_close_raw: bytes
    unknown3_raw: bytes
    category: str
    category_reason: str
    board: str
    board_reason: str

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"
