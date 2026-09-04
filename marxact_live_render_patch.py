from __future__ import annotations

from functools import wraps
import sys
from typing import Any, Iterable

from ezdxf import colors as dxf_colors

from .kickthemap_dxf_export import ObjectLayerRule, build_object_layer_rules
from .virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


PATCH_VERSION = 1
LIVE_RENDER_VERSION_KEY = "_sleufbase_marxact_live_render_version"


def _is_marxact_virtual_layer(layer: Any) -> bool:
    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return False
    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    if not isinstance(payload, dict):
        return False
    return bool(
        str(payload.get("source", "")).strip().casefold() == "marxact"
        or metadata.get("marxact_boundary_3d")
        or metadata.get("marxact_source_path")
    )


def _configured_object_layer_rules() -> tuple[ObjectLayerRule, ...]:
    """Resolve the current KickTheMap/MarXact cable colours from settings.

    MarXact stores the already mapped Kabel/Leiding value in ``source_name``.
    The same configured ObjectLayerRule set that drives the DXF cross section is
    therefore the authoritative source for the map/raster display colour too.
    """

    try:
        from .settings import load_settings

        settings = load_settings()
        rows: list[tuple[object, object, object, object]] = []
        for rule in list(getattr(settings, "kickthemap_object_layer_rules", ()) or ()):
            raw_keywords = getattr(rule, "keywords", "")
            if isinstance(raw_keywords, (list, tuple)):
                keywords = ",".join(str(value) for value in raw_keywords if str(value).strip())
            else:
                keywords = str(raw_keywords or "")
            rows.append(
                (
                    keywords,
                    str(getattr(rule, "target_layer", "0") or "0"),
                    getattr(rule, "color", 256),
                    str(getattr(rule, "profile_label", "") or ""),
                )
            )
        configured = build_object_layer_rules(rows)
        if configured:
            return configured
    except Exception:
        pass
    return build_object_layer_rules()


def _aci_rgb(color: object) -> tuple[int, int, int] | None:
    try:
        aci = int(color)
    except (TypeError, ValueError):
        return None
    if aci < 1 or aci > 255:
        return None
    try:
        rgb = dxf_colors.aci2rgb(aci)
        return int(rgb[0]), int(rgb[1]), int(rgb[2])
    except Exception:
        return None


def _matching_rule(source_name: str, rules: Iterable[ObjectLayerRule]) -> ObjectLayerRule | None:
    for rule in rules:
        try:
            if rule.matches(source_name):
                return rule
        except Exception:
            continue
    return None


def apply_marxact_display_colors(
    layer: Any,
    rules: Iterable[ObjectLayerRule] | None = None,
    *,
    overwrite: bool = False,
) -> bool:
    """Persist per-object RGB colours on a MarXact virtual trench.

    The virtual-trench renderer deliberately falls back to one blue colour when
    ``display_rgb`` is absent. MarXact imports previously never populated that
    field, so the overview map in the DXF template could only show blue even
    though the cross-section itself used the configured ACI colours.
    """

    if not _is_marxact_virtual_layer(layer):
        return False
    metadata = getattr(layer, "metadata", {})
    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    points = payload.get("points") if isinstance(payload, dict) else None
    if not isinstance(points, list):
        return False
    resolved_rules = tuple(rules) if rules is not None else _configured_object_layer_rules()
    changed = False
    for point in points:
        if not isinstance(point, dict) or str(point.get("role", "")).casefold() != "object":
            continue
        if not overwrite and isinstance(point.get("display_rgb"), (list, tuple)) and len(point["display_rgb"]) >= 3:
            continue
        source_name = str(point.get("source_name", "") or "").strip()
        if not source_name:
            continue
        rule = _matching_rule(source_name, resolved_rules)
        if rule is None:
            continue
        rgb = _aci_rgb(rule.color)
        if rgb is None:
            continue
        new_value = [rgb[0], rgb[1], rgb[2]]
        if point.get("display_rgb") != new_value:
            point["display_rgb"] = new_value
            changed = True

    # Keep any configured dekband colour in the same metadata form. These rows
    # are optional, but when present they should follow the same map-render path.
    raw_dekbanden = metadata.get("template_dekband_lines")
    if isinstance(raw_dekbanden, list):
        for row in raw_dekbanden:
            if not isinstance(row, dict):
                continue
            if not overwrite and isinstance(row.get("display_rgb"), (list, tuple)) and len(row["display_rgb"]) >= 3:
                continue
            source_name = str(row.get("source_name", "") or row.get("label", "") or "").strip()
            rule = _matching_rule(source_name, resolved_rules) if source_name else None
            rgb = _aci_rgb(rule.color) if rule is not None else None
            if rgb is None:
                continue
            new_value = [rgb[0], rgb[1], rgb[2]]
            if row.get("display_rgb") != new_value:
                row["display_rgb"] = new_value
                changed = True
    return changed


