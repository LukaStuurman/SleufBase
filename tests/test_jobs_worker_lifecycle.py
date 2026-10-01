from __future__ import annotations

from pathlib import Path
import threading
from types import MethodType, SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from SleufBase.kickthemap_jobs_browser import KickTheMapJobsWindow, _JobsWorkerUpdates


def _window(client=None):
    main_thread = threading.get_ident()
    messages, completed, errors, scheduled = [], [], [], []

    def set_status(message):
        if threading.get_ident() != main_thread:
            raise AssertionError("worker touched the Tk status variable")
        messages.append(message)

    def after(delay, callback):
        if threading.get_ident() != main_thread:
            raise AssertionError("worker called Tk.after")
        scheduled.append((delay, callback))
        return "poll"

    window = SimpleNamespace(
        _worker_updates=_JobsWorkerUpdates(),
        _worker_poll_after=None,
        _jobs_operation_active=False,
        account=SimpleNamespace(email="test@example.com", password="test"),
        client=client,
        status_var=SimpleNamespace(set=set_status),
        after=after,
        report_callback_exception=lambda *_args: errors.append("callback failed"),
        _set_jobs=lambda jobs: completed.append(jobs),
        _finish_load_geotiffs=lambda *args: completed.append(args),
        _show_error=errors.append,
        winfo_children=lambda: [],
        messages=messages,
        completed=completed,
        errors=errors,
        scheduled=scheduled,
    )
    for name in (
        "_worker_cancelled", "_post_worker_callback", "_post_worker_status",
        "_poll_worker_updates", "_set_controls_enabled", "_load_jobs_worker",
    ):
        setattr(window, name, MethodType(getattr(KickTheMapJobsWindow, name), window))
    return window


def _run_worker(callback):
    failures = []

    def run():
        try:
            callback()
        except Exception as exc:
            failures.append(exc)

    worker = threading.Thread(target=run)
    worker.start()
    worker.join(timeout=2)
    if worker.is_alive():
        raise AssertionError("worker did not finish")
    if failures:
        raise failures[0]


