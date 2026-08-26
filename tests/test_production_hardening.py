from __future__ import annotations

import importlib.util
import marshal
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase.legacy_bytecode import LegacyBytecodeError, load_legacy_module, read_validated_code


class LegacyBytecodeReliabilityTests(unittest.TestCase):
    def _write_pyc(self, root: Path, stem: str, payload: bytes, magic: bytes | None = None) -> Path:
        cache_tag = sys.implementation.cache_tag
        self.assertIsNotNone(cache_tag)
        directory = root / "_bytecode"
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / f"{stem}.{cache_tag}.pyc"
        path.write_bytes((magic if magic is not None else importlib.util.MAGIC_NUMBER) + (b"\0" * 12) + payload)
        return path

    def test_valid_code_is_loaded(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            code = compile("answer = 42", "legacy-test.py", "exec")
            self._write_pyc(root, "sample", marshal.dumps(code))
            namespace: dict[str, object] = {}
            load_legacy_module("sample", namespace, package_file)
            self.assertEqual(namespace["answer"], 42)

    def test_wrong_python_magic_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            code = compile("answer = 42", "legacy-test.py", "exec")
            self._write_pyc(root, "sample", marshal.dumps(code), magic=b"BAD!")
            with self.assertRaisesRegex(LegacyBytecodeError, "Python-runtime"):
                read_validated_code("sample", package_file)

    def test_truncated_pyc_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            cache_tag = sys.implementation.cache_tag
            directory = root / "_bytecode"
            directory.mkdir()
            (directory / f"sample.{cache_tag}.pyc").write_bytes(importlib.util.MAGIC_NUMBER + b"short")
            with self.assertRaisesRegex(LegacyBytecodeError, "header"):
                read_validated_code("sample", package_file)

    def test_non_code_payload_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            self._write_pyc(root, "sample", marshal.dumps({"not": "code"}))
            with self.assertRaisesRegex(LegacyBytecodeError, "codeobject"):
                read_validated_code("sample", package_file)


class ReleaseIntegrityTests(unittest.TestCase):
    def test_release_versions_are_consistent(self) -> None:
        result = subprocess.run(
            [sys.executable, str(REPO_ROOT / "tools" / "check_release_integrity.py")],
            cwd=REPO_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("0.3.0", result.stdout)

    def test_legacy_wrappers_do_not_duplicate_marshal_loader(self) -> None:
        for filename in ("settings.py", "streetsmart.py", "streetsmart_panel.py", "streetsmart_browser.py"):
            text = (REPO_ROOT / filename).read_text(encoding="utf-8")
            self.assertNotIn("marshal.loads", text, filename)
            self.assertIn("load_legacy_module", text, filename)


if __name__ == "__main__":
    unittest.main()
