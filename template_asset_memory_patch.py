from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from pathlib import Path
import threading

import numpy as np
from PIL import Image

from .virtual_trench import is_virtual_trench_layer


PATCH_VERSION = 2
SAFE_VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER = 1.25
MAX_VIRTUAL_TEMPLATE_ASSET_WORKERS = 1
VIRTUAL_TEMPLATE_PNG_COMPRESS_LEVEL = 1
TEMPLATE_UI_PUMP_INTERVAL_SECONDS = 0.08


def _contains_virtual_template_task(tasks: list[tuple[int, dict[str, object]]]) -> bool:
    for _layer_index, task_kwargs in tasks:
        layer = task_kwargs.get("layer")
        if layer is not None and is_virtual_trench_layer(layer):
            return True
    return False


def _memory_safe_normalize_template_tiff_raster_alpha(exporter, image: Image.Image) -> Image.Image:
    """Normalize TIFF alpha without the former full-size int16 RGB copy.

    The previous implementation converted every RGB channel to int16 before
    determining whether a pixel was almost white. For a large rotated raster
    that temporary copy can easily exceed a gigabyte. uint8 min/max subtraction
    is safe here because max is always greater than or equal to min.
    """

    rgba = image.convert("RGBA")
    pixels = np.array(rgba, dtype=np.uint8, copy=True)
    alpha = pixels[:, :, 3]
    rgb = pixels[:, :, :3]
    visible = alpha > exporter.MASK_ALPHA_THRESHOLD
    rgb_min = rgb.min(axis=2)
    rgb_max = rgb.max(axis=2)
    near_white = visible & (rgb_min >= 240) & ((rgb_max - rgb_min) <= 30)
    edge_background = exporter._edge_connected_mask((~visible) | near_white)

    alpha[:] = 0
    alpha[visible & ~edge_background] = 255
    return Image.fromarray(pixels, mode="RGBA")


def _pump_template_ui(status_callback) -> bool:
    """Keep Tk responsive while the single heavy raster worker is running.

    The legacy template export is invoked from the Tk main thread. Waiting for
    a Future with ``as_completed`` therefore froze painting and window events
    until a complete slot had finished. Pumping the bound status callback's Tk
    owner at a low frequency keeps the application interactive without adding
    more raster workers or increasing CPU pressure.
    """

    if status_callback is None or threading.current_thread() is not threading.main_thread():
        return False
    owner = getattr(status_callback, "__self__", None)
    if owner is None:
        return False
    update_idletasks = getattr(owner, "update_idletasks", None)
    update = getattr(owner, "update", None)
    if not callable(update_idletasks) and not callable(update):
        return False
    try:
        if callable(update_idletasks):
            update_idletasks()
        if callable(update):
            update()
        return True
    except Exception:
        return False


