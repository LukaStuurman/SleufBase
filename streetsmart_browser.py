from __future__ import annotations

import marshal
import sys
import threading
import time
from pathlib import Path


def _load_cached_module() -> None:
    cache_tag = sys.implementation.cache_tag
    if not cache_tag:
        raise ImportError("Python cache tag is niet beschikbaar.")
    pyc_path = Path(__file__).with_name("_bytecode") / f"streetsmart_browser.{cache_tag}.pyc"
    if not pyc_path.exists():
        raise ImportError(f"Bytecode voor app.streetsmart_browser niet gevonden: {pyc_path}")
    code = marshal.loads(pyc_path.read_bytes()[16:])
    exec(code, globals())


_load_cached_module()


# StreetSmart exposes getBearerToken() as part of its documented JavaScript API.
# Capture that token after a successful normal StreetSmart login so the aerial
# client can retry Cyclomedia Atlas with exactly the permissions of the logged-in
# StreetSmart user. We deliberately do not inspect cookies, localStorage or
# network requests.
from .streetsmart_bearer import save_streetsmart_bearer_token


def _capture_documented_streetsmart_bearer(window) -> bool:
    try:
        result = window.evaluate_js(
            r"""
            (() => {
              try {
                const api = (typeof StreetSmartApi !== 'undefined' && StreetSmartApi)
                  || window.StreetSmartApi
                  || window.streetSmartApi;
                if (!api || typeof api.getBearerToken !== 'function') {
                  return null;
                }
                const token = api.getBearerToken();
                if (!token || typeof token !== 'string') {
                  return null;
                }
                let permissions = null;
                try {
                  if (typeof api.getPermissions === 'function') {
                    permissions = api.getPermissions();
                  }
                } catch (_permissionError) {}
                return { token, permissions };
              } catch (_error) {
                return null;
              }
            })()
            """
        )
    except Exception:
        return False

    if not isinstance(result, dict):
        return False
    token = str(result.get("token") or "").strip()
    if not token:
        return False
    return save_streetsmart_bearer_token(token, result.get("permissions"))


def _schedule_bearer_capture(window) -> None:
    def worker() -> None:
        # The page-loaded event can fire before StreetSmart's API has restored
        # its authenticated state. Retry briefly without blocking the browser UI.
        for delay in (0.15, 0.5, 1.0, 2.0, 4.0):
            time.sleep(delay)
            if _capture_documented_streetsmart_bearer(window):
                return

    threading.Thread(
        target=worker,
        name="streetsmart-bearer-capture",
        daemon=True,
    ).start()


_original_inject_browser_script = globals().get("_inject_browser_script")
if callable(_original_inject_browser_script):
    def _inject_browser_script(*args, **kwargs):
        result = _original_inject_browser_script(*args, **kwargs)
        window = args[0] if args else kwargs.get("window")
        if window is not None:
            _schedule_bearer_capture(window)
        return result

    globals()["_inject_browser_script"] = _inject_browser_script
    _sleufbase_bearer_capture_installed = True
else:
    _sleufbase_bearer_capture_installed = False
