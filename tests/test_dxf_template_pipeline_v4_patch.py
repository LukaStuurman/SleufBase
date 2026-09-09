from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from PIL import Image, ImageDraw

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.dxf_template_pipeline_v4_patch import (
    _annotation_key,
    _copy_or_rotate_reverse_tiff,
    _profile_vector_relation,
    _render_normal_and_reverse_map_pair,
    _reverse_reference_annotation,
)
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform, ProfileReferenceAnnotation


class _PathExporter:
    def _unique_raster_copy_path(self, asset_dir: Path, filename: str) -> Path:
        return Path(asset_dir) / filename


class _AnnotationExporter(_PathExporter):
    def __init__(self, metadata_point=None) -> None:
        self.metadata_point = metadata_point

    def _template_reference_metadata_point(self, _layer):
        return self.metadata_point

    @staticmethod
    def _rotate_template_map_point(x, y, _cx, _cy, _rotation):
        return float(x), float(y)


class _Provider:
    def __init__(self) -> None:
        self.calls = 0

    def fetch_map(self, _bounds, size):
        self.calls += 1
        return Image.new("RGBA", size, (10, 20, 30, 255))


class _Renderer:
    def __init__(self) -> None:
        self.calls = 0

    def render(self, _bounds, size, _tiffs, _overlays, *, background=None, map_comments=None):
        self.calls += 1
        assert background is not None
        assert map_comments is None
        return Image.new("RGBA", size, (40, 50, 60, 255))


class _PageExporter:
    def __init__(self) -> None:
        self.map_width_px = 20
        self.map_height_px = 12
        self.default_background_provider = _Provider()
        self.renderer = _Renderer()

    def _determine_map_bounds(self, _bounds):
        return Bounds(0.0, 0.0, 20.0, 12.0)

    def _compose_page(self, map_image, _map_bounds, *, background_attribution=None, reference_annotation=None):
        assert background_attribution == "test"
        page = map_image.copy()
        if reference_annotation is not None:
            draw = ImageDraw.Draw(page)
            x = max(0, min(page.width - 1, int(round(reference_annotation.x))))
            y = max(0, min(page.height - 1, int(round(reference_annotation.y))))
            draw.point((x, y), fill=(255, 255, 255, 255))
        return page


class DxfTemplatePipelineV4Tests(unittest.TestCase):
    def test_pipeline_v4_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v4_version", 0) or 0),
            1,
        )
        self.assertTrue(getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_RASTER_REUSE", False))

    def test_profile_vectors_detect_exact_reverse(self) -> None:
        self.assertEqual(_profile_vector_relation((2.0, 0.0), (-5.0, 0.0)), "reverse")
        self.assertEqual(_profile_vector_relation((2.0, 0.0), (5.0, 0.0)), "same")
        self.assertIsNone(_profile_vector_relation((1.0, 0.0), (0.0, 1.0)))

    def test_reverse_reference_annotation_uses_normal_profile_end(self) -> None:
        exporter = _AnnotationExporter()
        layer = SimpleNamespace(
            metadata={},
            image=Image.new("RGBA", (10, 10)),
            transform=SimpleNamespace(pixel_to_world=lambda _x, _y: (0.0, 0.0)),
        )
        profile = SimpleNamespace(end_point=SimpleNamespace(x=8.5, y=4.25))
        result = _reverse_reference_annotation(
            exporter,
            layer,
            profile,
            ProfileReferenceAnnotation(x=1.0, y=2.0),
        )
        self.assertEqual(_annotation_key(result), (8.5, 4.25))
        layer.image.close()

    def test_explicit_reference_annotation_stays_identical(self) -> None:
        exporter = _AnnotationExporter(metadata_point=(100.0, 200.0))
        layer = SimpleNamespace(metadata={})
        current = ProfileReferenceAnnotation(x=3.0, y=7.0)
        result = _reverse_reference_annotation(exporter, layer, None, current)
        self.assertEqual(_annotation_key(result), _annotation_key(current))

    def test_reverse_tiff_is_exact_180_degree_pixel_transform(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "normal.png"
            image = Image.new("RGBA", (3, 2))
            values = [
                (1, 0, 0, 255),
                (2, 0, 0, 255),
                (3, 0, 0, 255),
                (4, 0, 0, 255),
                (5, 0, 0, 255),
                (6, 0, 0, 255),
            ]
            image.putdata(values)
            image.save(source)
            image.close()

            result = _copy_or_rotate_reverse_tiff(
                _PathExporter(),
                source,
                root,
                "PS1",
                rotate_180=True,
            )
            with Image.open(result) as rotated:
                self.assertEqual(list(rotated.getdata()), list(reversed(values)))

    def test_normal_and_reverse_map_share_one_expensive_render(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            layer_image = Image.new("RGBA", (8, 6), (100, 100, 100, 255))
            layer = GeoTiffLayer(
                path=root / "ps1.tif",
                image=layer_image,
                transform=GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 6.0),
                bounds=Bounds(0.0, 0.0, 8.0, 6.0),
                epsg=28992,
                opacity=1.0,
                metadata={},
            )
            page_exporter = _PageExporter()
            exporter = _AnnotationExporter()
            profile = SimpleNamespace(end_point=SimpleNamespace(x=15.0, y=7.0))

            normal_path, reverse_path, reverse_annotation = _render_normal_and_reverse_map_pair(
                exporter,
                asset_dir=root,
                layer=layer,
                label="PS1",
                index=1,
                page_exporter=page_exporter,
                dxf_overlays=[],
                background_provider=page_exporter.default_background_provider,
                background_attribution="test",
                reference_annotation=ProfileReferenceAnnotation(x=2.0, y=3.0),
                profile=profile,
            )

            self.assertTrue(normal_path.exists())
            self.assertIsNotNone(reverse_path)
            assert reverse_path is not None
            self.assertTrue(reverse_path.exists())
            self.assertEqual(page_exporter.default_background_provider.calls, 1)
            self.assertEqual(page_exporter.renderer.calls, 1)
            self.assertEqual(_annotation_key(reverse_annotation), (15.0, 7.0))
            with Image.open(normal_path) as normal, Image.open(reverse_path) as reverse:
                self.assertNotEqual(normal.getpixel((2, 3)), reverse.getpixel((2, 3)))
                self.assertEqual(reverse.getpixel((15, 7)), (255, 255, 255, 255))
            layer_image.close()


if __name__ == "__main__":
    unittest.main()
