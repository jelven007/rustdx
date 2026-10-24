"""Quote snapshot models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class QuoteLevel:
    price: float
    volume: int
    price_delta_raw: int


@dataclass(frozen=True, slots=True)
class QuoteSnapshot:
    exchange: str
    market_id: int
    code: str
    active1: int
    last_price: float
    pre_close_price: float
    open_price: float
    high_price: float
    low_price: float
    time_raw: int
    unknown_after_time_raw: int
    total_hand: int
    current_hand: int
    amount: float
    amount_raw: int
    inside_dish: int
    outer_disc: int
    unknown_after_outer_raw: int
    open_amount_raw: int
    open_amount_yuan: float
    buy_levels: tuple[QuoteLevel, ...]
    sell_levels: tuple[QuoteLevel, ...]
    tail_raw: bytes

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def change(self) -> float:
        return self.last_price - self.pre_close_price

    @property
    def change_pct(self) -> float | None:
        if self.pre_close_price == 0:
            return None
        return self.change / self.pre_close_price * 100.0

    @property
    def sum_buy_vol(self) -> int:
        return sum(level.volume for level in self.buy_levels)

    @property
    def sum_sell_vol(self) -> int:
        return sum(level.volume for level in self.sell_levels)


@dataclass(frozen=True, slots=True)
class LegacyQuote:
    exchange: str
    market_id: int
    code: str
    active1: int
    last_price: float
    pre_close_price: float
    open_price: float
    high_price: float
    low_price: float
    server_time_raw: int
    unknown_after_time_raw: int
    total_hand: int
    current_hand: int
    amount: float
    amount_raw: int
    inside_dish: int
    outer_disc: int
    unknown_after_outer_raw: int
    open_amount_raw: int
    open_amount_yuan: float
    buy_levels: tuple[QuoteLevel, ...]
    sell_levels: tuple[QuoteLevel, ...]
    trading_status_raw: int
    tail_metrics_raw: tuple[int, int, int, int]
    rise_speed_raw: int | None
    active2: int | None
    record_hex: str = ""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def change(self) -> float:
        return self.last_price - self.pre_close_price

    @property
    def change_pct(self) -> float | None:
        if self.pre_close_price == 0:
            return None
        return self.change / self.pre_close_price * 100.0

    @property
    def sum_buy_vol(self) -> int:
        return sum(level.volume for level in self.buy_levels)

    @property
    def sum_sell_vol(self) -> int:
        return sum(level.volume for level in self.sell_levels)

    @property
    def trading_status_hex(self) -> str:
        return f"0x{self.trading_status_raw:04x}"


@dataclass(frozen=True, slots=True)
class CategoryQuoteRecord:
    exchange: str
    market_id: int
    code: str
    active1: int
    active2: int
    last_price: float
    pre_close_price: float
    open_price: float
    high_price: float
    low_price: float
    server_time_raw: int
    neg_price_raw: int
    total_hand: int
    current_hand: int
    amount: float
    amount_raw: int
    inside_dish: int
    outer_disc: int
    after_outer_raw: int
    open_amount_raw: int
    open_amount: float
    bid1: float
    ask1: float
    bid_vol1: int
    ask_vol1: int
    status_or_sort_raw: int
    rise_speed_raw: int
    rise_speed: float
    short_turnover_raw: int
    short_turnover: float
    min2_amount: float
    opening_rush_raw: int
    opening_rush: float
    extra_pair_raw: bytes
    vol_rise_speed: float
    depth: float
    extra_meta_raw: bytes
    tail_raw: bytes
    record_hex: str = ""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def change(self) -> float:
        return self.last_price - self.pre_close_price

    @property
    def change_pct(self) -> float | None:
        if self.pre_close_price == 0:
            return None
        return self.change / self.pre_close_price * 100.0

    @property
    def locked_amount(self) -> float:
        return self.bid1 * self.bid_vol1 * 100.0


@dataclass(frozen=True, slots=True)
class CategoryQuotePage:
    category: int
    sort_type: int
    start: int
    request_count: int
    sort_reverse: int
    filter_raw: int
    header: int
    records: tuple[CategoryQuoteRecord, ...]
    raw_payload: bytes = b""

    @property
    def count(self) -> int:
        return len(self.records)


@dataclass(frozen=True, slots=True)
class QuoteRefreshRecord:
    exchange: str
    market_id: int
    code: str
    active: int
    update_time_raw: int
    last_price: float
    last_close_price: float
    open_price: float
    high_price: float
    low_price: float
    status_or_reserved_raw: int
    total_hand: int
    current_hand: int
    amount: float
    amount_raw: int
    inside_dish: int
    outer_disc: int
    unknown_after_outer_raw: int
    open_amount_raw: int
    open_amount_yuan: float
    buy_levels: tuple[QuoteLevel, ...]
    sell_levels: tuple[QuoteLevel, ...]
    tail_raw: bytes
    record_hex: str = ""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"


@dataclass(frozen=True, slots=True)
class QuoteRefreshPage:
    requested_codes: tuple[str, ...]
    records: tuple[QuoteRefreshRecord, ...]
    decoded_payload: bytes
    raw_payload: bytes = b""

    @property
    def count(self) -> int:
        return len(self.records)
