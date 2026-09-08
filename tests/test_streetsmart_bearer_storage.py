from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import streetsmart_bearer


class StreetSmartBearerStorageTests(unittest.TestCase):
    def test_windows_save_writes_only_dpapi_protected_token(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "streetsmart_bearer.json"
            with mock.patch.object(streetsmart_bearer, "_token_path", return_value=path), mock.patch.object(
                streetsmart_bearer.os, "name", "nt"
            ), mock.patch.object(
                streetsmart_bearer, "_dpapi_protect_text", return_value="encrypted-base64-value"
            ), mock.patch.object(streetsmart_bearer.time, "time", return_value=1000.0):
                saved = streetsmart_bearer.save_streetsmart_bearer_token("Bearer plaintext-secret-token")

            self.assertTrue(saved)
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(payload["schema_version"], 2)
            self.assertEqual(payload["protection"], "dpapi-current-user")
            self.assertEqual(payload["token_protected"], "encrypted-base64-value")
            self.assertNotIn("token", payload)
            self.assertNotIn("plaintext-secret-token", path.read_text(encoding="utf-8"))

    def test_windows_load_decrypts_dpapi_payload(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "streetsmart_bearer.json"
            path.write_text(
                json.dumps(
                    {
                        "schema_version": 2,
                        "protection": "dpapi-current-user",
                        "token_protected": "encrypted-value",
                        "captured_at": 1000.0,
                        "expires_at": None,
                        "permissions": ["atlas"],
                    }
                ),
                encoding="utf-8",
            )
            with mock.patch.object(streetsmart_bearer, "_token_path", return_value=path), mock.patch.object(
                streetsmart_bearer.os, "name", "nt"
            ), mock.patch.object(
                streetsmart_bearer, "_dpapi_unprotect_text", return_value="decrypted-token"
            ), mock.patch.object(streetsmart_bearer.time, "time", return_value=1100.0):
                token = streetsmart_bearer.load_streetsmart_bearer_token()

            self.assertEqual(token, "decrypted-token")

    def test_plaintext_windows_payload_is_migrated_without_extending_age(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "streetsmart_bearer.json"
            legacy = {
                "token": "legacy-plaintext-token",
                "captured_at": 1000.0,
                "expires_at": 5000.0,
                "permissions": {"atlas": True},
            }
            path.write_text(json.dumps(legacy), encoding="utf-8")
            with mock.patch.object(streetsmart_bearer, "_token_path", return_value=path), mock.patch.object(
                streetsmart_bearer.os, "name", "nt"
            ), mock.patch.object(
                streetsmart_bearer, "_dpapi_protect_text", return_value="migrated-protected-value"
            ), mock.patch.object(streetsmart_bearer.time, "time", return_value=1100.0):
                token = streetsmart_bearer.load_streetsmart_bearer_token()

            self.assertEqual(token, "legacy-plaintext-token")
            migrated = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(migrated["captured_at"], 1000.0)
            self.assertEqual(migrated["expires_at"], 5000.0)
            self.assertEqual(migrated["protection"], "dpapi-current-user")
            self.assertEqual(migrated["token_protected"], "migrated-protected-value")
            self.assertNotIn("token", migrated)

    def test_dpapi_failure_never_falls_back_to_plaintext_on_windows(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "streetsmart_bearer.json"
            with mock.patch.object(streetsmart_bearer, "_token_path", return_value=path), mock.patch.object(
                streetsmart_bearer.os, "name", "nt"
            ), mock.patch.object(
                streetsmart_bearer, "_dpapi_protect_text", side_effect=OSError("DPAPI unavailable")
            ):
                saved = streetsmart_bearer.save_streetsmart_bearer_token("must-not-be-plaintext")

            self.assertFalse(saved)
            self.assertFalse(path.exists())

    def test_non_windows_storage_remains_available_for_development(self) -> None:
        if os.name == "nt":
            self.skipTest("Non-Windows fallback test")
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "streetsmart_bearer.json"
            with mock.patch.object(streetsmart_bearer, "_token_path", return_value=path), mock.patch.object(
                streetsmart_bearer.time, "time", return_value=1000.0
            ):
                self.assertTrue(streetsmart_bearer.save_streetsmart_bearer_token("dev-token"))
                token = streetsmart_bearer.load_streetsmart_bearer_token()

            self.assertEqual(token, "dev-token")
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(payload["protection"], "plaintext-nonwindows")

    @unittest.skipUnless(os.name == "nt", "Windows DPAPI roundtrip")
    def test_real_windows_dpapi_roundtrip(self) -> None:
        protected = streetsmart_bearer._dpapi_protect_text("roundtrip-secret-token")
        self.assertNotIn("roundtrip-secret-token", protected)
        self.assertEqual(streetsmart_bearer._dpapi_unprotect_text(protected), "roundtrip-secret-token")


if __name__ == "__main__":
    unittest.main()
