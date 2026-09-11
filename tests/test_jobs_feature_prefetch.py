from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
import unittest

from SleufBase.kickthemap_jobs_browser import KickTheMapJobsWindow


class _Status:
    def __init__(self) -> None:
        self.messages: list[str] = []

    def set(self, message: str) -> None:
        self.messages.append(message)


class _Client:
    is_logged_in = True

    def __init__(self) -> None:
        self.feature_jobs = []

    @staticmethod
    def default_download_dir() -> Path:
        return Path("cache")

    def download_tiffs(self, jobs, *, max_workers, progress_callback):
        progress_callback(len(jobs), len(jobs))
        return ({job.job_id: Path(f"{job.job_id}.tiff") for job in jobs}, {})

    def download_job_features_files(self, jobs, target_dir, *, max_workers, progress_callback):
        self.feature_jobs = list(jobs)
        progress_callback(len(self.feature_jobs), len(self.feature_jobs))
        return ({job.job_id: Path(target_dir) / f"{job.job_id}_jobFeatures.json" for job in jobs}, {})


class JobsFeaturePrefetchTests(unittest.TestCase):
    def test_successful_tiff_jobs_prefetch_features_before_handoff(self) -> None:
        jobs = [SimpleNamespace(job_id=1, title="PS 1"), SimpleNamespace(job_id=2, title="PS 2")]
        client = _Client()
        finished = []
        errors = []
        window = SimpleNamespace(
            account=None,
            client=client,
            status_var=_Status(),
            after=lambda _delay, callback: callback(),
            _finish_load_geotiffs=lambda paths, warnings, records: finished.append(
                (paths, warnings, records)
            ),
            _show_error=lambda error: errors.append(error),
        )

        KickTheMapJobsWindow._load_selected_geotiffs_worker(window, jobs)

        self.assertEqual(client.feature_jobs, jobs)
        self.assertEqual(errors, [])
        self.assertEqual(len(finished), 1)
        paths, warnings, records = finished[0]
        self.assertEqual(paths, ["1.tiff", "2.tiff"])
        self.assertEqual(warnings, [])
        self.assertEqual(set(records), {"1", "2"})
        self.assertTrue(
            any("Objectpunten voorbereiden" in message for message in window.status_var.messages)
        )


if __name__ == "__main__":
    unittest.main()