def refresh_marxact_live_render(
    layer: Any,
    *,
    force: bool = False,
    rules: Iterable[ObjectLayerRule] | None = None,
) -> bool:
    """Upgrade a persisted/stale MarXact layer to the current clipped renderer.

    The detailed template raster is rebuilt immediately before export, but the
    SleufBase map and the template overview map consume ``GeoTiffLayer.image``.
    Old projects can therefore keep an image produced before local polygon
    clipping existed. Rebuild it once on first live render and cache the version
    in metadata so normal panning/zooming remains cheap.
    """

    if not _is_marxact_virtual_layer(layer):
        return False
    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return False
    colors_changed = apply_marxact_display_colors(layer, rules)
    try:
        current_version = int(metadata.get(LIVE_RENDER_VERSION_KEY, 0) or 0)
    except (TypeError, ValueError):
        current_version = 0
    if not force and not colors_changed and current_version >= PATCH_VERSION:
        return False

    from . import virtual_trench as vt

    old_image = getattr(layer, "image", None)
    try:
        image, bounds, transform = vt.build_virtual_trench_render(layer)
    except Exception:
        return False
    layer.image = image
    layer.bounds = bounds
    layer.transform = transform
    try:
        layer.epsg = 28992
    except Exception:
        pass
    invalidate = getattr(layer, "invalidate_native_rgba_cache", None)
    if callable(invalidate):
        try:
            invalidate()
        except Exception:
            pass
    metadata[LIVE_RENDER_VERSION_KEY] = PATCH_VERSION
    if old_image is not None and old_image is not image:
        try:
            old_image.close()
        except Exception:
            pass
    return True


def install_marxact_live_render_patch() -> None:
    from . import marxact_import as marxact_import_module
    from .cadastral_export import CadastralDxfExporter
    from .renderer import MapRenderer

    if int(getattr(MapRenderer, "_sleufbase_marxact_live_render_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_render = MapRenderer.render
    original_builder = marxact_import_module.build_marxact_virtual_layer
    original_prepare_virtual = CadastralDxfExporter._prepared_virtual_trench_export_layer

    @wraps(original_render)
    def render_with_current_marxact_layers(self, view_bounds, size, tiff_layers, dxf_overlays, *args, **kwargs):
        for layer in list(tiff_layers or ()):
            refresh_marxact_live_render(layer)
        return original_render(self, view_bounds, size, tiff_layers, dxf_overlays, *args, **kwargs)

    @wraps(original_builder)
    def build_colored_marxact_virtual_layer(*args, **kwargs):
        layer = original_builder(*args, **kwargs)
        # Force a second, authoritative render after the mapped object types have
        # been coloured. This is the image SleufBase itself will display.
        refresh_marxact_live_render(layer, force=True)
        return layer

    @wraps(original_prepare_virtual)
    def prepare_virtual_with_current_colors(self, layer):
        apply_marxact_display_colors(layer)
        return original_prepare_virtual(self, layer)

    MapRenderer.render = render_with_current_marxact_layers
    marxact_import_module.build_marxact_virtual_layer = build_colored_marxact_virtual_layer
    CadastralDxfExporter._prepared_virtual_trench_export_layer = prepare_virtual_with_current_colors

    # ``marxact_import_patch`` imports the builder by value. It is normally not
    # loaded until after this package-level patch, but update the alias as a
    # defensive measure for tests/embedded runtimes that imported it earlier.
    import_patch = sys.modules.get("SleufBase.marxact_import_patch")
    if import_patch is not None:
        setattr(import_patch, "build_marxact_virtual_layer", build_colored_marxact_virtual_layer)

    MapRenderer._sleufbase_marxact_live_render_patch_version = PATCH_VERSION
