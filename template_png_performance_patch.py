from __future__ import annotations

from contextlib import contextmanager
import threading

from PIL import Image


PATCH_VERSION = 1
TEMPLATE_PNG_COMPRESS_LEVEL = 1
_SCOPE = threading.local()
_ORIGINAL_IMAGE_SAVE = Image.Image.save


@contextmanager
def _template_png_fast_save_scope():
    previous = getattr(_SCOPE, "enabled", None)
    _SCOPE.enabled = True
    try:
        yield
    finally:
        if previous is None:
            _SCOPE.__dict__.pop("enabled", None)
        else:
            _SCOPE.enabled = previous


def _install_scoped_pillow_save() -> None:
    if int(getattr(Image.Image, "_sleufbase_template_png_save_version", 0) or 0) >= PATCH_VERSION:
        return

    def save_scoped(self, fp, format=None, **params):
        if bool(getattr(_SCOPE, "enabled", False)) and str(format or "").upper() == "PNG":
            # DXF raster assets are temporary/local and gain nothing from CPU-
            # heavy PNG compression. Level 1 remains fully lossless.
            params.setdefault("compress_level", TEMPLATE_PNG_COMPRESS_LEVEL)
            params.setdefault("optimize", False)
        return _ORIGINAL_IMAGE_SAVE(self, fp, format=format, **params)

    Image.Image.save = save_scoped
    Image.Image._sleufbase_template_png_save_version = PATCH_VERSION


def install_template_png_performance_patch() -> None:
    from .cadastral_export import CadastralDxfExporter

    if int(
        getattr(CadastralDxfExporter, "_sleufbase_template_png_performance_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    _install_scoped_pillow_save()
    previous_build = CadastralDxfExporter._build_template_tiff_raster

    def _build_template_tiff_raster_fast_png(self, *args, **kwargs):
        with _template_png_fast_save_scope():
            return previous_build(self, *args, **kwargs)

    CadastralDxfExporter._build_template_tiff_raster = _build_template_tiff_raster_fast_png
    CadastralDxfExporter._sleufbase_template_png_performance_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_TEMPLATE_PNG_FAST_COMPRESSION = True
