from __future__ import annotations

"""Apply the one-time, behaviour-preserving migration scaffold to large files.

The GitHub connector intentionally writes whole text files. ``app.py`` is large,
so this script performs two exact, reviewable substitutions inside the checked-
out repository and refuses to continue if the expected legacy text has changed.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count == 0 and new in text:
        print(f"{path.name}: scaffold already applied")
        return
    if count != 1:
        raise SystemExit(
            f"{path}: verwacht exact 1 migratieanker, gevonden {count}; bestand niet aangepast"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
    print(f"{path.name}: migration scaffold applied")


def patch_app() -> None:
    path = ROOT / "app.py"
    _replace_once(path, "import marshal\n", "")
    old_loader = '''def _load_cached_module() -> None:\n    cache_tag = sys.implementation.cache_tag\n    if not cache_tag:\n        raise ImportError("Python cache tag is niet beschikbaar.")\n    pyc_path = Path(__file__).with_name("_bytecode") / f"app.{cache_tag}.pyc"\n    if not pyc_path.exists():\n        raise ImportError(f"Bytecode voor app.app niet gevonden: {pyc_path}")\n    code = marshal.loads(pyc_path.read_bytes()[16:])\n    exec(code, globals())\n'''
    new_loader = '''def _load_cached_module() -> None:\n    # Keep the historical function name while the migration is in progress so\n    # existing startup/patch order remains unchanged. Source mode is explicit\n    # and strict; normal production still uses the validated legacy loader.\n    from .source_migration import load_source_or_legacy\n\n    load_source_or_legacy("app", globals(), __file__)\n'''
    _replace_once(path, old_loader, new_loader)


def patch_launcher() -> None:
    path = ROOT / "sleufbase_launcher.py"
    old = '''def _validate_core_legacy_bytecode() -> None:\n    from SleufBase import legacy_bytecode\n\n    legacy_bytecode.validate_legacy_bytecode("app", legacy_bytecode.__file__)\n'''
    new = '''def _validate_core_legacy_bytecode() -> None:\n    from SleufBase.source_migration import source_enabled\n\n    if source_enabled("app"):\n        return\n\n    from SleufBase import legacy_bytecode\n\n    legacy_bytecode.validate_legacy_bytecode("app", legacy_bytecode.__file__)\n'''
    _replace_once(path, old, new)


def main() -> int:
    patch_app()
    patch_launcher()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
