from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.dxf_template_pipeline_v8_patch import _materialize_reverse_asset


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
            1,
        )
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_ASSET_MATERIALIZATION", False)
        )
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_RASTER_SELF_CONTAINED", False)
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


if __name__ == "__main__":
    unittest.main()
