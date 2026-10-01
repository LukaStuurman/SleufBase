from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import requests

from SleufBase.kickthemap import KickTheMapClient, KickTheMapError, _close_download_worker


class _StreamingResponse:
    def __init__(self, failure: str | None = None) -> None:
        self.failure = failure
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.closed = True

    def raise_for_status(self) -> None:
        if self.failure == "http":
            raise requests.HTTPError("download unavailable")

    def iter_content(self, chunk_size):
        yield b"new-data"
        if self.failure == "stream":
            raise requests.ConnectionError("stream interrupted")
        yield b""
        yield b"-complete"


class KickTheMapSessionLifecycleTests(unittest.TestCase):
    def make_client(self) -> KickTheMapClient:
        client = KickTheMapClient()
        self.addCleanup(client.session.close)
        client.logged_in_email = "example@example.com"
        return client

    def test_stream_download_closes_session_response_and_file(self) -> None:
        client = self.make_client()
        response = _StreamingResponse()
        session = SimpleNamespace(headers={}, get=Mock(return_value=response), close=Mock())
        opened_files = []
        original_open = Path.open

        def track_open(path, *args, **kwargs):
            handle = original_open(path, *args, **kwargs)
            opened_files.append(handle)
            return handle

        with TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "download.json"
            with patch("SleufBase.kickthemap.requests.Session", return_value=session):
                with patch.object(Path, "open", autospec=True, side_effect=track_open):
                    client._download_url_to_file("https://example.com/download", target)

            self.assertTrue(response.closed)
            session.close.assert_called_once_with()
            self.assertEqual(len(opened_files), 1)
            self.assertTrue(opened_files[0].closed)
            self.assertEqual(target.read_bytes(), b"new-data-complete")
            self.assertFalse(target.with_name("download.json.tmp").exists())
            session.get.assert_called_once_with(
                "https://example.com/download", stream=True, timeout=client.timeout
            )

    def test_http_and_stream_failures_close_resources_and_preserve_old_file(self) -> None:
        client = self.make_client()
        for failure in ("http", "stream"):
            with self.subTest(failure=failure), TemporaryDirectory() as temporary_directory:
                target = Path(temporary_directory) / "download.json"
                target.write_bytes(b"old-good-copy")
                response = _StreamingResponse(failure)
                session = SimpleNamespace(headers={}, get=Mock(return_value=response), close=Mock())

                with patch("SleufBase.kickthemap.requests.Session", return_value=session):
                    with self.assertRaises(requests.RequestException):
                        client._download_url_to_file("https://example.com/download", target)

                self.assertTrue(response.closed)
                session.close.assert_called_once_with()
                self.assertEqual(target.read_bytes(), b"old-good-copy")
                self.assertFalse(target.with_name("download.json.tmp").exists())

    def test_connection_failure_closes_session_without_creating_temp_file(self) -> None:
        client = self.make_client()
        session = SimpleNamespace(
            headers={},
            get=Mock(side_effect=requests.ConnectionError("connection failed")),
            close=Mock(),
        )
        with TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "download.json"
            with patch("SleufBase.kickthemap.requests.Session", return_value=session):
                with self.assertRaises(requests.ConnectionError):
                    client._download_url_to_file("https://example.com/download", target)

            session.close.assert_called_once_with()
            self.assertEqual(list(target.parent.iterdir()), [])

    def test_file_promotion_failure_closes_session_and_removes_temp_file(self) -> None:
        client = self.make_client()
        response = _StreamingResponse()
        session = SimpleNamespace(headers={}, get=Mock(return_value=response), close=Mock())
        with TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "download.json"
            target.write_bytes(b"old-good-copy")
            with patch("SleufBase.kickthemap.requests.Session", return_value=session):
                with patch.object(Path, "replace", side_effect=OSError("file locked")):
                    with self.assertRaises(OSError):
                        client._download_url_to_file("https://example.com/download", target)

            self.assertTrue(response.closed)
            session.close.assert_called_once_with()
            self.assertEqual(target.read_bytes(), b"old-good-copy")
            self.assertFalse(target.with_name("download.json.tmp").exists())

    def test_parallel_downloads_close_each_worker_on_success_and_error(self) -> None:
        jobs = [SimpleNamespace(job_id=number, safe_file_stem=f"job-{number}") for number in (1, 2)]
        for method_name, helper_name in (
            ("download_job_features_files", "_download_fresh_job_features"),
            ("download_tiffs", "_download_fresh_tiff"),
        ):
            with self.subTest(method=method_name), TemporaryDirectory() as temporary_directory:
                client = self.make_client()
                workers = []
                progress = []

                def make_worker():
                    worker = SimpleNamespace(session=SimpleNamespace(close=Mock()))
                    workers.append(worker)
                    return worker

                def download(worker, job, target, file_name):
                    if job.job_id == 2:
                        raise requests.ConnectionError("download interrupted")
                    target.write_bytes(b"downloaded")
                    return target

                with patch.object(client, "_parallel_worker_client", side_effect=make_worker):
                    with patch(f"SleufBase.kickthemap_scalability_patch.{helper_name}", side_effect=download):
                        with patch.object(client.session, "close") as parent_close:
                            paths, errors = getattr(client, method_name)(
                                jobs,
                                Path(temporary_directory),
                                max_workers=2,
                                progress_callback=lambda completed, total: progress.append((completed, total)),
                            )
                            parent_close.assert_not_called()

                self.assertEqual(set(paths), {1})
                self.assertEqual(set(errors), {2})
                self.assertIsInstance(errors[2], KickTheMapError)
                self.assertEqual(len(workers), 2)
                for worker in workers:
                    worker.session.close.assert_called_once_with()
                self.assertEqual(progress[-1], (2, 2))

    def test_parallel_downloads_accept_sessionless_lightweight_workers(self) -> None:
        client = self.make_client()
        job = SimpleNamespace(job_id=1, safe_file_stem="job-1")
        for method_name, helper_name in (
            ("download_job_features_files", "_download_fresh_job_features"),
            ("download_tiffs", "_download_fresh_tiff"),
        ):
            with self.subTest(method=method_name), TemporaryDirectory() as temporary_directory:
                with patch.object(client, "_parallel_worker_client", return_value=SimpleNamespace()):
                    with patch(
                        f"SleufBase.kickthemap_scalability_patch.{helper_name}",
                        side_effect=lambda worker, job, target, file_name: target,
                    ):
                        paths, errors = getattr(client, method_name)([job], Path(temporary_directory))

                self.assertEqual(set(paths), {1})
                self.assertEqual(errors, {})

    def test_parallel_worker_has_its_own_authenticated_session(self) -> None:
        client = self.make_client()
        client._csrf_token = "csrf-token"
        client.session.cookies.set("login", "value")
        client.session.headers["X-Custom"] = "custom-value"

        worker = client._parallel_worker_client()
        self.addCleanup(worker.session.close)

        self.assertIsNot(worker.session, client.session)
        self.assertEqual(worker.logged_in_email, client.logged_in_email)
        self.assertEqual(worker._csrf_token, client._csrf_token)
        self.assertEqual(worker.session.cookies.get("login"), "value")
        self.assertEqual(worker.session.headers["X-Custom"], "custom-value")
        self.assertEqual(worker.job_features_reuse_seconds, 0.0)
        with patch.object(worker.session, "close") as worker_close:
            with patch.object(client.session, "close") as parent_close:
                _close_download_worker(worker)
                worker_close.assert_called_once_with()
                parent_close.assert_not_called()

    def test_worker_initialization_failure_closes_created_session(self) -> None:
        client = self.make_client()
        session = SimpleNamespace(
            headers={},
            cookies=SimpleNamespace(update=Mock(side_effect=RuntimeError("invalid cookie state"))),
            close=Mock(),
        )
        with patch("SleufBase.kickthemap.requests.Session", return_value=session):
            with self.assertRaisesRegex(RuntimeError, "invalid cookie state"):
                client._parallel_worker_client()

        session.close.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