class JobsWorkerLifecycleTests(unittest.TestCase):
    def test_jobs_worker_delivers_success_and_errors_only_when_ui_polls(self):
        for failure in (None, RuntimeError("offline")):
            with self.subTest(failure=failure):
                client = SimpleNamespace(
                    is_logged_in=True,
                    fetch_jobs=Mock(return_value=["job"], side_effect=failure),
                )
                window = _window(client)

                _run_worker(window._load_jobs_worker)

                self.assertEqual(window.completed, [])
                self.assertEqual(window.errors, [])
                self.assertEqual(window.scheduled, [])
                window._poll_worker_updates()
                self.assertEqual(window.completed, [["job"]] if failure is None else [])
                self.assertEqual(window.errors, [] if failure is None else [failure])

    def test_close_discards_pending_success_error_and_future_updates(self):
        for failure in (None, RuntimeError("offline")):
            with self.subTest(failure=failure):
                window = _window(SimpleNamespace(
                    is_logged_in=True,
                    fetch_jobs=Mock(return_value=["job"], side_effect=failure),
                ))
                _run_worker(window._load_jobs_worker)
                window._post_worker_status("downloaded")
                window._worker_updates.close()
                window._post_worker_callback(lambda: window.completed.append("late result"))
                window._post_worker_status("late status")

                window._poll_worker_updates()

                self.assertEqual(window.completed, [])
                self.assertEqual(window.errors, [])
                self.assertEqual(window.messages, [])
                self.assertEqual(window.scheduled, [])
                self.assertEqual(window._worker_updates.take_updates(), (None, ()))

    def test_download_success_waits_for_main_thread_before_import(self):
        job = SimpleNamespace(job_id=1, title="PS 1")
        client = SimpleNamespace(
            is_logged_in=True,
            default_download_dir=lambda: Path("cache"),
        )

        def download_tiffs(jobs, *, max_workers, progress_callback):
            for index in range(1000):
                progress_callback(index, 1000)
            return {1: Path("1.tiff")}, {}

        def download_features(jobs, target_dir, *, max_workers, progress_callback):
            progress_callback(1, 1)
            return {1: Path("cache/1.json")}, {}

        client.download_tiffs = download_tiffs
        client.download_job_features_files = download_features
        window = _window(client)

        _run_worker(lambda: KickTheMapJobsWindow._load_selected_geotiffs_worker(window, [job]))

        self.assertEqual(window.completed, [])
        self.assertEqual(window.messages, [])
        self.assertEqual(window.scheduled, [])
        window._poll_worker_updates()
        self.assertEqual(window.messages, ["Objectpunten voorbereiden... 1 van 1"])
        self.assertEqual(len(window.completed), 1)
        self.assertEqual(window.completed[0][0], ["1.tiff"])
        self.assertEqual(window.errors, [])

    def test_close_during_tiff_download_skips_prefetch_and_import(self):
        job = SimpleNamespace(job_id=1, title="PS 1")
        client = SimpleNamespace(is_logged_in=True, download_job_features_files=Mock())
        window = _window(client)

        def download_tiffs(*args, **kwargs):
            window._worker_updates.close()
            kwargs["progress_callback"](1, 1)
            return {1: Path("1.tiff")}, {}

        client.download_tiffs = download_tiffs
        _run_worker(lambda: KickTheMapJobsWindow._load_selected_geotiffs_worker(window, [job]))
        window._poll_worker_updates()

        client.download_job_features_files.assert_not_called()
        self.assertEqual(window.completed, [])
        self.assertEqual(window.errors, [])
        self.assertEqual(window.messages, [])

    def test_close_during_feature_download_drops_import_completion(self):
        job = SimpleNamespace(job_id=1, title="PS 1")
        client = SimpleNamespace(
            is_logged_in=True,
            default_download_dir=lambda: Path("cache"),
            download_tiffs=Mock(return_value=({1: Path("1.tiff")}, {})),
        )
        window = _window(client)

        def download_features(*args, **kwargs):
            window._worker_updates.close()
            return {1: Path("cache/1.json")}, {}

        client.download_job_features_files = download_features
        _run_worker(lambda: KickTheMapJobsWindow._load_selected_geotiffs_worker(window, [job]))
        window._poll_worker_updates()

        self.assertEqual(window.completed, [])
        self.assertEqual(window.errors, [])
        self.assertEqual(window._worker_updates.take_updates(), (None, ()))

    def test_close_interrupts_manual_download_capture_wait(self):
        client = SimpleNamespace(
            is_logged_in=True,
            create_download_capture=lambda _job: Path("capture.json"),
            learn_download_strategy_from_capture=Mock(),
        )
        window = _window(client)

        def read_capture(_path):
            window._worker_updates.close()
            return {"status": "pending"}

        client.read_download_capture = read_capture
        with (
            patch("SleufBase.kickthemap_jobs_browser._job_url", return_value="https://example.com/job"),
            patch("SleufBase.kickthemap_jobs_browser._browser_launch_command", return_value=("browser", [])),
            patch("SleufBase.kickthemap_jobs_browser.subprocess.Popen"),
        ):
            _run_worker(lambda: KickTheMapJobsWindow._repair_download_process_worker(
                window, SimpleNamespace(title="PS 1"),
            ))

        client.learn_download_strategy_from_capture.assert_not_called()
        self.assertEqual(window.completed, [])
        self.assertEqual(window.errors, [])

    def test_repeated_refresh_starts_only_one_operation(self):
        window = _window()
        with patch("SleufBase.kickthemap_jobs_browser.threading.Thread") as thread:
            KickTheMapJobsWindow.refresh_jobs(window)
            KickTheMapJobsWindow.refresh_jobs(window)
            KickTheMapJobsWindow.load_selected_geotiffs(window)
            KickTheMapJobsWindow.repair_download_process(window)

        self.assertEqual(thread.call_count, 1)
        self.assertTrue(window._jobs_operation_active)
        self.assertEqual(window.messages, ["Jobs laden..."])

    def test_destroy_cancels_ui_poll_and_is_idempotent(self):
        window = object.__new__(KickTheMapJobsWindow)
        window._worker_updates = _JobsWorkerUpdates()
        window._worker_poll_after = "poll"
        window.after_cancel = Mock()
        window._post_worker_callback(Mock())

        with patch("tkinter.Tk.destroy") as destroy:
            window.destroy()
            window.destroy()

        self.assertTrue(window._worker_cancelled())
        window.after_cancel.assert_called_once_with("poll")
        destroy.assert_called_once()
        self.assertEqual(window._worker_updates.take_updates(), (None, ()))


if __name__ == "__main__":
    unittest.main()
