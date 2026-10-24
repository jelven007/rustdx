"""Versioned, independent decoding of 0feb request dates in frozen evidence."""
import datetime as dt
import unittest


def date_from_selector(raw, schema=1):
    assert 0 <= raw < 2**32
    if schema == 1:
        # Historical captures used the incorrect Gregorian-ordinal formula.
        # This recovers the caller annotation, not the date selected on server.
        return int(dt.date.fromordinal(0xFED62304 - raw).strftime("%Y%m%d"))
    assert schema == 2, f"unsupported 0feb date schema {schema}"
    signed = raw if raw < 2**31 else raw - 2**32
    day = -signed
    assert 10000101 <= day <= 99991231
    dt.date(day // 10000, day // 100 % 100, day % 100)
    return day


class Regression(unittest.TestCase):
    def test_frozen_legacy_annotation_and_selected_date_are_distinct(self):
        self.assertEqual(date_from_selector(0xFECAD8FF, 1), 20260817)
        self.assertEqual(date_from_selector(0xFECAD8FF, 2), 20260609)
        self.assertEqual(date_from_selector(0xFECAD8FE, 2), 20260610)
        self.assertEqual(date_from_selector(0xFECAD82F, 2), 20260817)

    def test_leap_date_calendar_edges_and_unknown_schema(self):
        for day in [10000101, 20000229, 20240229, 20261231, 20270101, 99991231]:
            self.assertEqual(date_from_selector(2**32 - day, 2), day)
        for day in [0, 20230229, 20261000, 20261301]:
            with self.assertRaises((AssertionError, ValueError)):
                date_from_selector((-day) & 0xFFFFFFFF, 2)
        with self.assertRaises(AssertionError):
            date_from_selector(0xFECAD8FF, 3)


if __name__ == "__main__":
    unittest.main()