def install_template_asset_memory_patch() -> None:
    from .cadastral_export import (
        CadastralDxfExporter,
        CadastralExportError,
        PreparedTemplateSlotAssets,
    )

    if int(getattr(CadastralDxfExporter, "_sleufbase_template_asset_memory_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_batch = CadastralDxfExporter._prepare_template_slot_assets_batch

    # v0.3.24 already reduced virtual-trench rendering from 6x to 2x. MarXact
    # linework is still sharp at 1.25x (max 2000 px before rotation), while the
    # number of pixels to rotate/compress drops to ~39% of the previous 2x path.
    CadastralDxfExporter.VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER = (
        SAFE_VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER
    )

    def _normalize_template_tiff_raster_alpha_memory_safe(self, image: Image.Image) -> Image.Image:
        return _memory_safe_normalize_template_tiff_raster_alpha(self, image)

    def _build_template_tiff_raster_memory_safe(
        self,
        asset_dir: Path,
        layer,
        label: str,
        index: int,
        road_orientation_paths,
        terrain_boundary_paths,
        profile=None,
        reverse_orientation: bool = False,
    ) -> Path:
        image_path = self._unique_raster_copy_path(asset_dir, f"{label}_geotiff.png")
        virtual_layer = is_virtual_trench_layer(layer)
        export_layer = self._prepared_virtual_trench_export_layer(layer)

        orientation_vector = self._template_tiff_orientation_pixel_vector(
            export_layer,
            road_orientation_paths,
            terrain_boundary_paths,
            profile=profile,
        )

        image = export_layer.image.convert("RGBA")
        if virtual_layer and export_layer.image is not layer.image:
            try:
                export_layer.image.close()
            except Exception:
                pass

        try:
            # Virtual trenches are born with a transparent background. Cropping
            # before rotation avoids rotating a large empty canvas, and unlike
            # normal GeoTIFFs they do not need the expensive white-background
            # alpha normalizer at all.
            if virtual_layer:
                cropped = self._crop_template_tiff_to_visible_bounds(image)
                if cropped is not image:
                    image.close()
                    image = cropped

            if orientation_vector is not None:
                dx, dy = orientation_vector
                if (dx * dx + dy * dy) > 1e-6:
                    current_bearing = self._template_bearing_from_pixel_vector(dx, dy)
                    target_bearing = (
                        270.0
                        if profile is not None
                        else (90.0 if reverse_orientation else 270.0)
                    )
                    rotation_degrees = self._normalize_rotation_degrees(
                        target_bearing - current_bearing
                    )
                    rotated = image.rotate(
                        rotation_degrees,
                        # MarXact/virtual rasters are crisp vector-like line art.
                        # Bilinear rotation is visibly sufficient here and much
                        # cheaper than bicubic; normal GeoTIFFs keep bicubic.
                        resample=(
                            Image.Resampling.BILINEAR
                            if virtual_layer
                            else Image.Resampling.BICUBIC
                        ),
                        expand=True,
                        fillcolor=(255, 255, 255, 0),
                    )
                    image.close()
                    image = rotated

            cropped = self._crop_template_tiff_to_visible_bounds(image)
            if cropped is not image:
                image.close()
                image = cropped

            if not virtual_layer:
                normalized = self._normalize_template_tiff_raster_alpha(image)
                if normalized is not image:
                    image.close()
                    image = normalized

            try:
                if virtual_layer:
                    # DXF image assets do not benefit from maximum PNG compression;
                    # a low level cuts CPU time substantially and remains lossless.
                    image.save(
                        image_path,
                        format="PNG",
                        compress_level=VIRTUAL_TEMPLATE_PNG_COMPRESS_LEVEL,
                        optimize=False,
                    )
                else:
                    image.save(image_path, format="PNG")
            except OSError as exc:
                raise CadastralExportError(
                    f"GeoTIFF-afbeelding kon niet worden opgeslagen voor {label}: {exc}"
                ) from exc
            return image_path.resolve()
        finally:
            try:
                image.close()
            except Exception:
                pass

    def _prepare_template_slot_assets_batch_memory_safe(
        self,
        tasks: list[tuple[int, dict[str, object]]],
        *,
        status_callback=None,
    ) -> dict[int, PreparedTemplateSlotAssets]:
        if not tasks:
            return {}
        if not _contains_virtual_template_task(tasks):
            return original_batch(self, tasks, status_callback=status_callback)

        # MarXact imports can contain dozens of virtual trenches. Keep their
        # raster/rotation work sequential so peak RAM and CPU load stay bounded.
        # The main Tk thread only waits for short intervals and pumps UI events,
        # so the window keeps repainting/responding while the worker runs.
        prepared_assets: dict[int, PreparedTemplateSlotAssets] = {}
        max_workers = min(MAX_VIRTUAL_TEMPLATE_ASSET_WORKERS, len(tasks))
        with ThreadPoolExecutor(
            max_workers=max_workers,
            thread_name_prefix="template-assets-virtual",
        ) as executor:
            future_map = {
                executor.submit(self._prepare_template_slot_assets, **task_kwargs): layer_index
                for layer_index, task_kwargs in tasks
            }
            pending = set(future_map)
            completed_assets = 0
            while pending:
                done, pending = wait(
                    pending,
                    timeout=TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
                    return_when=FIRST_COMPLETED,
                )
                if not done:
                    _pump_template_ui(status_callback)
                    continue
                for future in done:
                    layer_index = future_map[future]
                    try:
                        prepared_assets[layer_index] = future.result()
                    except CadastralExportError:
                        raise
                    except Exception as exc:
                        raise CadastralExportError(
                            f"Sjabloonvak {layer_index + 1} kon niet worden voorbereid: {exc}"
                        ) from exc
                    completed_assets += 1
                    if status_callback is not None:
                        status_callback(
                            f"Bereid sjabloonafbeeldingen voor... {completed_assets}/{len(tasks)}"
                        )
                _pump_template_ui(status_callback)
        return prepared_assets

    CadastralDxfExporter._normalize_template_tiff_raster_alpha = (
        _normalize_template_tiff_raster_alpha_memory_safe
    )
    CadastralDxfExporter._build_template_tiff_raster = _build_template_tiff_raster_memory_safe
    CadastralDxfExporter._prepare_template_slot_assets_batch = (
        _prepare_template_slot_assets_batch_memory_safe
    )
    CadastralDxfExporter._sleufbase_template_asset_memory_patch_version = PATCH_VERSION
