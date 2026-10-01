from __future__ import annotations

from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter, CadastralExportError
from SleufBase.dxf_template_pipeline_v4_patch import _render_normal_and_reverse_map_pair
from SleufBase.exporting import MapExporter, _prepare_export_tiff_layer
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform, ProfileReferenceAnnotation
from SleufBase.template_raster_io import save_template_png


def _layer(image):
    return GeoTiffLayer(Path("test.tif"), image, GeoTransform(1, 0, 0, 0, -1, 2), Bounds(0, 0, 2, 2), 28992, 0.5, {})


class TemplateRasterReliabilityTests(unittest.TestCase):
    def test_atomic_png_preserves_old_file_after_encode_or_publish_failure(self):
        for stage in ("encode", "publish"):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as temp, Image.new("RGBA", (4, 3)) as image:
                target = Path(temp) / "map.png"
                target.write_bytes(b"old-complete-image")

                def fail_encode(path, **kwargs):
                    Path(path).write_bytes(b"partial")
                    raise OSError("encoding interrupted")

                operation = patch.object(image, "save", side_effect=fail_encode) if stage == "encode" else patch(
                    "SleufBase.template_raster_io.os.replace", side_effect=OSError("output locked")
                )
                with operation, self.assertRaises(OSError):
                    save_template_png(image, target)
                self.assertEqual(target.read_bytes(), b"old-complete-image")
                self.assertEqual(list(Path(temp).iterdir()), [target])

    def test_fast_png_round_trip_preserves_every_pixel_and_alpha(self):
        with tempfile.TemporaryDirectory() as temp, Image.new("RGBA", (3, 2)) as image:
            pixels = [(255, 255, 255, 0), (0, 12, 30, 127), (10, 20, 30, 255)] * 2
            image.putdata(pixels)
            target = Path(temp) / "map.png"
            with patch.object(image, "save", wraps=image.save) as save:
                save_template_png(image, target)
            self.assertEqual(save.call_args.kwargs["compress_level"], 1)
            self.assertFalse(save.call_args.kwargs["optimize"])
            with Image.open(target) as restored:
                self.assertEqual(list(restored.getdata()), pixels)

    def test_export_tiff_reuses_rgba_source_without_mutating_or_closing_it(self):
        with Image.new("RGBA", (2, 2), (1, 2, 3, 127)) as source:
            source.putpixel((0, 0), (245, 250, 255, 255))
            with patch.object(source, "convert", side_effect=AssertionError("redundant RGBA copy")):
                prepared = _prepare_export_tiff_layer(_layer(source), forced_opacity=1)
            with prepared.image:
                self.assertEqual(prepared.image.getpixel((0, 0)), (245, 250, 255, 0))
                self.assertEqual(prepared.image.getpixel((1, 1)), (1, 2, 3, 127))
            self.assertEqual(source.getpixel((0, 0)), (245, 250, 255, 255))
            self.assertEqual(prepared.opacity, 1)

    def test_export_tiff_closes_owned_conversion_when_array_preparation_fails(self):
        with Image.new("RGB", (2, 2)) as source:
            converted = source.convert("RGBA")
            with patch.object(source, "convert", return_value=converted), patch(
                "SleufBase.exporting.np.array", side_effect=RuntimeError("allocation failed")
            ), self.assertRaisesRegex(RuntimeError, "allocation failed"):
                _prepare_export_tiff_layer(_layer(source))
            with self.assertRaises(ValueError):
                converted.getpixel((0, 0))
            self.assertEqual(source.getpixel((0, 0)), (0, 0, 0))

    def test_base_map_raster_closes_page_on_success_and_save_error(self):
        for failure in (False, True):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temp, Image.new("RGBA", (2, 2)) as source:
                page = Image.new("RGBA", (2, 2), (10, 20, 30, 255))
                exporter = object.__new__(CadastralDxfExporter)
                page_exporter = SimpleNamespace(build_page_image=lambda *args, **kwargs: page)
                with patch("SleufBase.template_raster_io.os.replace", side_effect=OSError("locked") if failure else None, wraps=None if failure else __import__("os").replace):
                    if failure:
                        with self.assertRaises(CadastralExportError):
                            exporter._build_template_map_raster(Path(temp), _layer(source), "PS1", 1, "polygon", (0, 0, 0), (0, 0, 0), page_exporter, [], None, None, None)
                    else:
                        result = exporter._build_template_map_raster(Path(temp), _layer(source), "PS1", 1, "polygon", (0, 0, 0), (0, 0, 0), page_exporter, [], None, None, None)
                        self.assertTrue(result.exists())
                with self.assertRaises(ValueError):
                    page.getpixel((0, 0))
                source.getpixel((0, 0))

    def test_fallback_page_builder_closes_intermediates_on_success_and_failure(self):
        for stage in ("success", "prepare", "render", "compose"):
            with self.subTest(stage=stage), Image.new("RGBA", (2, 2)) as source:
                owned = []

                def allocate():
                    image = Image.new("RGBA", (2, 2))
                    owned.append(image)
                    return image

                def prepare(*args, **kwargs):
                    if stage == "prepare":
                        raise RuntimeError(stage)
                    return SimpleNamespace(image=allocate())

                def render(*args, **kwargs):
                    if stage == "render":
                        raise RuntimeError(stage)
                    return allocate()

                def compose(*args, **kwargs):
                    if stage == "compose":
                        raise RuntimeError(stage)
                    return allocate()

                exporter = object.__new__(MapExporter)
                exporter.map_width_px = exporter.map_height_px = 2
                exporter._determine_map_bounds = lambda bounds: bounds
                exporter.default_background_provider = SimpleNamespace(fetch_map=lambda *args: allocate())
                exporter.renderer = SimpleNamespace(render=render)
                exporter._compose_page = compose
                with patch("SleufBase.exporting._prepare_export_tiff_layer", side_effect=prepare):
                    if stage == "success":
                        page = exporter.build_page_image(_layer(source), [])
                        self.assertEqual(page.getpixel((0, 0)), (0, 0, 0, 0))
                        page.close()
                    else:
                        with self.assertRaisesRegex(RuntimeError, stage):
                            exporter.build_page_image(_layer(source), [])
                for image in owned:
                    with self.assertRaises(ValueError):
                        image.getpixel((0, 0))
                source.getpixel((0, 0))

    def test_map_pair_releases_all_owned_images_on_every_failure_stage(self):
        for stage in ("success", "render", "normal_page", "reverse_page", "normal_save", "reverse_save"):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as temp, Image.new("RGBA", (2, 2)) as source:
                owned = []

                def new_image():
                    image = Image.new("RGBA", (2, 2))
                    owned.append(image)
                    return image

                def render(*args, **kwargs):
                    if stage == "render":
                        raise RuntimeError(stage)
                    return new_image()

                composed = 0

                def compose(*args, **kwargs):
                    nonlocal composed
                    composed += 1
                    if stage == ("normal_page" if composed == 1 else "reverse_page"):
                        raise RuntimeError(stage)
                    return new_image()

                saved = 0

                def save(image, path):
                    nonlocal saved
                    saved += 1
                    if stage == ("normal_save" if saved == 1 else "reverse_save"):
                        raise RuntimeError(stage)
                    save_template_png(image, path)

                provider = SimpleNamespace(fetch_map=lambda *args: new_image())
                page_exporter = SimpleNamespace(
                    map_width_px=2, map_height_px=2, default_background_provider=provider,
                    _determine_map_bounds=lambda bounds: bounds,
                    renderer=SimpleNamespace(render=render), _compose_page=compose,
                )
                exporter = SimpleNamespace(
                    _unique_raster_copy_path=lambda directory, name: directory / name,
                    _template_reference_metadata_point=lambda layer: None,
                )
                def prepare(*args, **kwargs):
                    return SimpleNamespace(image=new_image())

                with patch("SleufBase.dxf_template_pipeline_v4_patch.exporting_module._prepare_export_tiff_layer", side_effect=prepare), patch(
                    "SleufBase.dxf_template_pipeline_v4_patch.save_template_png", side_effect=save
                ):
                    kwargs = dict(
                        asset_dir=Path(temp), layer=_layer(source), label="PS1", index=1,
                        page_exporter=page_exporter, dxf_overlays=[], background_provider=provider,
                        background_attribution=None, reference_annotation=ProfileReferenceAnnotation(0, 0),
                        profile=SimpleNamespace(end_point=SimpleNamespace(x=1, y=1)),
                    )
                    if stage == "success":
                        normal, reverse, annotation = _render_normal_and_reverse_map_pair(exporter, **kwargs)
                        self.assertTrue(normal.exists())
                        self.assertTrue(reverse.exists())
                    else:
                        with self.assertRaisesRegex(RuntimeError, stage):
                            _render_normal_and_reverse_map_pair(exporter, **kwargs)
                for image in owned:
                    with self.assertRaises(ValueError):
                        image.getpixel((0, 0))
                source.getpixel((0, 0))


if __name__ == "__main__":
    unittest.main()
