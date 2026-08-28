from __future__ import annotations

from types import ModuleType

from .settings import (
    KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY,
    load_settings,
    normalize_kickthemap_profile_extra_choices,
)


PATCH_VERSION = 1


def _option_identity(option: object) -> set[str]:
    if not isinstance(option, dict):
        return set()
    identities: set[str] = set()
    for key in ("code", "label"):
        value = str(option.get(key, "") or "").strip().casefold()
        if value:
            identities.add(value)
    return identities


def merge_profile_dropdown_options(
    base_options: list[dict[str, str]],
    extra_choices: object,
) -> list[dict[str, str]]:
    """Append user-managed dropdown words without altering DXF mapping rules."""

    merged = [dict(option) for option in base_options]
    seen: set[str] = set()
    for option in merged:
        seen.update(_option_identity(option))

    for choice in normalize_kickthemap_profile_extra_choices(extra_choices):
        normalized = choice.casefold()
        if normalized in seen:
            continue
        merged.append(
            {
                "label": choice,
                "code": choice,
                "keywords": choice,
            }
        )
        seen.add(normalized)
    return merged


def install_kickthemap_profile_choices_patch(browser_module: ModuleType) -> None:
    """Extend the browser's Kabel/Leiding dropdown with settings-managed words."""

    if int(getattr(browser_module, "_sleufbase_profile_choices_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_profile_options = getattr(browser_module, "_profile_options", None)
    if not callable(original_profile_options):
        raise RuntimeError("KickTheMap-browser mist _profile_options().")

    def _profile_options() -> list[dict[str, str]]:
        base_options = list(original_profile_options() or [])
        settings = load_settings()
        extras = getattr(settings, KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY, [])
        return merge_profile_dropdown_options(base_options, extras)

    browser_module._profile_options = _profile_options
    browser_module._sleufbase_profile_choices_patch_version = PATCH_VERSION
