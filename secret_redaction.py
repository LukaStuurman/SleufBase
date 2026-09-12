from __future__ import annotations

import re


REDACTED_SECRET = "[REDACTED]"

_SENSITIVE_KEYS = (
    "password",
    "passwd",
    "pwd",
    "token",
    "access_token",
    "refresh_token",
    "api_key",
    "apikey",
    "client_secret",
    "secret",
)
_KEY_PATTERN = "|".join(re.escape(key) for key in _SENSITIVE_KEYS)

_AUTHORIZATION_RE = re.compile(
    r"(?i)(\bauthorization\b\s*[:=]\s*)(?:bearer\s+)?([^\s,;]+)"
)
_BEARER_RE = re.compile(r"(?i)\bbearer\s+([A-Za-z0-9._~+/=-]{8,})")
_QUERY_RE = re.compile(
    rf"(?i)([?&](?:{_KEY_PATTERN})=)([^&#\s]+)"
)
_QUOTED_VALUE_RE = re.compile(
    rf"(?i)([\"']?(?:{_KEY_PATTERN})[\"']?\s*[:=]\s*)([\"'])(.*?)(\2)"
)
_UNQUOTED_VALUE_RE = re.compile(
    rf"(?i)(\b(?:{_KEY_PATTERN})\b\s*[:=]\s*)([^\s,;}}\]]+)"
)


def redact_sensitive_text(value: object) -> str:
    """Return diagnostic text with common credentials and bearer tokens removed."""

    text = str(value)
    text = _AUTHORIZATION_RE.sub(lambda match: match.group(1) + REDACTED_SECRET, text)
    text = _BEARER_RE.sub("Bearer " + REDACTED_SECRET, text)
    text = _QUERY_RE.sub(lambda match: match.group(1) + REDACTED_SECRET, text)
    text = _QUOTED_VALUE_RE.sub(
        lambda match: match.group(1) + match.group(2) + REDACTED_SECRET + match.group(4),
        text,
    )
    text = _UNQUOTED_VALUE_RE.sub(lambda match: match.group(1) + REDACTED_SECRET, text)
    return text
