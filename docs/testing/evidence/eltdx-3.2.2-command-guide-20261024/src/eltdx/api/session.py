"""Connection/session API."""

from __future__ import annotations

from .base import ApiBase


class SessionApi(ApiBase):
    def ping(self) -> str:
        return self._transport.request("ping")

    def handshake(self):
        return self._execute("handshake")

    def heartbeat(self):
        return self._execute("heartbeat")
