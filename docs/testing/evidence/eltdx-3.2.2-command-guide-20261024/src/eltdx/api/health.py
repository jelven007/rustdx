"""Health-check API helpers."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from eltdx.client import TdxClient


def ping(client: TdxClient | None = None) -> str:
    """Run a client ping using the provided client or a default instance."""

    if client is None:
        from eltdx.client import TdxClient

        client = TdxClient.in_memory()
    return client.ping()
