from __future__ import annotations

import io
import logging
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import professional_runtime
from SleufBase.secret_redaction import REDACTED_SECRET, redact_sensitive_text


class SecretRedactionTests(unittest.TestCase):
    def test_common_secret_formats_are_redacted(self) -> None:
        samples = {
            "Authorization: Bearer abcdefghijklmnop": "abcdefghijklmnop",
            "Bearer eyJhbGciOiJub25l.abcdefgh.signature": "eyJhbGciOiJub25l.abcdefgh.signature",
            "https://example.invalid/?access_token=url-secret-123&x=1": "url-secret-123",
            'password="super-secret"': "super-secret",
            "api_key=key-secret-456": "key-secret-456",
            "client_secret: secret-value-789": "secret-value-789",
        }
        for raw, secret in samples.items():
            with self.subTest(raw=raw):
                redacted = redact_sensitive_text(raw)
                self.assertNotIn(secret, redacted)
                self.assertIn(REDACTED_SECRET, redacted)

    def test_non_sensitive_diagnostic_text_is_preserved(self) -> None:
        raw = "GET https://example.invalid/tiles/12/34/56.png status=503 attempt=2"
        self.assertEqual(redact_sensitive_text(raw), raw)

    def test_logging_formatter_redacts_message_and_exception_traceback(self) -> None:
        stream = io.StringIO()
        handler = logging.StreamHandler(stream)
        handler.setFormatter(professional_runtime._RedactingFormatter("%(levelname)s | %(message)s"))
        logger = logging.getLogger("SleufBase.tests.secret-redaction")
        logger.handlers = [handler]
        logger.propagate = False
        logger.setLevel(logging.INFO)
        try:
            try:
                raise RuntimeError("Authorization: Bearer traceback-secret-123456")
            except RuntimeError:
                logger.exception("request failed password=message-secret-987654")
        finally:
            logger.handlers = []
            logger.propagate = True

        output = stream.getvalue()
        self.assertNotIn("traceback-secret-123456", output)
        self.assertNotIn("message-secret-987654", output)
        self.assertGreaterEqual(output.count(REDACTED_SECRET), 2)

    def test_crash_report_redacts_exception_secrets(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            diagnostics = Path(temp_dir)
            try:
                raise RuntimeError(
                    "request failed: https://example.invalid/api?token=crash-query-secret-123 "
                    "password=crash-password-secret-456"
                )
            except RuntimeError as exc:
                with mock.patch.object(professional_runtime, "configure_environment", return_value=None), mock.patch.object(
                    professional_runtime, "diagnostics_dir", return_value=diagnostics
                ):
                    report = professional_runtime._write_crash_report(type(exc), exc, exc.__traceback__)

            content = report.read_text(encoding="utf-8")
            self.assertNotIn("crash-query-secret-123", content)
            self.assertNotIn("crash-password-secret-456", content)
            self.assertIn(REDACTED_SECRET, content)
            self.assertIn("RuntimeError", content)


if __name__ == "__main__":
    unittest.main()
