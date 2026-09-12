from __future__ import annotations

import threading
import time
from types import SimpleNamespace
import unittest

from SleufBase.large_template_export_patch import (
    MAX_TEMPLATE_TASK_BATCH,
    PATCH_VERSION,
    _extend_template_for_slot_count_bulk,
    _parallel_ordered_bounded,
)


class _Entity:
    def __init__(self) -> None:
        self.translations = []

    def copy(self):
        return _Entity()

    def translate(self, dx, dy, dz):
        self.translations.append((dx, dy, dz))


class _Modelspace:
    def __init__(self, entities):
        self.entities = list(entities)
        self.added = []

    def __iter__(self):
        return iter(self.entities)

    def add_entity(self, entity):
        self.added.append(entity)
        self.entities.append(entity)


class _Document:
    def __init__(self, entities):
        self._modelspace = _Modelspace(entities)

    def modelspace(self):
        return self._modelspace


class _Exporter:
    TEMPLATE_SLOTS_PER_LAYOUT = 8

    def __init__(self):
        self.detect_calls = 0
        self.layout_pages = []

    def _detect_template_slots(self, document):
        self.detect_calls += 1
        return [SimpleNamespace(row_box=index) for index in range(8)]

    def _template_slot_pages(self, slots):
        return [slots]

    def _template_page_content_bounds(self, page_slots):
        return "source-bounds"

    def _template_page_translation(self, pages):
        return (0.0, -100.0)

    def _template_page_contains_entity(self, bounds, entity):
        return True

    def _ensure_template_layout_exists(self, document, page_number, main_viewport_translation):
        self.layout_pages.append((page_number, main_viewport_translation))


class LargeTemplateExportPatchTests(unittest.TestCase):
    def test_bulk_extension_scans_source_page_once_for_hundreds_of_slots(self) -> None:
        exporter = _Exporter()
        document = _Document([_Entity(), _Entity(), _Entity()])
        fallback_calls = []

        _extend_template_for_slot_count_bulk(
            exporter,
            document,
            808,
            lambda _document, count: fallback_calls.append(count),
        )

        self.assertEqual(exporter.detect_calls, 1)
        self.assertEqual(fallback_calls, [])
        self.assertEqual(len(exporter.layout_pages), 100)
        self.assertEqual(len(document._modelspace.added), 300)
        self.assertEqual(exporter.layout_pages[-1][0], 101)

    def test_parallel_ordered_bounded_preserves_order_and_worker_cap(self) -> None:
        lock = threading.Lock()
        active = 0
        peak = 0

        def worker(value):
            nonlocal active, peak
            with lock:
                active += 1
                peak = max(peak, active)
            try:
                time.sleep(0.002)
                return value * 2
            finally:
                with lock:
                    active -= 1

        values = list(range(180))
        result = _parallel_ordered_bounded(
            values,
            worker,
            max_workers=5,
        )

        self.assertEqual(result, [value * 2 for value in values])
        self.assertLessEqual(peak, 5)

    def test_large_batch_size_is_deliberately_bounded(self) -> None:
        self.assertEqual(PATCH_VERSION, 1)
        self.assertGreaterEqual(MAX_TEMPLATE_TASK_BATCH, 8)
        self.assertLessEqual(MAX_TEMPLATE_TASK_BATCH, 32)


if __name__ == "__main__":
    unittest.main()
