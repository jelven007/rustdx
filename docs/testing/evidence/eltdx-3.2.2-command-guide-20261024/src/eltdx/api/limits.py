"""Reference limit-price API."""

from __future__ import annotations

from .base import ApiBase


class LimitApi(ApiBase):
    def special(self, *, start_index: int = 0):
        return self._execute("special_limits", start_index=start_index)

    def scan_special(self, *, start_index: int = 0, max_rows: int = 10000):
        records = []
        index = start_index
        while index < start_index + max_rows:
            page = self.special(start_index=index)
            if not page.records:
                break
            records.extend(page.records)
            index += page.count
        return records
