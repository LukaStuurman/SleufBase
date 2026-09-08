from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import native_accel


class _FakeFunction:
    def __init__(self, value: int) -> None:
        self.value = value
        self.argtypes = None
        self.restype = None

    def __call__(self) -> int:
        return self.value


class _FakeLibrary:
    def __init__(self, version: int) -> None:
        self.ktk_accel_version = _FakeFunction(version)


class NativeAccelAbiTests(unittest.TestCase):
    def _probe(self, library):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "ktk_accel.dll"
            path.write_bytes(b"placeholder")
            with mock.patch.object(native_accel, "_candidate_paths", return_value=[path]), mock.patch.object(
                native_accel.cdll, "LoadLibrary", return_value=library
            ):
                return native_accel._load_library()

    def test_compatible_abi_is_accepted(self) -> None:
        library = _FakeLibrary(native_accel.EXPECTED_NATIVE_ABI_VERSION)
        loaded, version, error = self._probe(library)
        self.assertIs(loaded, library)
        self.assertEqual(version, native_accel.EXPECTED_NATIVE_ABI_VERSION)
        self.assertIsNone(error)

    def test_incompatible_abi_is_rejected_before_function_binding(self) -> None:
        library = _FakeLibrary(native_accel.EXPECTED_NATIVE_ABI_VERSION + 1)
        loaded, version, error = self._probe(library)
        self.assertIsNone(loaded)
        self.assertEqual(version, native_accel.EXPECTED_NATIVE_ABI_VERSION + 1)
        self.assertIsNotNone(error)
        self.assertIn("ABI", error or "")
        self.assertIn(str(native_accel.EXPECTED_NATIVE_ABI_VERSION), error or "")

    def test_missing_version_symbol_is_rejected(self) -> None:
        loaded, version, error = self._probe(object())
        self.assertIsNone(loaded)
        self.assertIsNone(version)
        self.assertIsNotNone(error)
        self.assertIn("ktk_accel_version", error or "")

    def test_dll_load_error_disables_native_acceleration(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "ktk_accel.dll"
            path.write_bytes(b"placeholder")
            with mock.patch.object(native_accel, "_candidate_paths", return_value=[path]), mock.patch.object(
                native_accel.cdll, "LoadLibrary", side_effect=OSError("bad image")
            ):
                loaded, version, error = native_accel._load_library()
        self.assertIsNone(loaded)
        self.assertIsNone(version)
        self.assertIn("bad image", error or "")

    def test_status_helpers_report_module_state(self) -> None:
        with mock.patch.object(native_accel, "_LIB", None), mock.patch.object(
            native_accel, "_NATIVE_ABI_VERSION", 99
        ), mock.patch.object(native_accel, "_NATIVE_LOAD_ERROR", "incompatible"):
            self.assertFalse(native_accel.is_available())
            self.assertEqual(native_accel.native_abi_version(), 99)
            self.assertEqual(native_accel.native_load_error(), "incompatible")


if __name__ == "__main__":
    unittest.main()
