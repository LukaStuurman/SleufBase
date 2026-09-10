from __future__ import annotations

from typing import Any

from . import dxf_template_pipeline_patch as pipeline_perf
from . import template_export_performance_patch as template_perf
from .resource_policy import get_resource_policy


PATCH_VERSION = 2
RESOURCE_POLICY = get_resource_policy()
MAX_PARALLEL_TEMPLATE_MAPS = max(1, RESOURCE_POLICY.light_map_workers)
MAX_PARALLEL_TEMPLATE_ASSETS = max(1, RESOURCE_POLICY.template_asset_workers)
_PROFILE_LAYER_CACHE_ATTR = "_sleufbase_profile_layer_cache_v1"
_PROFILE_LEADER_CACHE_ATTR = "_sleufbase_profile_leader_block_v1"


def _configure_profile_layers_once(exporter: Any, document: Any, profile_points: tuple[Any, ...]) -> None:
    """Configure each distinct profile layer at most once per effective style.

    The legacy implementation performs a layer-table contains/get/update cycle for
    every profile point. Profiles commonly contain many cables on the same layer,
    so that work is redundant. Keeping the last requested color per layer preserves
    the legacy final state when duplicate points use the same layer name.
    """

    requested: dict[str, int] = {}
    for point in profile_points:
        layer_name = str(getattr(point, "layer_name", "") or "")
        if not layer_name or layer_name == "0":
            continue
        requested[layer_name] = int(getattr(point, "color", 7))

    if not requested:
        return

    cache = getattr(document, _PROFILE_LAYER_CACHE_ATTR, None)
    if not isinstance(cache, dict):
        cache = {}
        try:
            setattr(document, _PROFILE_LAYER_CACHE_ATTR, cache)
        except Exception:
            # A lightweight test/embed document may not allow arbitrary attrs.
            # The unique-layer compaction still avoids per-point table work.
            cache = {}

    lineweight = int(exporter.TEMPLATE_PROFILE_LINEWEIGHT)
    for layer_name, color in requested.items():
        style = (color, lineweight)
        if cache.get(layer_name) == style:
            continue
        if layer_name in document.layers:
            layer = document.layers.get(layer_name)
            layer.dxf.color = color
            layer.dxf.lineweight = lineweight
        else:
            document.layers.add(
                name=layer_name,
                dxfattribs={"color": color, "lineweight": lineweight},
            )
        cache[layer_name] = style


def install_template_fill_performance_patch() -> None:
    """Speed up template work while scaling safely to the detected hardware."""

    from .cadastral_export import CadastralDxfExporter

    if int(
        getattr(CadastralDxfExporter, "_sleufbase_template_fill_performance_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    # This patch is installed last in the template performance chain, making it
    # the single place that publishes the resolved process-wide resource policy
    # to the older pipeline modules. Output geometry and reverse semantics are
    # intentionally untouched.
    template_perf.MAX_VIRTUAL_TEMPLATE_MAP_WORKERS = MAX_PARALLEL_TEMPLATE_MAPS
    template_perf.MAX_HEAVY_TEMPLATE_RASTER_WORKERS = max(
        1, RESOURCE_POLICY.heavy_raster_workers
    )
    pipeline_perf.MAX_ADAPTIVE_TIFF_WORKERS = max(1, RESOURCE_POLICY.heavy_raster_workers)
    pipeline_perf.VIRTUAL_TIFF_PIXEL_BUDGET = max(
        4_000_000, RESOURCE_POLICY.virtual_tiff_pixel_budget
    )

    previous_leader_block = CadastralDxfExporter._ensure_template_profile_leader_block

    def _ensure_template_profile_layers_fast(self, document, profile_points) -> None:
        _configure_profile_layers_once(self, document, tuple(profile_points))

    def _ensure_template_profile_leader_block_fast(self, document) -> str:
        cached = getattr(document, _PROFILE_LEADER_CACHE_ATTR, None)
        if isinstance(cached, str) and cached:
            return cached
        block_name = previous_leader_block(self, document)
        try:
            setattr(document, _PROFILE_LEADER_CACHE_ATTR, block_name)
        except Exception:
            pass
        return block_name

    @staticmethod
    def _template_asset_worker_count_adaptive(asset_count: int) -> int:
        return max(1, min(MAX_PARALLEL_TEMPLATE_ASSETS, int(asset_count)))

    CadastralDxfExporter._ensure_template_profile_layers = _ensure_template_profile_layers_fast
    CadastralDxfExporter._ensure_template_profile_leader_block = (
        _ensure_template_profile_leader_block_fast
    )
    CadastralDxfExporter._template_asset_worker_count = _template_asset_worker_count_adaptive
    CadastralDxfExporter._sleufbase_template_fill_performance_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_TEMPLATE_MAP_WORKERS = MAX_PARALLEL_TEMPLATE_MAPS
    CadastralDxfExporter.SLEUFBASE_TEMPLATE_ASSET_WORKERS = MAX_PARALLEL_TEMPLATE_ASSETS
    CadastralDxfExporter.SLEUFBASE_HEAVY_RASTER_WORKERS = RESOURCE_POLICY.heavy_raster_workers
    CadastralDxfExporter.SLEUFBASE_TIFF_PIXEL_BUDGET = RESOURCE_POLICY.virtual_tiff_pixel_budget
    CadastralDxfExporter.SLEUFBASE_RESOURCE_MODE = RESOURCE_POLICY.mode
    CadastralDxfExporter.SLEUFBASE_PROFILE_LAYER_CACHE = True
    CadastralDxfExporter.SLEUFBASE_PROFILE_LEADER_BLOCK_CACHE = True
