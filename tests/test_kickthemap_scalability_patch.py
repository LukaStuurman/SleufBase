from __future__ import annotations

import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from SleufBase.kickthemap_scalability_patch import (
    DOWNLOAD_ATTEMPTS,
    PATCH_VERSION,
    _download_fresh_job_features,
    _refresh_template_kickthemap_snapshot,
    install_kickthemap_client_scalability_patch,
)


class _WorkerClient:
    def __init__(self, payload) -> None:
        self.payload = payload
        self.calls = 0

    @staticmethod
    def _job_features_key(job):
        return f"{job.job_id}/jobFeatures.json"

    def _download_project_file(self, job, **kwargs):
        self.calls += 1
        Path(kwargs["target_path"]).write_text(
            json.dumps(self.payload),
            encoding="utf-8",
        )


class _RefreshClient:
    is_logged_in = True

    def __init__(self, root: Path) -> None:
        self.root = root
        self.force_refresh = None
        self.max_workers = None

    def fetch_jobs(self):
        return [
            SimpleNamespace(job_id=1, safe_file_stem="ps1", title="PS 1"),
            SimpleNamespace(job_id=2, safe_file_stem="ps2", title="PS 2"),
        ]

    def default_download_dir(self):
        return self.root

    def download_job_features_files(
        self,
        jobs,
        target_dir,
        *,
        max_workers,
        progress_callback,
        force_refresh=False,
    ):
        self.force_refresh = force_refresh
        self.max_workers = max_workers
        paths = {}
        jobs = list(jobs)
        for index, job in enumerate(jobs, start=1):
            path = Path(target_dir) / f"{job.job_id}.json"
            path.write_text('{"features":[]}', encoding="utf-8")
            paths[job.job_id] = path
            if progress_callback is not None:
                progress_callback(index, len(jobs))
        return paths, {}


class KickTheMapScalabilityPatchTests(unittest.TestCase):
    def test_fresh_feature_download_replaces_cache_only_after_valid_json(self) -> None:
        job = SimpleNamespace(job_id=7)
        with TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "jobFeatures.json"
            target.write_text('{"features":[{"old":true}]}', encoding="utf-8")
            worker = _WorkerClient({"features": [{"new": True}]})

            result = _download_fresh_job_features(worker, job, target, "jobFeatures.json")

            self.assertEqual(result, target)
            payload = json.loads(target.read_text(encoding="utf-8"))
            self.assertEqual(payload["features"], [{"new": True}])
            self.assertEqual(worker.calls, 1)

    def test_invalid_fresh_feature_download_preserves_previous_good_file(self) -> None:
        job = SimpleNamespace(job_id=8)
        with TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "jobFeatures.json"
            original = '{"features":[{"keep":"old-good-copy"}]}'
            target.write_text(original, encoding="utf-8")
            worker = _WorkerClient({"wrong": []})

            with patch("SleufBase.kickthemap_scalability_patch.time.sleep", return_value=None):
                with self.assertRaises(RuntimeError):
                    _download_fresh_job_features(worker, job, target, "jobFeatures.json")

            self.assertEqual(target.read_text(encoding="utf-8"), original)
            self.assertEqual(worker.calls, DOWNLOAD_ATTEMPTS)

    def test_template_snapshot_always_requests_force_refresh(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            client = _RefreshClient(Path(temporary_directory))
            layers = [
                SimpleNamespace(metadata={"kickthemap_job_id": 1}),
                SimpleNamespace(metadata={"kickthemap_job_id": 2}),
            ]
            app = SimpleNamespace(
                kickthemap_client=client,
                _kickthemap_job_id_for_layer=lambda layer: int(layer.metadata["kickthemap_job_id"]),
            )
            progress = []

            snapshot = _refresh_template_kickthemap_snapshot(
                app,
                layers,
                progress_callback=lambda completed, total: progress.append((completed, total)),
            )

            self.assertTrue(client.force_refresh)
            self.assertEqual(set(snapshot.paths), {1, 2})
            self.assertEqual(snapshot.errors, {})
            self.assertEqual(snapshot.missing_job_ids, ())
            self.assertEqual(progress[-1], (2, 2))

    def test_client_patch_is_installed(self) -> None:
        from SleufBase.kickthemap import KickTheMapClient

        install_kickthemap_client_scalability_patch()

        self.assertEqual(PATCH_VERSION, 1)
        self.assertGreaterEqual(
            int(getattr(KickTheMapClient, "_sleufbase_scalability_patch_version", 0)),
            PATCH_VERSION,
        )


if __name__ == "__main__":
    unittest.main()
