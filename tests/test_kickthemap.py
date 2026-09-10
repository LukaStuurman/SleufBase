import unittest
from unittest.mock import Mock

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


if __name__ == "__main__":
    unittest.main()
