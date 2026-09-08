from __future__ import annotations

from . import dxf_template_pipeline_patch as pipeline
from . import template_export_performance_patch as performance_patch


PATCH_VERSION = 1


def _contains_virtual_template_task_live(tasks):
    """Keep the v2 pipeline coupled to the active performance-patch predicate.

    The existing performance regression suite replaces this predicate while it
    verifies concurrency. Resolving it through the module at call time also
    prevents a stale imported function reference when later runtime patches
    refine virtual-trench detection.
    """

    return performance_patch._contains_virtual_template_task(tasks)


def install_dxf_template_pipeline_bridge() -> None:
    if int(getattr(pipeline, "_sleufbase_live_predicate_bridge_version", 0) or 0) >= PATCH_VERSION:
        return
    pipeline._contains_virtual_template_task = _contains_virtual_template_task_live
    pipeline._sleufbase_live_predicate_bridge_version = PATCH_VERSION
