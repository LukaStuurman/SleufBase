from __future__ import annotations

import os
import sys
from pathlib import Path


def _package_parent() -> Path:
    here = Path(__file__).resolve()
    return here.parent.parent


def _resource_root() -> Path:
    return Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))


def _ensure_package_importable() -> None:
    parent = _package_parent()
    parent_text = str(parent)
    if parent_text not in sys.path:
        sys.path.insert(0, parent_text)


def _smoke_trace(stage: str) -> None:
    """Write a best-effort phase marker for frozen CI smoke tests."""
    trace_path = os.environ.get("SLEUFBASE_SMOKE_TRACE")
    if not trace_path:
        return
    try:
        path = Path(trace_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as handle:
            handle.write(f"{stage}\n")
            handle.flush()
    except Exception:
        # Diagnostics may never be allowed to turn a successful smoke test into
        # a product failure.
        pass


def _validate_core_legacy_bytecode() -> None:
    # app.py still wraps legacy Python 3.11 bytecode because the original source
    # is not present in the repository. Resolve the bytecode relative to an
    # imported SleufBase package module: in a frozen executable the launcher is
    # extracted at the bundle root while _bytecode lives under SleufBase/.
    from SleufBase import legacy_bytecode

    legacy_bytecode.validate_legacy_bytecode("app", legacy_bytecode.__file__)


def _install_runtime_patches() -> None:
    _validate_core_legacy_bytecode()
    from SleufBase.cyclomedia_fallback import install_cyclomedia_pdok_fallback
    from SleufBase.start_point_patch import install_manual_start_point_patch
    from SleufBase.template_reverse_patch import install_template_reverse_export_patch

    install_cyclomedia_pdok_fallback()
    install_manual_start_point_patch()
    install_template_reverse_export_patch()


def _take_option(args: list[str], name: str) -> str | None:
    try:
        index = args.index(name)
    except ValueError:
        return None
    if index + 1 >= len(args):
        raise SystemExit(f"Ontbrekende waarde voor {name}")
    value = args[index + 1]
    del args[index : index + 2]
    return value


def _run_smoke_test() -> None:
    """Validate the frozen runtime without initializing the desktop GUI runtime."""
    _smoke_trace("smoke:start")
    _ensure_package_importable()
    _smoke_trace("package-path:ok")

    _install_runtime_patches()
    _smoke_trace("runtime-patches:ok")

    from SleufBase.app import KlicViewerApp
    from SleufBase.cadastral_export import CadastralDxfExporter

    _smoke_trace("app-import:ok")
    from SleufBase.cyclomedia import CyclomediaAerialClient

    _smoke_trace("cyclomedia-import:ok")
    from SleufBase import native_accel

    _smoke_trace("native-accel-import:ok")
    from SleufBase import streetsmart_browser as streetsmart_browser_module
    from SleufBase.streetsmart_bearer import bearer_authorization_header

    _smoke_trace("streetsmart-imports:ok")

    # Import pywebview only; do not initialize its GUI backend in headless CI.
    import webview

    _smoke_trace("webview-import:ok")
    webview.settings["OPEN_EXTERNAL_LINKS_IN_BROWSER"] = False
    webview.settings["SHOW_DEFAULT_MENUS"] = False

    if not getattr(CyclomediaAerialClient, "_sleufbase_pdok_fallback_installed", False):
        raise RuntimeError("Cyclomedia/PDOK luchtfoto-fallback is niet geïnstalleerd")
    if not getattr(CyclomediaAerialClient, "_sleufbase_streetsmart_bearer_retry_installed", False):
        raise RuntimeError("Cyclomedia StreetSmart-bearer retry is niet geïnstalleerd")
    if not getattr(streetsmart_browser_module, "_sleufbase_bearer_capture_installed", False):
        raise RuntimeError("StreetSmart bearer capture hook is niet geïnstalleerd")
    if bearer_authorization_header("smoke-test-token") != "Bearer smoke-test-token":
        raise RuntimeError("StreetSmart bearer Authorization-header is ongeldig")
    if not native_accel.is_available():
        raise RuntimeError("Native DXF/GeoTIFF renderer ktk_accel.dll is niet geladen")
    if not getattr(KlicViewerApp, "_manual_cross_section_start_patch", False):
        raise RuntimeError("Handmatige-beginpuntpatch is niet geïnstalleerd")
    if int(getattr(KlicViewerApp, "_sleufbase_start_point_patch_version", 0) or 0) < 2:
        raise RuntimeError("Verouderde beginpuntpatch in frozen build")
    if not callable(getattr(KlicViewerApp, "_set_automatic_template_cross_section_start_metadata", None)):
        raise RuntimeError("Automatische beginpuntsetter met handmatige voorrang ontbreekt")
    if not getattr(CadastralDxfExporter, "_sleufbase_reverse_variant_export_patch", False):
        raise RuntimeError("Normaal/reverse proefsleuf-exportpatch is niet geïnstalleerd")
    if not bool(getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_VARIANTS_DEFAULT", False)):
        raise RuntimeError("Reverse proefsleufversie staat niet standaard aan")
    _smoke_trace("runtime-validations:ok")

    resource_root = _resource_root()
    icon_path = resource_root / "assets" / "sleufbase_icon.ico"
    if not icon_path.exists():
        raise RuntimeError(f"Techbase Windows-icoon ontbreekt in frozen build: {icon_path}")

    template_path = resource_root / "assets" / "cadastral_template.dxf"
    if not template_path.exists():
        raise RuntimeError(f"Ingebouwd DXF-sjabloon ontbreekt in frozen build: {template_path}")
    if template_path.stat().st_size < 1024:
        raise RuntimeError(f"Ingebouwd DXF-sjabloon lijkt ongeldig: {template_path}")
    _smoke_trace("assets:ok")
    _smoke_trace("smoke:ok")

    # Frozen GUI/runtime libraries may own non-daemon threads. Smoke mode is a
    # process-scoped assertion mode, so exit without running unrelated GUI
    # shutdown hooks after every assertion has succeeded.
    os._exit(0)


def main() -> int:
    args = list(sys.argv[1:])

    # Keep smoke mode completely outside professional_runtime/Tk initialization.
    # This is intentionally the first executable mode so CI can validate frozen
    # imports even if the desktop runtime has a startup regression.
    if "--smoke-test" in args:
        _run_smoke_test()
        return 0

    _ensure_package_importable()

    from SleufBase.professional_runtime import (
        initialize_professional_runtime,
        install_tk_exception_handler,
        mark_startup_complete,
        write_diagnostics,
    )

    initialize_professional_runtime()

    if "--diagnostics" in args:
        write_diagnostics()
        return 0

    if "--kickthemap-jobs-browser" in args:
        from SleufBase.kickthemap_jobs_browser import main as jobs_main

        jobs_main()
        return 0

    prelogin = "--kickthemap-browser-prelogin" in args
    if prelogin:
        args.remove("--kickthemap-browser-prelogin")

    browser_url = _take_option(args, "--kickthemap-browser-url")
    browser_title = _take_option(args, "--kickthemap-browser-title")
    if prelogin or browser_url is not None or browser_title is not None:
        from SleufBase.kickthemap_browser import main as browser_main

        browser_main(start_url=browser_url, window_title=browser_title, prelogin=prelogin)
        return 0

    _install_runtime_patches()
    from SleufBase.app import KlicViewerApp

    app = KlicViewerApp()
    install_tk_exception_handler(app)

    positional_paths = [arg for arg in args if not arg.startswith("--")]
    if positional_paths:
        loader = getattr(app, "load_paths", None) or getattr(app, "open_paths", None)
        if callable(loader):
            try:
                loader(positional_paths)
            except TypeError:
                for path in positional_paths:
                    loader(path)

    try:
        app.after_idle(mark_startup_complete)
    except Exception:
        pass
    app.mainloop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
