from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
from types import CodeType, ModuleType
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTRACT = REPO_ROOT / "tests" / "characterization" / "legacy_bytecode_contract.json"


def _load_legacy_bytecode_helper() -> ModuleType:
    helper_path = REPO_ROOT / "legacy_bytecode.py"
    spec = importlib.util.spec_from_file_location("_sleufbase_legacy_bytecode_snapshot", helper_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Kan legacy bytecode-helper niet laden: {helper_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _git_blob_sha(data: bytes) -> str:
    header = f"blob {len(data)}\0".encode("ascii")
    return hashlib.sha1(header + data).hexdigest()


def _code_object_snapshot(code: CodeType) -> dict[str, Any]:
    nested: list[dict[str, Any]] = []
    for value in code.co_consts:
        if isinstance(value, CodeType):
            nested.append(_code_object_snapshot(value))
    nested.sort(key=lambda item: (item["firstlineno"], item["qualname"], item["name"]))
    return {
        "name": code.co_name,
        "qualname": getattr(code, "co_qualname", code.co_name),
        "firstlineno": code.co_firstlineno,
        "argcount": code.co_argcount,
        "posonlyargcount": getattr(code, "co_posonlyargcount", 0),
        "kwonlyargcount": code.co_kwonlyargcount,
        "names": sorted(set(code.co_names)),
        "nested": nested,
    }


def _load_contract(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict) or payload.get("schema_version") != 1:
        raise RuntimeError(f"Ongeldig legacy bytecode-contract: {path}")
    modules = payload.get("modules")
    if not isinstance(modules, dict) or not modules:
        raise RuntimeError(f"Legacy bytecode-contract bevat geen modules: {path}")
    return payload


def build_snapshot(contract_path: Path = DEFAULT_CONTRACT) -> dict[str, Any]:
    contract = _load_contract(contract_path)
    expected_cache_tag = str(contract.get("python_cache_tag") or "")
    actual_cache_tag = str(sys.implementation.cache_tag or "")
    if expected_cache_tag and actual_cache_tag != expected_cache_tag:
        raise RuntimeError(
            f"Characterization vereist {expected_cache_tag}, huidige runtime is {actual_cache_tag or 'onbekend'}."
        )

    helper = _load_legacy_bytecode_helper()
    modules_snapshot: dict[str, Any] = {}
    for module_stem, expected in sorted(contract["modules"].items()):
        if not isinstance(expected, dict):
            raise RuntimeError(f"Ongeldig contract voor module {module_stem!r}.")
        relative_path = Path(str(expected.get("path") or ""))
        bytecode_path = REPO_ROOT / relative_path
        data = bytecode_path.read_bytes()

        expected_size = int(expected.get("size", -1))
        if len(data) != expected_size:
            raise RuntimeError(
                f"Bytecodegrootte gewijzigd voor {module_stem}: {len(data)} != {expected_size}. "
                "Werk het contract alleen bewust bij na characterization."
            )

        actual_blob_sha = _git_blob_sha(data)
        expected_blob_sha = str(expected.get("git_blob_sha") or "")
        if actual_blob_sha != expected_blob_sha:
            raise RuntimeError(
                f"Bytecode-inhoud gewijzigd voor {module_stem}: {actual_blob_sha} != {expected_blob_sha}. "
                "Werk het contract alleen bewust bij na characterization."
            )

        code = helper.read_validated_code(module_stem, REPO_ROOT / "legacy_bytecode.py")
        modules_snapshot[module_stem] = {
            "path": relative_path.as_posix(),
            "size": len(data),
            "git_blob_sha": actual_blob_sha,
            "sha256": hashlib.sha256(data).hexdigest(),
            "code": _code_object_snapshot(code),
        }

    return {
        "schema_version": 1,
        "python_cache_tag": actual_cache_tag,
        "modules": modules_snapshot,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate the frozen legacy bytecode contract and optionally write a static API/code-object snapshot."
    )
    parser.add_argument("--contract", type=Path, default=DEFAULT_CONTRACT)
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--check", action="store_true", help="Validate the frozen bytecode contract.")
    args = parser.parse_args()

    snapshot = build_snapshot(args.contract)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(snapshot, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"Legacy API snapshot geschreven: {args.output}")
    elif not args.check:
        print(json.dumps(snapshot, ensure_ascii=False, indent=2))

    if args.check:
        print(f"Legacy bytecode-contract OK: {len(snapshot['modules'])} modules ({snapshot['python_cache_tag']}).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
