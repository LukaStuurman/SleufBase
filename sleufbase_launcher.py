from __future__ import annotations

import sys
from pathlib import Path


def _package_parent() -> Path:
    here = Path(__file__).resolve()
    return here.parent.parent


def _ensure_package_importable() -> None:
    parent = _package_parent()
    parent_text = str(parent)
    if parent_text not in sys.path:
        sys.path.insert(0, parent_text)


def _install_runtime_patches() -> None:
    # A StreetSmart account may be valid for the viewer while lacking separate
    # Cyclomedia Aerial WMS Basic-auth access. The aerial chain is therefore:
    # normal Cyclomedia WMS -> documented StreetSmart bearer -> public PDOK HR.
    from SleufBase.cyclomedia_fallback import install_cyclomedia_pdok_fallback

    install_cyclomedia_pdok_fallback()


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


def main() -> int:
    _ensure_package_importable()
    _install_runtime_patches()
    args = list(sys.argv[1:])

    # CI/runtime smoke test: import the complete app module without constructing
    # the Tk GUI and validate the frozen browser/auth patches used at runtime.
    if "--smoke-test" in args:
        from SleufBase.app import KlicViewerApp  # noqa: F401
        from SleufBase.cyclomedia import CyclomediaAerialClient
        from SleufBase import streetsmart_browser as streetsmart_browser_module
        from SleufBase.streetsmart_bearer import (
            bearer_authorization_header,
            load_streetsmart_bearer_token,  # noqa: F401
        )
        import webview

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

    from SleufBase.app import KlicViewerApp

    app = KlicViewerApp()

    # Positional arguments are file paths used by SleufBase itself when a frozen
    # process starts a second instance. Pass them to the app when the runtime
    # exposes the normal loader helper; otherwise start the UI normally.
    positional_paths = [arg for arg in args if not arg.startswith("--")]
    if positional_paths:
        loader = getattr(app, "load_paths", None) or getattr(app, "open_paths", None)
        if callable(loader):
            try:
                loader(positional_paths)
            except TypeError:
                for path in positional_paths:
                    loader(path)

    app.mainloop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
