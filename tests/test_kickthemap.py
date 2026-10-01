import base64
import hashlib
import json
import sqlite3
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import Mock, patch

from Crypto.Cipher import AES
import requests

from SleufBase.kickthemap import KickTheMapClient, KickTheMapJob


def _job() -> KickTheMapJob:
    return KickTheMapJob(
        job_id=12345,
        title="Example job",
        prefix="test@example.com_2026-01-01_00-00-00",
        project_mail="test@example.com",
        project_date="2026-01-01_00-00-00",
        address="",
        client_date="",
        delivery_date="",
        status=10,
        download_available=True,
        archived=0,
    )


class KickTheMapJobsPageTests(unittest.TestCase):
    def test_jobs_parser_accepts_current_javascript_declaration_styles(self) -> None:
        payload = [
            {
                "Id": 12345,
                "Prefix": "test@example.com_2026-01-01_00-00-00",
                "AboutProject": "Example job",
                "Download": True,
            }
        ]
        declarations = (
            "var projects",
            "let projects",
            "const projects",
            "window.projects",
            "const projectList",
            "let jobs",
        )

        for declaration in declarations:
            with self.subTest(declaration=declaration):
                html = f"<script>{declaration} = {json.dumps(payload)};</script>"
                jobs = KickTheMapClient()._parse_jobs_page(html)

                self.assertEqual(len(jobs), 1)
                self.assertEqual(jobs[0].job_id, 12345)
                self.assertEqual(jobs[0].title, "Example job")

    def test_jobs_parser_fetches_current_jobs_api_for_spa_page(self) -> None:
        client = KickTheMapClient()
        client._csrf_token = "csrf-token"
        response = Mock()
        response.json.return_value = {
            "status": True,
            "data": [
                {
                    "id": 12345,
                    "name": "Example job",
                    "s3_root_dir": "test@example.com_2026-01-01_00-00-00",
                    "user_id": 87,
                    "address": {
                        "road": "Example Road",
                        "number": "4",
                        "postcode": "1234 AB",
                        "town": "Exampletown",
                        "district": "Example district",
                        "region": "Example region",
                        "country": "Netherlands",
                    },
                    "created_at": "2026-01-02T03:04:05.000000Z",
                    "delivery_date": "2026-01-03T00:00:00.000000Z",
                    "status": 3,
                    "archived": 0,
                    "can_download_cloud": True,
                    "commune": "Example commune",
                    "lat": 52.3,
                    "lng": 4.9,
                }
            ],
            "cdn_base_url": "https://cdn.example.test",
        }
        client.session.post = Mock(return_value=response)
        html = '<script type="module" src="/build/assets/my-jobs-app-test.js"></script>'

        jobs = client._parse_jobs_page(html)

        self.assertEqual(len(jobs), 1)
        job = jobs[0]
        self.assertEqual(job.job_id, 12345)
        self.assertEqual(job.title, "Example job")
        self.assertEqual(job.prefix, "test@example.com_2026-01-01_00-00-00")
        self.assertEqual(job.project_mail, "test@example.com")
        self.assertEqual(job.project_date, "2026-01-01_00-00-00")
        self.assertEqual(job.user_id, "87")
        self.assertEqual(
            job.address,
            "Example Road 4, 1234 AB Exampletown, Example district, Example region, Netherlands",
        )
        self.assertEqual(job.client_date, "2026-01-02T03:04:05.000000Z")
        self.assertEqual(job.municipality, "Example commune")
        self.assertEqual(job.coordinates, "4.9, 52.3")
        self.assertTrue(job.download_available)
        response.raise_for_status.assert_called_once_with()
        client.session.post.assert_called_once_with(
            "https://www.my.kickthemap.com/jobs/get-user-jobs",
            json={},
            headers={"Accept": "application/json", "X-CSRF-TOKEN": "csrf-token"},
            timeout=client.timeout,
        )


    def test_parser_keeps_job_when_storage_prefix_format_changes(self) -> None:
        client = KickTheMapClient()
        client.logged_in_email = "test@example.com"
        jobs = client._parse_job_projects(
            [
                {
                    "id": 777,
                    "name": "Changed prefix",
                    "s3_root_dir": "new-storage-layout/job-777",
                    "created_at": "2026-09-29T12:34:56.000000Z",
                    "can_download_cloud": True,
                }
            ]
        )

        self.assertEqual(len(jobs), 1)
        self.assertEqual(jobs[0].job_id, 777)
        self.assertEqual(jobs[0].prefix, "new-storage-layout/job-777")
        self.assertEqual(jobs[0].project_mail, "test@example.com")
        self.assertEqual(jobs[0].project_date, "2026-09-29_12-34-56")
        self.assertEqual(client._job_storage_prefix(jobs[0]), "new-storage-layout/job-777")
        self.assertEqual(client.last_jobs_diagnostics["fallback_identity_count"], 1)

    def test_jobs_api_accepts_nested_paginated_shape(self) -> None:
        client = KickTheMapClient()
        client._csrf_token = "csrf-token"
        response = Mock(status_code=200)
        response.json.return_value = {
            "status": True,
            "data": {
                "data": [
                    {
                        "id": 12345,
                        "name": "Nested job",
                        "s3_root_dir": "test@example.com_2026-01-01_00-00-00",
                    }
                ],
                "current_page": 1,
            },
        }
        response.raise_for_status.return_value = None
        client.session.post = Mock(return_value=response)

        jobs = client._fetch_jobs_api()

        self.assertEqual([job.job_id for job in jobs], [12345])

    def test_jobs_api_retries_temporary_connection_failure(self) -> None:
        client = KickTheMapClient()
        client._csrf_token = "csrf-token"
        good = Mock(status_code=200)
        good.json.return_value = {
            "status": True,
            "data": [
                {
                    "id": 12345,
                    "name": "Recovered job",
                    "s3_root_dir": "test@example.com_2026-01-01_00-00-00",
                }
            ],
        }
        good.raise_for_status.return_value = None
        client.session.post = Mock(side_effect=[requests.ConnectionError("temporary"), good])

        with patch("SleufBase.kickthemap.time.sleep"):
            jobs = client._fetch_jobs_api()

        self.assertEqual(len(jobs), 1)
        self.assertEqual(client.session.post.call_count, 2)

    def test_fetch_jobs_falls_back_to_last_good_cache(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            client = KickTheMapClient()
            client.logged_in_email = "test@example.com"
            cached_job = _job()

            with patch.object(KickTheMapClient, "default_download_dir", return_value=root):
                client._write_jobs_cache([cached_job])
                with patch.object(
                    client,
                    "_fetch_jobs_with_fallback",
                    side_effect=KickTheMapClient.__mro__[1]("network failed")
                    if False
                    else requests.ConnectionError("network failed"),
                ):
                    jobs = client.fetch_jobs()

            self.assertEqual([job.job_id for job in jobs], [cached_job.job_id])
            self.assertEqual(client.last_jobs_diagnostics["source"], "cache")
            self.assertTrue(client.last_jobs_diagnostics["stale"])

    def test_nonempty_cache_requires_second_empty_response_before_clearing(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            client = KickTheMapClient()
            client.logged_in_email = "test@example.com"

            with patch.object(KickTheMapClient, "default_download_dir", return_value=root):
                client._write_jobs_cache([_job()])
                with patch.object(client, "_fetch_jobs_with_fallback", side_effect=[[], []]) as fetch:
                    jobs = client.fetch_jobs()

            self.assertEqual(jobs, [])
            self.assertEqual(fetch.call_count, 2)


class KickTheMapBrowserSessionTests(unittest.TestCase):
    @staticmethod
    def _make_profile(download_root: Path, master_key: bytes) -> None:
        profile = (
            download_root
            / "browser_sessions"
            / "test_example.com"
            / "EBWebView"
        )
        cookie_db = profile / "Default" / "Network" / "Cookies"
        cookie_db.parent.mkdir(parents=True)
        (profile / "Local State").write_text(
            json.dumps(
                {
                    "os_crypt": {
                        "encrypted_key": base64.b64encode(b"DPAPIwrapped-key").decode("ascii")
                    }
                }
            ),
            encoding="utf-8",
        )

        host_key = ".my.kickthemap.com"
        plaintext = hashlib.sha256(host_key.encode("utf-8")).digest() + b"valid-session"
        nonce = b"testnonce123"
        cipher = AES.new(master_key, AES.MODE_GCM, nonce=nonce)
        ciphertext, tag = cipher.encrypt_and_digest(plaintext)
        encrypted_cookie = b"v10" + nonce + ciphertext + tag

        connection = sqlite3.connect(cookie_db)
        try:
            connection.execute(
                "CREATE TABLE cookies ("
                "host_key TEXT, name TEXT, path TEXT, value TEXT, "
                "encrypted_value BLOB, expires_utc INTEGER, is_secure INTEGER)"
            )
            connection.execute(
                "INSERT INTO cookies VALUES (?, ?, ?, ?, ?, ?, ?)",
                (host_key, "session", "/", "", encrypted_cookie, 0, 1),
            )
            connection.commit()
        finally:
            connection.close()

    def test_saved_profile_cookies_are_decrypted_and_imported(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            download_root = Path(temporary_directory)
            master_key = b"k" * 32
            self._make_profile(download_root, master_key)
            client = KickTheMapClient()

            with patch.object(KickTheMapClient, "default_download_dir", return_value=download_root):
                with patch.object(
                    KickTheMapClient,
                    "_unprotect_chromium_data",
                    return_value=master_key,
                ):
                    imported = client._import_saved_browser_profile_cookies(
                        "test@example.com"
                    )

            self.assertTrue(imported)
            self.assertEqual(
                client.session.cookies.get(
                    "session", domain="my.kickthemap.com", path="/"
                ),
                "valid-session",
            )
            imported_cookie = next(
                cookie for cookie in client.session.cookies if cookie.name == "session"
            )
            self.assertTrue(imported_cookie.secure)

    def test_login_uses_saved_profile_before_browser_capture(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            download_root = Path(temporary_directory)
            master_key = b"m" * 32
            self._make_profile(download_root, master_key)
            client = KickTheMapClient()
            signin_response = Mock(status_code=200)
            signin_response.text = '<script src="https://www.google.com/recaptcha/api.js"></script>'
            jobs_response = Mock(status_code=200)
            jobs_response.text = (
                '<meta name="csrf-token" content="csrf-token">'
                '<script type="module" src="/build/assets/jobs.js"></script>'
            )
            api_response = Mock(status_code=200)
            api_response.json.return_value = {"status": True, "data": []}
            client.session.get = Mock(side_effect=[signin_response, jobs_response])
            client.session.post = Mock(return_value=api_response)

            with patch.object(KickTheMapClient, "default_download_dir", return_value=download_root):
                with patch.object(
                    KickTheMapClient,
                    "_unprotect_chromium_data",
                    return_value=master_key,
                ):
                    with patch.object(client, "_import_browser_session_cookies") as capture:
                        jobs = client.login("test@example.com", "saved-password")

            self.assertEqual(jobs, [])
            self.assertEqual(client.logged_in_email, "test@example.com")
            capture.assert_not_called()
            client.session.post.assert_called_once()


class KickTheMapDownloadTests(unittest.TestCase):
    def test_single_feature_download_reuses_recent_valid_disk_copy(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            target_root = Path(temporary_directory)
            target_path = target_root / "Example_job_12345_jobFeatures.json"
            target_path.write_text(json.dumps({"features": []}), encoding="utf-8")
            client = KickTheMapClient()
            client.logged_in_email = "test@example.com"

            with patch.object(client, "_download_project_file") as download_mock:
                result = client.download_job_features_file(_job(), target_root)

            self.assertEqual(result, target_path)
            download_mock.assert_not_called()

    def test_request_project_file_url_reads_current_nested_url_response(self) -> None:
        client = KickTheMapClient()
        client.logged_in_email = "test@example.com"
        client._csrf_token = "csrf-token"

        response = Mock(status_code=200)
        response.json.return_value = {
            "status": True,
            "data": {
                "url": "https://s3.example.test/signed.tiff",
                "fileName": "Example job.tiff",
            },
        }
        client.session.post = Mock(return_value=response)

        signed_url = client._request_project_file_url(
            _job(),
            folder="cloud",
            remote_file_name="test@example.com_2026-01-01_00-00-00.tiff",
            export_name="Example job.tiff",
        )

        self.assertEqual(signed_url, "https://s3.example.test/signed.tiff")
        request = client.session.post.call_args
        self.assertEqual(request.kwargs["headers"], {"X-CSRF-TOKEN": "csrf-token"})
        self.assertEqual(
            {key: value[1] for key, value in request.kwargs["files"].items()},
            {
                "projectId": "12345",
                "folder": "cloud",
                "fileName": "test@example.com_2026-01-01_00-00-00.tiff",
                "exportName": "Example job.tiff",
            },
        )

    def test_request_project_file_url_keeps_support_for_legacy_string_response(self) -> None:
        client = KickTheMapClient()
        client.logged_in_email = "test@example.com"
        client._csrf_token = "csrf-token"

        response = Mock(status_code=200)
        response.json.return_value = {
            "status": True,
            "data": "https://s3.example.test/legacy-signed.tiff",
        }
        client.session.post = Mock(return_value=response)

        signed_url = client._request_project_file_url(
            _job(),
            folder="cloud",
            remote_file_name="job.tiff",
            export_name="job.tiff",
        )

        self.assertEqual(signed_url, "https://s3.example.test/legacy-signed.tiff")

    def test_request_uses_remembered_strategy_first(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            strategy_root = Path(temporary_directory)
            with patch.object(KickTheMapClient, "default_download_dir", return_value=strategy_root):
                KickTheMapClient._save_download_strategy("form", _job())

                client = KickTheMapClient()
                client.logged_in_email = "test@example.com"
                client._csrf_token = "csrf-token"
                response = Mock(status_code=200)
                response.json.return_value = {
                    "status": True,
                    "data": {"url": "https://s3.example.test/form-signed.tiff"},
                }
                client.session.post = Mock(return_value=response)

                signed_url = client._request_project_file_url(
                    _job(),
                    folder="cloud",
                    remote_file_name="job.tiff",
                    export_name="job.tiff",
                )

                self.assertEqual(signed_url, "https://s3.example.test/form-signed.tiff")
                request = client.session.post.call_args
                self.assertIn("data", request.kwargs)
                self.assertNotIn("files", request.kwargs)

    def test_learn_download_strategy_accepts_only_a_valid_tiff(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            target_root = Path(temporary_directory)
            client = KickTheMapClient()
            client.logged_in_email = "test@example.com"

            def write_valid_tiff(_signed_url: str, target_path: Path) -> None:
                target_path.parent.mkdir(parents=True, exist_ok=True)
                target_path.write_bytes(b"II*\x00" + (b"\x00" * 32))

            with patch.object(KickTheMapClient, "default_download_dir", return_value=target_root):
                with patch.object(client, "_request_project_file_url", return_value="https://s3.example.test/sample.tiff") as request_mock:
                    with patch.object(client, "_download_url_to_file", side_effect=write_valid_tiff):
                        sample_path, request_mode = client.learn_download_strategy(_job())
                self.assertEqual(request_mode, "multipart")
                self.assertTrue(sample_path.is_file())
                self.assertEqual(request_mock.call_args.kwargs["request_mode"], "multipart")
                self.assertEqual(KickTheMapClient._load_download_strategy(), "multipart")

    def test_learn_download_strategy_falls_back_to_form_request(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            target_root = Path(temporary_directory)
            client = KickTheMapClient()
            client.logged_in_email = "test@example.com"

            def request_side_effect(*_args, **kwargs):
                if kwargs["request_mode"] == "multipart":
                    raise RuntimeError("multipart rejected")
                return "https://s3.example.test/sample.tiff"

            def write_valid_tiff(_signed_url: str, target_path: Path) -> None:
                target_path.parent.mkdir(parents=True, exist_ok=True)
                target_path.write_bytes(b"MM\x00*" + (b"\x00" * 32))

            with patch.object(KickTheMapClient, "default_download_dir", return_value=target_root):
                with patch.object(client, "_request_project_file_url", side_effect=request_side_effect) as request_mock:
                    with patch.object(client, "_download_url_to_file", side_effect=write_valid_tiff):
                        _sample_path, request_mode = client.learn_download_strategy(_job())
                self.assertEqual(request_mode, "form")
                self.assertEqual(
                    [call.kwargs["request_mode"] for call in request_mock.call_args_list],
                    ["multipart", "form"],
                )
                self.assertEqual(KickTheMapClient._load_download_strategy(), "form")

    def test_manual_capture_is_replayed_and_stored_as_a_template(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            target_root = Path(temporary_directory)
            capture_path = target_root / "capture.json"
            capture_path.write_text(
                json.dumps(
                    {
                        "status": "captured",
                        "request": {
                            "endpoint": "https://www.my.kickthemap.com/jobs/get-file-url",
                            "method": "POST",
                            "request_mode": "multipart",
                            "fields": {
                                "projectId": "12345",
                                "folder": "cloud",
                                "fileName": "test@example.com_2026-01-01_00-00-00.tiff",
                                "exportName": "Example job.tiff",
                            },
                            "response_url_path": "data.url",
                        },
                    }
                ),
                encoding="utf-8",
            )
            client = KickTheMapClient()
            client.logged_in_email = "test@example.com"
            client._csrf_token = "csrf-token"
            response = Mock(status_code=200)
            response.json.return_value = {
                "status": True,
                "data": {"url": "https://s3.example.test/captured.tiff"},
            }
            client.session.post = Mock(return_value=response)

            def write_valid_tiff(_signed_url: str, target_path: Path) -> None:
                target_path.parent.mkdir(parents=True, exist_ok=True)
                target_path.write_bytes(b"II*\x00" + (b"\x00" * 32))

            with patch.object(KickTheMapClient, "default_download_dir", return_value=target_root):
                with patch.object(client, "_download_url_to_file", side_effect=write_valid_tiff):
                    sample_path, request_mode = client.learn_download_strategy_from_capture(
                        _job(),
                        capture_path,
                    )

                self.assertEqual(request_mode, "multipart")
                self.assertTrue(sample_path.is_file())
                request = client.session.post.call_args
                self.assertEqual(
                    {key: value[1] for key, value in request.kwargs["files"].items()},
                    {
                        "projectId": "12345",
                        "folder": "cloud",
                        "fileName": "test@example.com_2026-01-01_00-00-00.tiff",
                        "exportName": "Example job.tiff",
                    },
                )
                strategy = KickTheMapClient._load_download_strategy_record()
                self.assertIsNotNone(strategy)
                self.assertEqual(strategy["endpoint_path"], "/jobs/get-file-url")
                self.assertEqual(strategy["fields"]["projectId"], "{job_id}")
                self.assertEqual(strategy["fields"]["fileName"], "{storage_prefix}.tiff")
                self.assertEqual(strategy["fields"]["exportName"], "{export_name}")


if __name__ == "__main__":
    unittest.main()
