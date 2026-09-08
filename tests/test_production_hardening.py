from __future__ import annotations

import hashlib
import importlib.util
import json
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

from SleufBase.legacy_bytecode import (
    LegacyBytecodeError,
    load_legacy_module,
    read_validated_code,
    validate_legacy_bytecode,
)


class LegacyBytecodeReliabilityTests(unittest.TestCase):
    def _write_manifest(self, root: Path, stem: str, path: Path, data: bytes) -> None:
        manifest = {
            "schema_version": 1,
            "hash_algorithm": "sha256",
            "modules": {
                stem: {
                    "filename": path.name,
                    "size": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                }
            },
        }
        (root / "_bytecode" / "legacy_bytecode_manifest.json").write_text(
            json.dumps(manifest, indent=2) + "\n",
            encoding="utf-8",
        )

    def _write_pyc(self, root: Path, stem: str, payload: bytes, magic: bytes | None = None) -> Path:
        cache_tag = sys.implementation.cache_tag
        self.assertIsNotNone(cache_tag)
        directory = root / "_bytecode"
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / f"{stem}.{cache_tag}.pyc"
        data = (magic if magic is not None else importlib.util.MAGIC_NUMBER) + (b"\0" * 12) + payload
        path.write_bytes(data)
        self._write_manifest(root, stem, path, data)
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
            path = directory / f"sample.{cache_tag}.pyc"
            data = importlib.util.MAGIC_NUMBER + b"short"
            path.write_bytes(data)
            self._write_manifest(root, "sample", path, data)
            with self.assertRaisesRegex(LegacyBytecodeError, "bytecodegrootte|header"):
                read_validated_code("sample", package_file)

    def test_non_code_payload_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            self._write_pyc(root, "sample", marshal.dumps({"not": "code"}))
            with self.assertRaisesRegex(LegacyBytecodeError, "codeobject"):
                read_validated_code("sample", package_file)

    def test_tampered_bytecode_is_rejected_before_execution(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            code = compile("answer = 42", "legacy-test.py", "exec")
            path = self._write_pyc(root, "sample", marshal.dumps(code))
            data = bytearray(path.read_bytes())
            data[-1] ^= 0x01
            path.write_bytes(data)
            namespace: dict[str, object] = {}
            with self.assertRaisesRegex(LegacyBytecodeError, "SHA-256"):
                load_legacy_module("sample", namespace, package_file)
            self.assertNotIn("answer", namespace)

    def test_missing_integrity_manifest_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_file = root / "wrapper.py"
            package_file.write_text("", encoding="utf-8")
            cache_tag = sys.implementation.cache_tag
            directory = root / "_bytecode"
            directory.mkdir()
            code = compile("answer = 42", "legacy-test.py", "exec")
            (directory / f"sample.{cache_tag}.pyc").write_bytes(
                importlib.util.MAGIC_NUMBER + (b"\0" * 12) + marshal.dumps(code)
            )
            with self.assertRaisesRegex(LegacyBytecodeError, "integriteitsmanifest ontbreekt"):
                read_validated_code("sample", package_file)

    def test_repository_manifest_validates_all_frozen_modules(self) -> None:
        for module in ("app", "settings", "streetsmart", "streetsmart_browser", "streetsmart_panel"):
            with self.subTest(module=module):
                path = validate_legacy_bytecode(module, REPO_ROOT / f"{module}.py")
                self.assertTrue(path.is_file())


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
        self.assertIn("Release-integriteit OK: SleufBase ", result.stdout)

    def test_legacy_wrappers_use_central_migration_switch(self) -> None:
        forbidden_loader_token = "marshal" + ".loads"
        for filename in (
            "app.py",
            "settings.py",
            "streetsmart.py",
            "streetsmart_panel.py",
            "streetsmart_browser.py",
        ):
            text = (REPO_ROOT / filename).read_text(encoding="utf-8")
            self.assertNotIn(forbidden_loader_token, text, filename)
            self.assertNotIn("load_legacy_module", text, filename)
            self.assertIn("load_migrating_module", text, filename)


if __name__ == "__main__":
    unittest.main()
