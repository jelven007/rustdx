"""Session-related response models."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime


@dataclass(frozen=True, slots=True)
class HandshakeInfo:
    server_datetime: datetime | None
    session_minutes_1: tuple[str, ...]
    session_minutes_2: tuple[str, ...]
    server_date_1: date | None
    server_date_2: date | None
    server_name: str
    product_tag: str
    unknown_time_1_raw: int | None
    unknown_time_2_raw: int | None
    flags_raw: bytes
    tail_control_raw: bytes
    raw_payload: bytes


@dataclass(frozen=True, slots=True)
class HeartbeatAck:
    reserved: bytes
    server_date_raw: int
    server_date: date | None
    raw_payload: bytes
