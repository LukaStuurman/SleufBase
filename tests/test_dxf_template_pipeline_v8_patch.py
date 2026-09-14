from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import ezdxf

from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.dxf_template_pipeline_v8_patch import (
    _materialize_reverse_asset,
    _transfer_reverse_image_assets_v8,
)


class _PathExporter:
    def _unique_raster_copy_path(self, asset_dir: Path, filename: str) -> Path:
        candidate = Path(asset_dir) / filename
        if not candidate.exists():
            return candidate
        stem = Path(filename).stem
        suffix = Path(filename).suffix
        counter = 2
        while True:
            candidate = Path(asset_dir) / f"{stem}_{counter}{suffix}"
            if not candidate.exists():
                return candidate
            counter += 1


class DxfTemplatePipelineV8Tests(unittest.TestCase):
    def test_pipeline_v8_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v8_version", 0) or 0),
            2,
        )
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_ASSET_MATERIALIZATION", False)
        )
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_RASTER_SELF_CONTAINED", False)
        )
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_DUPLICATE_IMAGEDEF_SAFE", False)
        )

    def test_reused_normal_tiff_is_materialized_in_reverse_source_assets(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            normal_assets = root / "export_assets"
            reverse_assets = root / ".export.sleufbase-reverse-source_assets"
            normal_assets.mkdir()
            source = normal_assets / "PS1_geotiff.png"
            source.write_bytes(b"normal-raster-data")

            result = _materialize_reverse_asset(
                _PathExporter(),
                source,
                reverse_assets,
                "PS1_geotiff.png",
                counter_name="test_reverse_tiff",
            )

            self.assertEqual(result.parent, reverse_assets.resolve())
            self.assertEqual(result.name, "PS1_geotiff.png")
            self.assertTrue(result.exists())
            self.assertEqual(result.read_bytes(), b"normal-raster-data")
            self.assertTrue(source.exists(), "normal raster must remain available")

    def test_prepared_reverse_asset_is_not_duplicated(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            reverse_assets = Path(tmp) / ".export.sleufbase-reverse-source_assets"
            reverse_assets.mkdir()
            source = reverse_assets / "PS1_geotiff.png"
            source.write_bytes(b"reverse-raster-data")

            result = _materialize_reverse_asset(
                _PathExporter(),
                source,
                reverse_assets,
                "PS1_geotiff.png",
                counter_name="test_reverse_tiff",
            )

            self.assertEqual(result, source.resolve())
            self.assertEqual([path.name for path in reverse_assets.iterdir()], ["PS1_geotiff.png"])

    def test_reused_reverse_map_is_also_materialized(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            normal_assets = root / "export_assets"
            reverse_assets = root / ".export.sleufbase-reverse-source_assets"
            normal_assets.mkdir()
            source = normal_assets / ".sleufbase-reverse-map-1-123.png"
            source.write_bytes(b"reverse-map-data")

            result = _materialize_reverse_asset(
                _PathExporter(),
                source,
                reverse_assets,
                "PS1_kaart.png",
                counter_name="test_reverse_map",
            )

            self.assertEqual(result.parent, reverse_assets.resolve())
            self.assertEqual(result.name, "PS1_kaart.png")
            self.assertEqual(result.read_bytes(), b"reverse-map-data")
            self.assertTrue(source.exists())

    def test_duplicate_image_defs_can_share_moved_reverse_raster(self) -> None:
        """Regression for the exact 'Reverse rasterbestand ontbreekt' failure."""

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reverse_source = root / ".export.sleufbase-reverse-source.dxf"
            reverse_assets = reverse_patch._source_asset_dir(reverse_source)
            reverse_assets.mkdir()
            source = reverse_assets / "PS1_geotiff.png"
            source.write_bytes(b"reverse-raster-data")
            final_output = root / "export.dxf"

            document = ezdxf.new("R2010")
            block = document.blocks.new(
                name=reverse_patch.variant_block_name(
                    "PS1",
                    1,
                    reverse_patch.REVERSE_MODE,
                )
            )
            first_def = document.add_image_def(
                filename=str(source.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_TIFF_REVERSE_A",
            )
            second_def = document.add_image_def(
                filename=str(source.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_TIFF_REVERSE_B",
            )
            block.add_image(
                insert=(0.0, 0.0),
                size_in_units=(8.0, 6.0),
                image_def=first_def,
            )
            block.add_image(
                insert=(10.0, 0.0),
                size_in_units=(8.0, 6.0),
                image_def=second_def,
            )

            target_dir = _transfer_reverse_image_assets_v8(
                document,
                reverse_source,
                final_output,
            )
            destination = (target_dir / "PS1_geotiff.png").resolve()

            self.assertFalse(source.exists(), "temporary raster is moved on first use")
            self.assertTrue(destination.exists())
            self.assertEqual(destination.read_bytes(), b"reverse-raster-data")
            self.assertEqual(Path(first_def.dxf.filename), destination)
            self.assertEqual(Path(second_def.dxf.filename), destination)


if __name__ == "__main__":
    unittest.main()
