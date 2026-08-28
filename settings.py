from __future__ import annotations

import json
from pathlib import Path

from .legacy_bytecode import load_legacy_module


load_legacy_module("settings", globals(), __file__)


# Deze instellingen zijn toegevoegd boven op de oudere, gebundelde settings-module.
# Class-attribuutfallbacks houden ook rechtstreeks gemaakte AppSettings-objecten
# achterwaarts compatibel met de nieuwere UI- en exportopties.
DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN = True
TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY = "template_auto_fill_bgt_fysiek_voorkomen"
KICKTHEMAP_MATERIAL_CHOICES_KEY = "kickthemap_material_choices"
KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY = "kickthemap_profile_extra_choices"
setattr(
    AppSettings,
    TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
    DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN,
)
setattr(AppSettings, KICKTHEMAP_MATERIAL_CHOICES_KEY, [])
setattr(AppSettings, KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY, [])

_load_settings_without_bgt_surface_option = load_settings
_save_settings_without_bgt_surface_option = save_settings


def _bgt_surface_option_value(value: object) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return bool(value)
    if isinstance(value, str):
        normalized = value.strip().casefold()
        if normalized in {"0", "false", "nee", "no", "off", "uit"}:
            return False
        if normalized in {"1", "true", "ja", "yes", "on", "aan"}:
            return True
    return DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN


def _normalize_choice_list(value: object) -> list[str]:
    if not isinstance(value, (list, tuple)):
        return []
    choices: list[str] = []
    seen: set[str] = set()
    for item in value:
        choice = str(item or "").strip()
        normalized = choice.casefold()
        if not choice or normalized in seen:
            continue
        seen.add(normalized)
        choices.append(choice)
    return choices


def normalize_kickthemap_material_choices(value: object) -> list[str]:
    return _normalize_choice_list(value)


def normalize_kickthemap_profile_extra_choices(value: object) -> list[str]:
    """Normalize user-managed extra values for the Kabel/Leiding dropdown."""

    return _normalize_choice_list(value)


def load_settings() -> AppSettings:
    settings = _load_settings_without_bgt_surface_option()
    option_value: object = DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN
    material_choices: object = []
    profile_extra_choices: object = []
    try:
        payload = json.loads(_settings_path().read_text(encoding="utf-8"))
        if isinstance(payload, dict):
            option_value = payload.get(
                TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
                DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN,
            )
            material_choices = payload.get(KICKTHEMAP_MATERIAL_CHOICES_KEY, [])
            profile_extra_choices = payload.get(KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY, [])
    except (FileNotFoundError, OSError, json.JSONDecodeError, TypeError, ValueError):
        pass
    setattr(
        settings,
        TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
        _bgt_surface_option_value(option_value),
    )
    setattr(
        settings,
        KICKTHEMAP_MATERIAL_CHOICES_KEY,
        normalize_kickthemap_material_choices(material_choices),
    )
    setattr(
        settings,
        KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY,
        normalize_kickthemap_profile_extra_choices(profile_extra_choices),
    )
    return settings


def save_settings(settings: AppSettings) -> Path:
    settings_path = _save_settings_without_bgt_surface_option(settings)
    try:
        payload = json.loads(settings_path.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, json.JSONDecodeError, TypeError, ValueError):
        payload = {}
    if not isinstance(payload, dict):
        payload = {}
    payload[TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY] = _bgt_surface_option_value(
        getattr(
            settings,
            TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
            DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN,
        )
    )
    payload[KICKTHEMAP_MATERIAL_CHOICES_KEY] = normalize_kickthemap_material_choices(
        getattr(settings, KICKTHEMAP_MATERIAL_CHOICES_KEY, [])
    )
    payload[KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY] = normalize_kickthemap_profile_extra_choices(
        getattr(settings, KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY, [])
    )
    settings_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return settings_path
