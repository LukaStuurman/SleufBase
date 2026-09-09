from __future__ import annotations

from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ezdxf

from SleufBase import dxf_template_pipeline_patch as pipeline
from SleufBase import dxf_template_pipeline_v3_patch as v3
from SleufBase import template_reverse_patch as reverse_patch


class _TailLayout:
    def __init__(self, values):
        self.values = list(values)
        self.iterated = False

    def __len__(self):
        return len(self.values)

    def __getitem__(self, index):
        return self.values[index]

    def __iter__(self):
        self.iterated = True
        raise AssertionError("tail fast path should not scan the whole layout")


class DxfTemplatePipelineV3Tests(unittest.TestCase):
    def tearDown(self) -> None:
        for name in (
            "pair_cache",
            "capture_reverse_save",
            "capture_reverse_path",
        ):
            reverse_patch._EXPORT_CONTEXT.__dict__.pop(name, None)

    def test_new_modelspace_entities_uses_tail_slice_without_second_full_scan(self) -> None:
        old_a = object()
        old_b = object()
        new_a = object()
        new_b = object()
        layout = _TailLayout([old_a, old_b, new_a, new_b])
        before_ids = {id(old_a), id(old_b)}

        result = v3._new_modelspace_entities_tail(layout, before_ids)

        self.assertEqual(result, [new_a, new_b])
        self.assertFalse(layout.iterated)

    def test_record_indexes_are_reused_only_inside_scoped_context(self) -> None:
        records = [[(0, "EOF")]]
        sections = [None]
        calls = []

        def fake_indexes(current_records, current_sections):
            calls.append((current_records, current_sections))
            return ({}, {}, {}, {})

        with patch.object(v3, "_ORIGINAL_RECORD_INDEXES", side_effect=fake_indexes):
            with v3._reuse_record_indexes():
                first = v3._record_indexes_v3(records, sections)
                second = v3._record_indexes_v3(records, sections)
            third = v3._record_indexes_v3(records, sections)

        self.assertIs(first, second)
        self.assertEqual(third, ({}, {}, {}, {}))
        self.assertEqual(len(calls), 2)

    def test_reverse_saveas_is_captured_in_memory_without_creating_dxf(self) -> None:
        v3._install_reverse_save_capture()
        with tempfile.TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "reverse.dxf"
            cache = {"values": {}, "phases": {}, "counters": {}}
            reverse_patch._EXPORT_CONTEXT.pair_cache = cache
            reverse_patch._EXPORT_CONTEXT.capture_reverse_save = True
            reverse_patch._EXPORT_CONTEXT.capture_reverse_path = target

            document = ezdxf.new("R2018")
            document.modelspace().add_line((0, 0), (1, 1))
            document.saveas(target)

            self.assertFalse(target.exists())
            self.assertIs(cache.get("reverse_document"), document)
            self.assertEqual(Path(cache.get("reverse_document_path")), target)
            self.assertGreaterEqual(cache["counters"].get("reverse_dxf_writes_skipped", 0), 1)

    def test_temporary_reverse_asset_is_moved_instead_of_copied(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            temporary_root = root / "reverse_assets"
            temporary_root.mkdir()
            source = temporary_root / "profile.png"
            source.write_bytes(b"profile")
            destination = root / "final_assets" / "profile.png"

            mode = v3._relocate_reverse_asset(source, destination, temporary_root)

            self.assertEqual(mode, "moved")
            self.assertFalse(source.exists())
            self.assertEqual(destination.read_bytes(), b"profile")

    def test_shared_reverse_asset_keeps_source_and_reuses_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            temporary_root = root / "temporary"
            temporary_root.mkdir()
            shared_root = root / "shared"
            shared_root.mkdir()
            source = shared_root / "map.png"
            source.write_bytes(b"map")
            destination = root / "final" / "map.png"

            mode = v3._relocate_reverse_asset(source, destination, temporary_root)

            self.assertIn(mode, {"linked", "copied"})
            self.assertTrue(source.exists())
            self.assertEqual(destination.read_bytes(), b"map")

    def test_v3_marker_is_installed_on_exporter(self) -> None:
        from SleufBase.cadastral_export import CadastralDxfExporter

        v3.install_dxf_template_pipeline_v3_patch()
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v3_version", 0)),
            1,
        )
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_REVERSE_DXF_IN_MEMORY)


if __name__ == "__main__":
    unittest.main()
