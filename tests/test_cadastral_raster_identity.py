from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import ezdxf
from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform


class CadastralRasterIdentityTests(unittest.TestCase):
    @staticmethod
    def _layer(path: Path, color: tuple[int, int, int]) -> GeoTiffLayer:
        return GeoTiffLayer(
            path=path,
            image=Image.new("RGBA", (4, 3), (*color, 255)),
            transform=GeoTransform(1.0, 0.0, 100.0, 0.0, -1.0, 200.0),
            bounds=Bounds(100.0, 197.0, 104.0, 200.0),
            epsg=28992,
        )

    def test_ps_number_is_read_after_phase_number_and_underscore(self) -> None:
        exporter = CadastralDxfExporter(None)
        layer = self._layer(Path("nederweert_fase_2_ps_21_66587.tiff"), (1, 2, 3))

        self.assertEqual(exporter._proefsleuf_base_name(layer, 1), "PS21")

        layer.image.close()

    def test_ordinary_rasters_get_stable_unique_slot_names(self) -> None:
        exporter = CadastralDxfExporter(None)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            sources = [
                root / "nederweert_fase_2_ps_20_66526.tiff",
                root / "nederweert_fase_2_ps_21_66587.tiff",
                root / "other_fase_2_ps_21.tiff",
            ]
            layers = [
                self._layer(sources[0], (20, 0, 0)),
                self._layer(sources[1], (21, 0, 0)),
                self._layer(sources[2], (22, 0, 0)),
            ]
            for layer in layers:
                layer.image.save(layer.path, format="TIFF")

            output = root / "Nederweert.dxf"
            first = exporter._prepare_tiff_raster_files(output, layers)
            second = exporter._prepare_tiff_raster_files(output, layers)

            expected_names = [
                "PS20__slot001.tiff",
                "PS21__slot002.tiff",
                "PS21__slot003.tiff",
            ]
            self.assertEqual([item.raster_path.name for item in first], expected_names)
            self.assertEqual([item.raster_path.name for item in second], expected_names)
            self.assertEqual(len({item.raster_path for item in first}), len(layers))
            self.assertEqual(
                [item.raster_path.read_bytes() for item in first],
                [layer.path.read_bytes() for layer in layers],
            )

            for layer, prepared in zip(layers, first):
                layer.image.close()
                prepared.layer.image.close()

    def test_each_image_entity_keeps_its_own_image_definition_file(self) -> None:
        exporter = CadastralDxfExporter(None)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            layers = [
                self._layer(root / "nederweert_fase_2_ps_20_66526.tiff", (20, 0, 0)),
                self._layer(root / "nederweert_fase_2_ps_21_66587.tiff", (21, 0, 0)),
            ]
            for layer in layers:
                layer.image.save(layer.path, format="TIFF")

            prepared = exporter._prepare_tiff_raster_files(root / "Nederweert.dxf", layers)
            document = ezdxf.new("R2018", setup=True)
            modelspace = document.modelspace()
            for index, item in enumerate(prepared, start=1):
                exporter._add_tiff_image(
                    document,
                    modelspace,
                    item.layer,
                    item.raster_path,
                    index,
                )

            images = list(modelspace.query("IMAGE"))
            filenames = [Path(image.image_def.dxf.filename) for image in images]
            image_def_names = sorted(document.rootdict.get_required_dict("ACAD_IMAGE_DICT").keys())

            self.assertEqual(len(images), 2)
            self.assertEqual([path.name for path in filenames], [
                "PS20__slot001.tiff",
                "PS21__slot002.tiff",
            ])
            self.assertEqual(len(set(filenames)), len(filenames))
            self.assertEqual(image_def_names, [
                "PS20__slot001.tiff",
                "PS21__slot002.tiff",
            ])
            self.assertTrue(all(path.exists() for path in filenames))

            for layer in layers:
                layer.image.close()


if __name__ == "__main__":
    unittest.main()
