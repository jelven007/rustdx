"""Basic response model."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class Response:
    """Simple response container for future protocol results."""

    status: str
    payload: bytes = b""
