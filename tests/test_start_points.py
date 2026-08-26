from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import start_point_patch as start_points


@dataclass
class FakeDataset:
    cross_section_start_xy: tuple[float, float] | None = None


class FakeBounds:
    def padded(self, _padding: float):
        return self


class FakeLayer:
    def __init__(self, path: Path, *, name: str = "proefsleuf.tif", job_id: int = 42) -> None:
        self.path = path
        self.name = name
        self.bounds = FakeBounds()
        self.metadata = {"kickthemap_job_id": job_id}


class FakeViewer:
    def __init__(self, layers: list[FakeLayer]) -> None:
        self.tiff_layers = layers
        self.status_messages: list[str] = []
        self.refresh_count = 0
        self.render_count = 0
        self.original_all_load_calls = 0

    def _set_template_cross_section_start_metadata(
        self, layer: FakeLayer, start_x: float, start_y: float
    ) -> None:
        layer.metadata[start_points.START_POINT_KEY] = (float(start_x), float(start_y))

    def load_all_kickthemap_start_points(self) -> None:
        self.original_all_load_calls += 1
        for layer in self.tiff_layers:
            self._set_template_cross_section_start_metadata(layer, 100.0, 200.0)

    def _load_local_maaiveld_dataset_for_layer(self, _layer: FakeLayer) -> FakeDataset:
        return FakeDataset()

    def _load_maaiveld_dataset_for_layer(self, _layer: FakeLayer) -> FakeDataset:
        return FakeDataset()

    def _save_project_to_path(self, path: str | Path) -> None:
        Path(path).write_text(
            json.dumps({"existing_project_data": {"keep": True}}),
            encoding="utf-8",
        )

    def load_project(self, path: str | Path) -> None:
        for layer in self.tiff_layers:
            layer.metadata[start_points.START_POINT_KEY] = (999.0, 999.0)
            layer.metadata[start_points.START_POINT_SOURCE_KEY] = (
                start_points.START_POINT_SOURCE_AUTOMATIC
            )

    def clear_all_reference_points(self) -> None:
        for layer in self.tiff_layers:
            layer.metadata.pop(start_points.START_POINT_KEY, None)

    def _refresh_map_edit_markers(self) -> None:
        self.refresh_count += 1

    def request_render(self, *, immediate: bool = False) -> None:
        self.render_count += 1

    def set_status(self, message: str) -> None:
        self.status_messages.append(message)

    def _is_virtual_trench_layer(self, _layer: FakeLayer) -> bool:
        return False

    def _kickthemap_job_id_for_layer(self, layer: FakeLayer) -> int | None:
        return int(layer.metadata["kickthemap_job_id"])


start_points._patch_viewer_class(FakeViewer)


