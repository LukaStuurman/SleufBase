import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import Mock, patch

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


class KickTheMapDownloadTests(unittest.TestCase):
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
