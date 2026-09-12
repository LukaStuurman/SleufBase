from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from typing import Any, Callable

from .template_asset_memory_patch import (
    TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
    _pump_template_ui,
)


PATCH_VERSION = 1
MAX_TEMPLATE_TASK_BATCH = 24
MAX_PARALLEL_INFLIGHT_MULTIPLIER = 2


def _safe_status(callback, message: str) -> None:
    if callback is None:
        return
    try:
        callback(message)
    except Exception:
        pass


def _parallel_ordered_bounded(
    items: list[Any],
    worker: Callable[[Any], Any],
    *,
    max_workers: int,
    status_callback=None,
    status_label: str = "",
) -> list[Any]:
    """Ordered parallel map with a bounded submission queue and UI pumping."""

    if not items:
        return []
    if len(items) == 1 or max_workers <= 1:
        result = [worker(items[0])]
        if status_label:
            _safe_status(status_callback, f"{status_label}...")
        return result

    worker_count = max(1, min(int(max_workers), len(items)))
    inflight_limit = min(
        len(items),
        max(worker_count, worker_count * MAX_PARALLEL_INFLIGHT_MULTIPLIER),
    )
    results: list[Any] = [None] * len(items)
    item_iter = iter(enumerate(items))
    future_map: dict[Any, int] = {}
    completed = 0
    executor = ThreadPoolExecutor(
        max_workers=worker_count,
        thread_name_prefix="template-fetch-bounded",
    )

    def fill_queue() -> None:
        while len(future_map) < inflight_limit:
            try:
                index, item = next(item_iter)
            except StopIteration:
                return
            future_map[executor.submit(worker, item)] = index

    try:
        fill_queue()
        while future_map:
            done, _pending = wait(
                set(future_map),
                timeout=TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
                return_when=FIRST_COMPLETED,
            )
            if not done:
                _pump_template_ui(status_callback)
                continue
            for future in done:
                index = future_map.pop(future)
                results[index] = future.result()
                completed += 1
                if status_label:
                    _safe_status(
                        status_callback,
                        f"{status_label}... {completed}/{len(items)}",
                    )
            fill_queue()
            _pump_template_ui(status_callback)
    finally:
        executor.shutdown(wait=True, cancel_futures=True)
    return results


def _extend_template_for_slot_count_bulk(
    exporter,
    document,
    required_slot_count: int,
    original_extend,
) -> None:
    """Append all required template pages from one source-page scan.

    The stock implementation re-detects every slot and re-scans the complete
    modelspace after each appended page. That becomes quadratic for 63+ slots.
    This implementation discovers the source page once, then copies it directly
    to every required page.
    """

    required_pages = max(
        1,
        (
            int(required_slot_count)
            + int(exporter.TEMPLATE_SLOTS_PER_LAYOUT)
            - 1
        )
        // int(exporter.TEMPLATE_SLOTS_PER_LAYOUT),
    )
    slots = exporter._detect_template_slots(document)
    pages = exporter._template_slot_pages(slots)
    current_pages = len(pages)
    if current_pages >= required_pages:
        return
    if not pages:
        return original_extend(document, required_slot_count)

    source_page_slots = pages[-1]
    if len(source_page_slots) != int(exporter.TEMPLATE_SLOTS_PER_LAYOUT):
        return original_extend(document, required_slot_count)

    source_bounds = exporter._template_page_content_bounds(source_page_slots)
    step_x, step_y = exporter._template_page_translation(pages)
    if abs(float(step_x)) < 1e-9 and abs(float(step_y)) < 1e-9:
        return original_extend(document, required_slot_count)

    source_entities = [
        entity
        for entity in document.modelspace()
        if exporter._template_page_contains_entity(source_bounds, entity)
    ]
    if not source_entities:
        return original_extend(document, required_slot_count)

    modelspace = document.modelspace()
    for page_number in range(current_pages + 1, required_pages + 1):
        page_offset = page_number - current_pages
        dx = float(step_x) * page_offset
        dy = float(step_y) * page_offset
        for entity in source_entities:
            copied_entity = entity.copy()
            copied_entity.translate(dx, dy, 0.0)
            modelspace.add_entity(copied_entity)

        exporter._ensure_template_layout_exists(
            document,
            page_number,
            main_viewport_translation=(float(step_x), float(step_y)),
        )


def install_large_template_export_patch() -> None:
    from .cadastral_export import CadastralDxfExporter
    from . import template_export_performance_patch as performance_patch

    exporter_class = CadastralDxfExporter
    if int(getattr(exporter_class, "_sleufbase_large_template_export_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    performance_patch._parallel_ordered = _parallel_ordered_bounded

    original_extend = exporter_class._extend_template_for_slot_count
    original_assets_batch = exporter_class._prepare_template_slot_assets_batch

    def _extend_template_for_slot_count_fast(self, document, required_slot_count: int) -> None:
        return _extend_template_for_slot_count_bulk(
            self,
            document,
            required_slot_count,
            lambda doc, count: original_extend(self, doc, count),
        )

    def _prepare_template_slot_assets_batch_bounded(
        self,
        tasks,
        *,
        status_callback=None,
    ):
        task_list = list(tasks or [])
        if len(task_list) <= MAX_TEMPLATE_TASK_BATCH:
            return original_assets_batch(
                self,
                task_list,
                status_callback=status_callback,
            )

        prepared = {}
        total = len(task_list)
        for start in range(0, total, MAX_TEMPLATE_TASK_BATCH):
            end = min(total, start + MAX_TEMPLATE_TASK_BATCH)
            _safe_status(
                status_callback,
                f"Bereid DXF-sjabloon in begrensde batches voor... {start}/{total}",
            )
            chunk_result = original_assets_batch(
                self,
                task_list[start:end],
                status_callback=status_callback,
            )
            prepared.update(chunk_result)
            _pump_template_ui(status_callback)
        _safe_status(
            status_callback,
            f"Bereid DXF-sjabloon in begrensde batches voor... {total}/{total}",
        )
        return prepared

    exporter_class._extend_template_for_slot_count = _extend_template_for_slot_count_fast
    exporter_class._prepare_template_slot_assets_batch = _prepare_template_slot_assets_batch_bounded
    exporter_class._sleufbase_large_template_export_patch_version = PATCH_VERSION
    exporter_class.SLEUFBASE_LARGE_TEMPLATE_BATCH_SIZE = MAX_TEMPLATE_TASK_BATCH
    exporter_class.SLEUFBASE_TEMPLATE_FETCH_QUEUE_BOUNDED = True