class StartPointPriorityTests(unittest.TestCase):
    def test_manual_click_is_authoritative_over_load_all(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            layer = FakeLayer(Path(temp_dir) / "manual.tif")
            viewer = FakeViewer([layer])

            viewer._set_template_cross_section_start_metadata(layer, 10.0, 20.0)
            viewer.load_all_kickthemap_start_points()

            self.assertEqual(
                layer.metadata[start_points.START_POINT_KEY],
                (10.0, 20.0),
            )
            self.assertEqual(
                layer.metadata[start_points.START_POINT_SOURCE_KEY],
                start_points.START_POINT_SOURCE_MANUAL,
            )
            self.assertTrue(layer.metadata[start_points.MANUAL_START_POINT_KEY])
            self.assertEqual(viewer.original_all_load_calls, 0)

    def test_direct_automatic_setter_cannot_replace_manual_point(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            layer = FakeLayer(Path(temp_dir) / "manual.tif")
            viewer = FakeViewer([layer])
            viewer._set_template_cross_section_start_metadata(layer, 15.0, 25.0)

            viewer._set_automatic_template_cross_section_start_metadata(
                layer, 150.0, 250.0
            )

            self.assertEqual(
                layer.metadata[start_points.START_POINT_KEY],
                (15.0, 25.0),
            )
            self.assertEqual(
                layer.metadata[start_points.START_POINT_SOURCE_KEY],
                start_points.START_POINT_SOURCE_MANUAL,
            )

    def test_automatic_load_is_marked_automatic_when_no_manual_point_exists(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            layer = FakeLayer(Path(temp_dir) / "automatic.tif")
            viewer = FakeViewer([layer])

            viewer.load_all_kickthemap_start_points()

            self.assertEqual(
                layer.metadata[start_points.START_POINT_KEY],
                (100.0, 200.0),
            )
            self.assertEqual(
                layer.metadata[start_points.START_POINT_SOURCE_KEY],
                start_points.START_POINT_SOURCE_AUTOMATIC,
            )
            self.assertNotIn(start_points.MANUAL_START_POINT_KEY, layer.metadata)
            self.assertEqual(viewer.original_all_load_calls, 1)

    def test_second_manual_click_immediately_replaces_persisted_point(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            project_path = root / "project.json"
            layer = FakeLayer(root / "manual.tif")
            viewer = FakeViewer([layer])

            viewer._save_project_to_path(project_path)
            viewer._set_template_cross_section_start_metadata(layer, 1.0, 2.0)
            viewer._set_template_cross_section_start_metadata(layer, 3.0, 4.0)

            payload = json.loads(project_path.read_text(encoding="utf-8"))
            self.assertTrue(payload["existing_project_data"]["keep"])
            state = payload[start_points.PROJECT_START_POINTS_KEY]
            self.assertEqual(state["version"], start_points.PROJECT_START_POINTS_VERSION)
            self.assertEqual(state["manual"][0]["xy"], [3.0, 4.0])
            self.assertEqual(state["manual"][0]["source"], "manual")

    def test_project_save_reinjects_manual_state_after_legacy_serializer_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            project_path = root / "project.json"
            layer = FakeLayer(root / "manual.tif")
            viewer = FakeViewer([layer])
            viewer._set_template_cross_section_start_metadata(layer, 11.0, 22.0)

            viewer._save_project_to_path(project_path)

            payload = json.loads(project_path.read_text(encoding="utf-8"))
            self.assertTrue(payload["existing_project_data"]["keep"])
            self.assertEqual(
                payload[start_points.PROJECT_START_POINTS_KEY]["manual"][0]["xy"],
                [11.0, 22.0],
            )

    def test_project_load_restores_manual_point_after_legacy_auto_value(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            project_path = root / "project.json"
            layer = FakeLayer(root / "manual.tif")
            viewer = FakeViewer([layer])
            viewer._set_template_cross_section_start_metadata(layer, 12.5, 24.5)
            viewer._save_project_to_path(project_path)

            reloaded_layer = FakeLayer(root / "manual.tif")
            reloaded = FakeViewer([reloaded_layer])
            reloaded.load_project(project_path)

            self.assertEqual(
                reloaded_layer.metadata[start_points.START_POINT_KEY],
                (12.5, 24.5),
            )
            self.assertEqual(
                reloaded_layer.metadata[start_points.START_POINT_SOURCE_KEY],
                start_points.START_POINT_SOURCE_MANUAL,
            )
            self.assertTrue(
                reloaded_layer.metadata[start_points.MANUAL_START_POINT_KEY]
            )

    def test_loading_legacy_project_tracks_path_for_next_manual_click(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            project_path = root / "legacy.json"
            project_path.write_text(json.dumps({"legacy": True}), encoding="utf-8")
            layer = FakeLayer(root / "manual.tif")
            viewer = FakeViewer([layer])

            viewer.load_project(project_path)
            viewer._set_template_cross_section_start_metadata(layer, 7.0, 8.0)

            payload = json.loads(project_path.read_text(encoding="utf-8"))
            self.assertEqual(
                payload[start_points.PROJECT_START_POINTS_KEY]["manual"][0]["xy"],
                [7.0, 8.0],
            )

    def test_dataset_always_uses_current_layer_start_point(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            layer = FakeLayer(Path(temp_dir) / "manual.tif")
            viewer = FakeViewer([layer])
            viewer._set_template_cross_section_start_metadata(layer, 31.0, 41.0)

            dataset = viewer._load_local_maaiveld_dataset_for_layer(layer)

            self.assertEqual(dataset.cross_section_start_xy, (31.0, 41.0))

    def test_clear_removes_persisted_manual_state(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            project_path = root / "project.json"
            layer = FakeLayer(root / "manual.tif")
            viewer = FakeViewer([layer])
            viewer._save_project_to_path(project_path)
            viewer._set_template_cross_section_start_metadata(layer, 5.0, 6.0)

            viewer.clear_all_reference_points()

            payload = json.loads(project_path.read_text(encoding="utf-8"))
            self.assertEqual(
                payload[start_points.PROJECT_START_POINTS_KEY]["manual"],
                [],
            )
            self.assertNotIn(start_points.MANUAL_START_POINT_KEY, layer.metadata)
            self.assertNotIn(start_points.START_POINT_SOURCE_KEY, layer.metadata)

    def test_fast_load_exception_uses_safe_fallback_instead_of_escaping(self) -> None:
        class FailingViewer(FakeViewer):
            def load_all_kickthemap_start_points(self) -> None:
                raise RuntimeError("simulated fast loader failure")

        start_points._patch_viewer_class(FailingViewer)
        with tempfile.TemporaryDirectory() as temp_dir:
            layer = FakeLayer(Path(temp_dir) / "automatic.tif")
            viewer = FailingViewer([layer])
            fallback_calls: list[object] = []

            with patch.object(
                start_points,
                "_safe_load_all_start_points",
                side_effect=lambda app: fallback_calls.append(app),
            ):
                viewer.load_all_kickthemap_start_points()

            self.assertEqual(fallback_calls, [viewer])


if __name__ == "__main__":
    unittest.main()
