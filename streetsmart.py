from __future__ import annotations

from typing import Any, MutableMapping

from .source_migration import load_migrating_module


_STREETSMART_SOURCE_EXPORTS = (
    "STREETSMART_WEB_URL",
    "STREETSMART_RD_SRS",
    "streetsmart_data_dir",
    "_safe_streetsmart_slug",
    "streetsmart_browser_sessions_dir",
    "streetsmart_browser_storage_dir",
    "streetsmart_embedded_storage_dir",
    "clear_streetsmart_storage",
    "streetsmart_state_path",
    "default_streetsmart_state",
    "load_streetsmart_state",
    "save_streetsmart_state",
    "streetsmart_selection_center",
    "streetsmart_selection_url",
    "streetsmart_login_script",
)


def _load_streetsmart_source(namespace: MutableMapping[str, Any]) -> None:
    from . import streetsmart_source

    for name in _STREETSMART_SOURCE_EXPORTS:
        namespace[name] = getattr(streetsmart_source, name)


load_migrating_module(
    "streetsmart",
    globals(),
    __file__,
    source_loader=_load_streetsmart_source,
)
