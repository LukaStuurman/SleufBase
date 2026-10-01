from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import ezdxf

from SleufBase import dxf_template_pipeline_patch as pipeline
from SleufBase import template_dynamic_visibility_patch as dynamic_patch
from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.dxf_template_pipeline_v8_patch import (
    _relocate_reverse_asset_preserving_existing,
    _transfer_reverse_image_assets_v8,
)


def _add_variant_image(document, source: Path, mode: str, *, slot: int = 1):
    image_def = document.add_image_def(
        filename=str(source.resolve()), size_in_pixel=(8, 6), name=f"PS{slot}_TIFF"
    )
    image = document.modelspace().add_image(
        insert=(slot * 10, 0), size_in_units=(8, 6), image_def=image_def
    )
    reverse_patch._move_entities_to_variant_container(
        document,
        document.modelspace(),
        [image],
        label=f"PS{slot}",
        slot_index=slot,
        mode=mode,
    )
    return image_def


class DxfReverseAssetPreservationTests(unittest.TestCase):
    def tearDown(self) -> None:
        reverse_patch._EXPORT_CONTEXT.__dict__.pop("pair_cache", None)

    def test_distinct_sources_with_same_basename_keep_their_own_image_data(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            first = root / "first" / "map.png"
            second = root / "second" / "map.png"
            first.parent.mkdir()
            second.parent.mkdir()
            first.write_bytes(b"first-map")
            second.write_bytes(b"second-map")
            document = ezdxf.new("R2018")
            first_def = _add_variant_image(document, first, reverse_patch.REVERSE_MODE, slot=1)
            second_def = _add_variant_image(document, second, reverse_patch.REVERSE_MODE, slot=2)

            _transfer_reverse_image_assets_v8(
                document, root / ".export.reverse.dxf", root / "export.dxf"
            )

            first_link = Path(first_def.dxf.filename)
            second_link = Path(second_def.dxf.filename)
            self.assertNotEqual(first_link, second_link)
            self.assertEqual(first_link.read_bytes(), b"first-map")
            self.assertEqual(second_link.read_bytes(), b"second-map")
            self.assertEqual(first.read_bytes(), b"first-map")
            self.assertEqual(second.read_bytes(), b"second-map")

    def test_copy_fallback_preserves_existing_destination(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            source = root / "new" / "map.png"
            target_dir = root / "export_reverse_assets"
            source.parent.mkdir()
            target_dir.mkdir()
            source.write_bytes(b"new-map")
            old_asset = target_dir / source.name
            old_asset.write_bytes(b"old-good-map")

            with patch("SleufBase.dxf_template_pipeline_v8_patch.os.link", side_effect=OSError("links unavailable")):
                destination, mode = _relocate_reverse_asset_preserving_existing(
                    source, target_dir, root / "temporary"
                )

            self.assertEqual(mode, "copied")
            self.assertNotEqual(destination, old_asset)
            self.assertEqual(destination.read_bytes(), b"new-map")
            self.assertEqual(old_asset.read_bytes(), b"old-good-map")

    def test_failed_copy_removes_only_its_new_partial_destination(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            source = root / "new" / "map.png"
            target_dir = root / "export_reverse_assets"
            source.parent.mkdir()
            target_dir.mkdir()
            source.write_bytes(b"new-map")
            old_asset = target_dir / source.name
            old_asset.write_bytes(b"old-good-map")

            def interrupted_copy(source_handle, destination_handle, **kwargs):
                destination_handle.write(b"partial")
                raise OSError("disk full")

            with patch("SleufBase.dxf_template_pipeline_v8_patch.os.link", side_effect=OSError("links unavailable")):
                with patch("SleufBase.dxf_template_pipeline_v8_patch.shutil.copyfileobj", side_effect=interrupted_copy):
                    with self.assertRaisesRegex(OSError, "disk full"):
                        _relocate_reverse_asset_preserving_existing(source, target_dir, root / "temporary")

            self.assertEqual(old_asset.read_bytes(), b"old-good-map")
            self.assertEqual(list(target_dir.iterdir()), [old_asset])

    def test_parallel_allocation_never_overwrites_an_existing_asset(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            source = root / "source" / "map.png"
            target_dir = root / "export_reverse_assets"
            source.parent.mkdir()
            target_dir.mkdir()
            source.write_bytes(b"new-map")
            old_asset = target_dir / source.name
            old_asset.write_bytes(b"old-good-map")

            with ThreadPoolExecutor(max_workers=4) as executor:
                futures = [executor.submit(
                    _relocate_reverse_asset_preserving_existing, source, target_dir, root / "temporary"
                ) for _ in range(8)]
                destinations = [future.result()[0] for future in futures]

            self.assertEqual(len(set(destinations)), 8)
            self.assertTrue(all(path.read_bytes() == b"new-map" for path in destinations))
            self.assertEqual(old_asset.read_bytes(), b"old-good-map")

    def _prepare_reexport(self, root: Path):
        output = root / "export.dxf"
        normal_dir = root / "export_assets"
        reverse_dir = root / "export_reverse_assets"
        normal_dir.mkdir()
        reverse_dir.mkdir()
        old_normal = normal_dir / "PS1_geotiff.png"
        old_reverse = reverse_dir / "PS1_geotiff.png"
        old_normal.write_bytes(b"old-normal-raster")
        old_reverse.write_bytes(b"old-reverse-raster")
        previous = ezdxf.new("R2018")
        _add_variant_image(previous, old_normal, reverse_patch.NORMAL_MODE)
        _add_variant_image(previous, old_reverse, reverse_patch.REVERSE_MODE)
        previous.saveas(output)
        original_dxf_bytes = output.read_bytes()

        exporter = CadastralDxfExporter(SimpleNamespace())
        new_normal = exporter._unique_raster_copy_path(normal_dir, old_normal.name)
        new_normal.write_bytes(b"new-normal-raster")
        normal = ezdxf.readfile(Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf")
        _add_variant_image(normal, new_normal, reverse_patch.NORMAL_MODE)
        reverse_source = reverse_patch._reverse_source_path(output)
        reverse_source_dir = reverse_patch._source_asset_dir(reverse_source)
        reverse_source_dir.mkdir()
        new_reverse = reverse_source_dir / old_reverse.name
        new_reverse.write_bytes(b"new-reverse-raster")
        reverse = ezdxf.new("R2018")
        _add_variant_image(reverse, new_reverse, reverse_patch.REVERSE_MODE)
        reverse_patch._EXPORT_CONTEXT.pair_cache = {
            "normal_document": normal,
            "normal_document_path": output,
            "reverse_document": reverse,
            "reverse_document_path": reverse_source,
            "values": {}, "phases": {}, "counters": {}, "exporter": exporter,
        }
        return output, reverse_source, old_normal, old_reverse, original_dxf_bytes

    def test_validation_and_final_replace_failure_preserve_previous_dxf_and_all_linked_rasters(self) -> None:
        for failure in ("validation", "replace"):
            with self.subTest(failure=failure), TemporaryDirectory() as temporary_directory:
                root = Path(temporary_directory)
                output, reverse_source, old_normal, old_reverse, old_dxf = self._prepare_reexport(root)
                original_replace = Path.replace

                def fail_final_replace(path, target):
                    if Path(target) == output:
                        raise OSError("drawing is open in CAD")
                    return original_replace(path, target)

                if failure == "validation":
                    failure_patch = patch.object(pipeline, "_validate_dynamic_details", side_effect=RuntimeError("invalid visibility"))
                else:
                    failure_patch = patch.object(Path, "replace", autospec=True, side_effect=fail_final_replace)
                with failure_patch:
                    with self.assertRaisesRegex((RuntimeError, OSError), "invalid visibility|drawing is open"):
                        pipeline._merge_reverse_variant_document_one_read(output, reverse_source)
                        pipeline._promote_exported_variants_one_pass(output)
                reverse_patch._cleanup_reverse_source(reverse_source)

                self.assertEqual(output.read_bytes(), old_dxf)
                self.assertEqual(old_normal.read_bytes(), b"old-normal-raster")
                self.assertEqual(old_reverse.read_bytes(), b"old-reverse-raster")
                self.assertFalse(dynamic_patch._working_dynamic_path(output).exists())
                self.assertFalse(reverse_patch._source_asset_dir(reverse_source).exists())
                old_document = ezdxf.readfile(output)
                linked_files = {Path(image.image_def.dxf.filename) for block in old_document.blocks for image in block.query("IMAGE")}
                self.assertEqual(linked_files, {old_normal.resolve(), old_reverse.resolve()})

    def test_successful_reexport_links_new_rasters_and_preserves_previous_assets(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            output, reverse_source, old_normal, old_reverse, _old_dxf = self._prepare_reexport(root)

            self.assertEqual(pipeline._merge_reverse_variant_document_one_read(output, reverse_source), 1)
            pipeline._promote_exported_variants_one_pass(output)
            reverse_patch._cleanup_reverse_source(reverse_source)

            document = ezdxf.readfile(output)
            linked_files = {Path(image.image_def.dxf.filename) for block in document.blocks for image in block.query("IMAGE")}
            self.assertEqual({path.read_bytes() for path in linked_files}, {b"new-normal-raster", b"new-reverse-raster"})
            self.assertNotIn(old_normal.resolve(), linked_files)
            self.assertNotIn(old_reverse.resolve(), linked_files)
            self.assertEqual(old_normal.read_bytes(), b"old-normal-raster")
            self.assertEqual(old_reverse.read_bytes(), b"old-reverse-raster")


if __name__ == "__main__":
    unittest.main()
