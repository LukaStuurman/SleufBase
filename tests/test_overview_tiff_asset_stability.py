from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory
import threading
import unittest
from unittest.mock import patch

import ezdxf
from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform


class OverviewTiffAssetStabilityTests(unittest.TestCase):
    def setUp(self) -> None:
        self.exporter = CadastralDxfExporter(object())

    def _layers(self, root: Path, *, save_sources: bool = True):
        layers = []
        for index, color in enumerate(("red", "green", "blue", "yellow")):
            source = root / f"source-{index}" / "PS2.tiff"
            source.parent.mkdir()
            image = Image.new("RGB", (8, 6), color)
            self.addCleanup(image.close)
            if save_sources:
                image.save(source, format="TIFF")
            layers.append(GeoTiffLayer(
                path=source,
                image=image,
                transform=GeoTransform(1, 0, index * 10, 0, -1, 6),
                bounds=Bounds(index * 10, 0, index * 10 + 8, 6),
                epsg=28992,
                metadata={"template_proefsleuf_label": "PS2"},
            ))
        return layers

    def test_duplicate_labels_copy_in_parallel_to_distinct_preallocated_paths(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            layers = self._layers(root)
            barrier = threading.Barrier(4)
            copy_destinations = []
            allocation_threads = []
            original_allocate = self.exporter._unique_raster_copy_path
            from SleufBase.cadastral_export import shutil
            original_copy = shutil.copy2

            def allocate(*args):
                allocation_threads.append(threading.get_ident())
                return original_allocate(*args)

            def copy(source, destination):
                copy_destinations.append(Path(destination))
                barrier.wait(timeout=5)
                return original_copy(source, destination)

            with patch.object(self.exporter, "_unique_raster_copy_path", side_effect=allocate):
                with patch("SleufBase.cadastral_export.shutil.copy2", side_effect=copy):
                    prepared = self.exporter._prepare_tiff_raster_files(root / "overview.dxf", layers)

            self.assertEqual(len(set(copy_destinations)), 4)
            self.assertEqual(allocation_threads, [threading.get_ident()] * 4)
            self.assertEqual([item.layer for item in prepared], layers)
            for item, layer in zip(prepared, layers):
                self.assertEqual(item.raster_path.read_bytes(), layer.path.read_bytes())
                self.assertEqual(self.exporter._proefsleuf_label(layer, 1), "PS2")

    def test_reexport_preserves_every_existing_asset(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            layers = self._layers(root)
            asset_dir = root / "overview_tiffs"
            asset_dir.mkdir()
            old_paths = [asset_dir / "PS2.tiff"] + [
                asset_dir / f"PS2__slot{index:03d}.tiff" for index in range(1, 5)
            ]
            for index, path in enumerate(old_paths):
                path.write_bytes(f"old-raster-{index}".encode())
            previous_bytes = {path: path.read_bytes() for path in old_paths}

            first_export = self.exporter._prepare_tiff_raster_files(root / "overview.dxf", layers)
            first_bytes = {item.raster_path: item.raster_path.read_bytes() for item in first_export}
            second_export = self.exporter._prepare_tiff_raster_files(root / "overview.dxf", layers)

            self.assertEqual({path: path.read_bytes() for path in old_paths}, previous_bytes)
            self.assertEqual({path: path.read_bytes() for path in first_bytes}, first_bytes)
            self.assertTrue(set(first_bytes).isdisjoint(item.raster_path for item in second_export))
            for item, layer in zip(second_export, layers):
                self.assertEqual(item.raster_path.read_bytes(), layer.path.read_bytes())

    def test_missing_source_rasters_save_to_distinct_paths_under_four_workers(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            layers = self._layers(root, save_sources=False)
            barrier = threading.Barrier(4)
            original_save = Image.Image.save

            def save(image, destination, *args, **kwargs):
                barrier.wait(timeout=5)
                return original_save(image, destination, *args, **kwargs)

            with patch.object(Image.Image, "save", autospec=True, side_effect=save):
                prepared = self.exporter._prepare_tiff_raster_files(root / "overview.dxf", layers)

            self.assertEqual(len({item.raster_path for item in prepared}), 4)
            for item, layer in zip(prepared, layers):
                with Image.open(item.raster_path) as raster:
                    self.assertEqual(raster.getpixel((0, 0)), layer.image.getpixel((0, 0)))

    def test_production_overview_writer_keeps_distinct_raster_references_and_visible_labels(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            layers = self._layers(root)
            output = root / "overview.dxf"

            self.exporter._write_dxf(
                output, layers, [], [], Bounds(0, 0, 38, 6),
                trench_mode=self.exporter.TRENCH_MODE_NONE,
                include_tiff_images=True,
                label_gap=0.0,
                centerline_color=(0, 0, 0),
                label_color=(0, 0, 0),
            )

            document = ezdxf.readfile(output)
            images = list(document.modelspace().query("IMAGE"))
            self.assertEqual(len(images), 4)
            linked_files = [Path(image.image_def.dxf.filename) for image in images]
            self.assertEqual(len(set(linked_files)), 4)
            self.assertEqual([path.read_bytes() for path in linked_files], [layer.path.read_bytes() for layer in layers])
            self.assertEqual(set(document.rootdict.get_required_dict("ACAD_IMAGE_DICT").keys()), {
                "PS2__slot001.tiff", "PS2__slot002.tiff", "PS2__slot003.tiff", "PS2__slot004.tiff",
            })
            self.assertTrue(all(self.exporter._proefsleuf_label(layer, index) == "PS2"
                                for index, layer in enumerate(layers, start=1)))


if __name__ == "__main__":
    unittest.main()
