from __future__ import annotations

import json
import time
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest.mock import patch

from SleufBase.geotiff import load_geotiff
from SleufBase.kickthemap_jobs_browser import KickTheMapJobsWindow
from SleufBase.models import Bounds, GeoTransform


class LoadedJobsFilterTests(unittest.TestCase):
    def test_active_state_returns_only_fresh_positive_job_ids(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            state_path = Path(temporary_directory) / "active_loaded_jobs.json"
            state_path.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "updated_at": time.time(),
                        "job_ids": [12, "34", 0, -1, "invalid"],
                    }
                ),
                encoding="utf-8",
            )
            window = SimpleNamespace(_active_loaded_jobs_state_path=lambda: state_path)

            result = KickTheMapJobsWindow._read_active_loaded_job_ids(window)

            self.assertEqual(result, {"12", "34"})

    def test_stale_active_state_is_not_treated_as_loaded(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            state_path = Path(temporary_directory) / "active_loaded_jobs.json"
            state_path.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "updated_at": time.time() - 60,
                        "job_ids": [12],
                    }
                ),
                encoding="utf-8",
            )
            window = SimpleNamespace(_active_loaded_jobs_state_path=lambda: state_path)

            result = KickTheMapJobsWindow._read_active_loaded_job_ids(window)

            self.assertEqual(result, set())

    def test_refresh_loaded_job_ids_intersects_active_layers_with_current_jobs(self) -> None:
        window = SimpleNamespace(
            jobs=[SimpleNamespace(job_id=1), SimpleNamespace(job_id=2)],
            loaded_job_ids={"999"},
            _read_active_loaded_job_ids=lambda: {"2", "3"},
        )

        KickTheMapJobsWindow._refresh_loaded_job_ids(window)

        self.assertEqual(window.loaded_job_ids, {"2"})


class KickTheMapSidecarTests(unittest.TestCase):
    def test_geotiff_loader_imports_kickthemap_sidecar_metadata(self) -> None:
        class FakeImage:
            width = 10
            height = 10
            tag_v2 = {}

            def close(self) -> None:
                pass

        with TemporaryDirectory() as temporary_directory:
            path = Path(temporary_directory) / "Example_12345.tiff"
            sidecar = path.with_suffix(path.suffix + ".job.json")
            sidecar.write_text(
                json.dumps(
                    {
                        "job_id": "12345",
                        "title": "Example trench",
                        "path": str(path),
                    }
                ),
                encoding="utf-8",
            )
            transform = GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 10.0)
            with (
                patch("SleufBase.geotiff.Image.open", return_value=FakeImage()),
                patch("SleufBase.geotiff._build_transform", return_value=transform),
                patch("SleufBase.geotiff._calculate_bounds", return_value=Bounds(0.0, 0.0, 10.0, 10.0)),
                patch("SleufBase.geotiff._parse_epsg", return_value=28992),
            ):
                layer = load_geotiff(path)

            self.assertEqual(layer.metadata["kickthemap_job_id"], 12345)
            self.assertEqual(layer.metadata["kickthemap_job_title"], "Example trench")
            self.assertEqual(layer.metadata["kickthemap_job_sidecar"], str(sidecar))


if __name__ == "__main__":
    unittest.main()
