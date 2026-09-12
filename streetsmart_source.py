from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import time
from typing import Any
from urllib.parse import quote

from .atomic_io import atomic_write_text


STREETSMART_WEB_URL = "https://streetsmart.cyclomedia.com/streetsmart"
STREETSMART_RD_SRS = "EPSG:28992"

_LOGIN_SCRIPT_PREFIX = '\n    (() => {\n      const USERNAME = '
_LOGIN_SCRIPT_MIDDLE = ';\n      const PASSWORD = '
_LOGIN_SCRIPT_SUFFIX = ';\n      const AUTO_LOGIN_COUNTER_KEY = \'klic-streetsmart-auto-login-count\';\n      const NEXT_LABELS = [\'volgende\', \'next\', \'continue\', \'doorgaan\'];\n      const SUBMIT_LABELS = [\'inloggen\', \'log in\', \'login\', \'sign in\', \'aanmelden\'];\n      const DASHBOARD_HEIGHT_THRESHOLD = 120;\n      const ACTION_DEBOUNCE_MS = 1400;\n      const FIELD_SETTLE_MS = 260;\n      const MAX_AUTO_LOGIN_ACTIONS = 6;\n\n      const normalizeText = (value) => String(value || \'\').replace(/\\s+/g, \' \').trim().toLowerCase();\n\n      const isVisible = (element) => {\n        if (!(element instanceof HTMLElement)) {\n          return false;\n        }\n        if (element.hidden || element.getAttribute(\'aria-hidden\') === \'true\' || element.disabled) {\n          return false;\n        }\n        const style = window.getComputedStyle(element);\n        const rect = element.getBoundingClientRect();\n        return style.display !== \'none\' && style.visibility !== \'hidden\' && rect.width > 0 && rect.height > 0;\n      };\n\n      const firstVisible = (selectors) => {\n        for (const selector of selectors) {\n          const matches = Array.from(document.querySelectorAll(selector));\n          for (const element of matches) {\n            if (isVisible(element)) {\n              return element;\n            }\n          }\n        }\n        return null;\n      };\n\n      const setFieldValue = (element, value) => {\n        if (!element) {\n          return;\n        }\n        const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;\n        const descriptor = Object.getOwnPropertyDescriptor(prototype, \'value\');\n        if (descriptor && typeof descriptor.set === \'function\') {\n          descriptor.set.call(element, value);\n        } else {\n          element.value = value;\n        }\n        element.dispatchEvent(new Event(\'input\', { bubbles: true }));\n        element.dispatchEvent(new Event(\'change\', { bubbles: true }));\n      };\n\n      const findActionButton = (labels) => {\n        const candidates = Array.from(\n          document.querySelectorAll(\'button, input[type="submit"], input[type="button"]\')\n        );\n        for (const candidate of candidates) {\n          if (!(candidate instanceof HTMLElement) || !isVisible(candidate)) {\n            continue;\n          }\n          const label = normalizeText(\n            candidate.textContent || candidate.value || candidate.getAttribute(\'aria-label\') || \'\'\n          );\n          if (labels.some((expected) => label.includes(expected))) {\n            return candidate;\n          }\n        }\n        return null;\n      };\n\n      const clickElement = (element) => {\n        if (!(element instanceof HTMLElement) || !isVisible(element)) {\n          return false;\n        }\n        element.dispatchEvent(new MouseEvent(\'mousedown\', { bubbles: true, cancelable: true, view: window }));\n        element.dispatchEvent(new MouseEvent(\'mouseup\', { bubbles: true, cancelable: true, view: window }));\n        element.click();\n        return true;\n      };\n\n      const submitForm = (form, submitter) => {\n        if (!(form instanceof HTMLFormElement)) {\n          return false;\n        }\n        if (typeof form.requestSubmit === \'function\') {\n          if (submitter instanceof HTMLElement) {\n            form.requestSubmit(submitter);\n          } else {\n            form.requestSubmit();\n          }\n        } else {\n          form.submit();\n        }\n        return true;\n      };\n\n      const navigateSameWindow = (rawUrl) => {\n        if (!rawUrl) {\n          return window;\n        }\n        const urlText = String(rawUrl).trim();\n        if (!urlText || urlText.startsWith(\'javascript:\') || urlText.startsWith(\'mailto:\') || urlText.startsWith(\'tel:\')) {\n          return window;\n        }\n        try {\n          window.location.href = new URL(urlText, window.location.href).href;\n        } catch (_error) {\n          window.location.href = urlText;\n        }\n        return window;\n      };\n\n      if (!window.__klicStreetSmartBrowserPatched) {\n        window.__klicStreetSmartBrowserPatched = true;\n\n        window.open = function(url) {\n          return navigateSameWindow(url);\n        };\n\n        document.addEventListener(\'click\', (event) => {\n          if (event.defaultPrevented || event.button !== 0) {\n            return;\n          }\n          if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {\n            return;\n          }\n          const anchor = event.target instanceof Element ? event.target.closest(\'a[href]\') : null;\n          if (!anchor || anchor.hasAttribute(\'download\')) {\n            return;\n          }\n          const target = String(anchor.getAttribute(\'target\') || \'\').trim().toLowerCase();\n          if (target !== \'_blank\' && target !== \'_new\') {\n            return;\n          }\n          event.preventDefault();\n          navigateSameWindow(anchor.href || anchor.getAttribute(\'href\'));\n        }, true);\n      }\n\n      const collapseDashboard = () => {\n        const tabsContainer = document.querySelector(\'.tabs-container\');\n        if (!(tabsContainer instanceof HTMLElement) || !isVisible(tabsContainer)) {\n          return false;\n        }\n        const rect = tabsContainer.getBoundingClientRect();\n        if (rect.height <= DASHBOARD_HEIGHT_THRESHOLD) {\n          return false;\n        }\n        const now = Date.now();\n        const lastClickAt = Number(window.__klicStreetSmartDashboardClickAt || \'0\');\n        if ((now - lastClickAt) < ACTION_DEBOUNCE_MS) {\n          return false;\n        }\n        const collapseButton = firstVisible([\n          \'button[data-testid="dashboard-collapse-button"]\',\n          \'button.dashboard-expand\',\n        ]);\n        if (collapseButton) {\n          window.__klicStreetSmartDashboardClickAt = now;\n          return clickElement(collapseButton);\n        }\n        tabsContainer.style.maxHeight = \'0px\';\n        tabsContainer.style.overflow = \'hidden\';\n        tabsContainer.style.border = \'0\';\n        tabsContainer.style.padding = \'0\';\n        return true;\n      };\n\n      const mosaicWindows = () => Array.from(\n        document.querySelectorAll(\'.mosaic-window.cm-mosaic-window, .mosaic-window\')\n      ).filter((element) => {\n        if (!(element instanceof HTMLElement) || !isVisible(element)) {\n          return false;\n        }\n        const rect = element.getBoundingClientRect();\n        return rect.width >= 180 && rect.height >= 140;\n      });\n\n      const windowSignature = (windows) => windows\n        .map((element) => {\n          const rect = element.getBoundingClientRect();\n          return `${Math.round(rect.left)}:${Math.round(rect.top)}:${Math.round(rect.width)}:${Math.round(rect.height)}`;\n        })\n        .join(\'|\');\n\n      const preferredWindow = (windows) => {\n        const viewportCenter = window.innerWidth / 2;\n        const items = windows.map((element) => {\n          const rect = element.getBoundingClientRect();\n          const haystack = normalizeText([\n            element.className || \'\',\n            element.getAttribute(\'data-testid\') || \'\',\n            element.textContent || \'\',\n            Array.from(element.querySelectorAll(\'[class],[data-testid]\'))\n              .slice(0, 28)\n              .map((node) => [node.className || \'\', node.getAttribute(\'data-testid\') || \'\'].join(\' \'))\n              .join(\' \'),\n          ].join(\' \'));\n          let score = 0;\n          if (haystack.includes(\'panorama\')) {\n            score += 40;\n          }\n          if (haystack.includes(\'cyclorama\') || haystack.includes(\'recording\') || haystack.includes(\'street\')) {\n            score += 30;\n          }\n          if (haystack.includes(\'oblique\')) {\n            score += 12;\n          }\n          if (haystack.includes(\'point cloud\')) {\n            score += 8;\n          }\n          if (haystack.includes(\'viewer-container__map\') || haystack.includes(\'location-map\') || haystack.includes(\'basemap\')) {\n            score -= 30;\n          }\n          const centerX = rect.left + (rect.width / 2);\n          score -= Math.abs(centerX - viewportCenter) / Math.max(window.innerWidth, 1);\n          return {\n            element,\n            rect,\n            centerX,\n            score,\n          };\n        });\n        if (!items.length) {\n          return null;\n        }\n        if (items.length >= 3) {\n          const middleItem = [...items].sort((left, right) => left.centerX - right.centerX)[Math.floor(items.length / 2)];\n          if (middleItem) {\n            return middleItem.element;\n          }\n        }\n        items.sort((left, right) => right.score - left.score);\n        return items[0].element;\n      };\n\n      const maximizePreferredWindow = () => {\n        const windows = mosaicWindows();\n        if (windows.length < 2) {\n          return false;\n        }\n        const signature = windowSignature(windows);\n        if (window.__klicStreetSmartMaximizedSignature === signature) {\n          return false;\n        }\n        const now = Date.now();\n        const lastClickAt = Number(window.__klicStreetSmartMaximizeClickAt || \'0\');\n        if ((now - lastClickAt) < ACTION_DEBOUNCE_MS) {\n          return false;\n        }\n        const targetWindow = preferredWindow(windows);\n        if (!(targetWindow instanceof HTMLElement)) {\n          return false;\n        }\n        const maximizeButton = Array.from(\n          targetWindow.querySelectorAll(\n            \'button.btn-maximize, button[data-testid^="maxmize-"], button[data-testid^="maximize-"]\'\n          )\n        ).find((element) => isVisible(element));\n        if (!(maximizeButton instanceof HTMLElement)) {\n          return false;\n        }\n        window.__klicStreetSmartMaximizeClickAt = now;\n        window.__klicStreetSmartMaximizedSignature = signature;\n        return clickElement(maximizeButton);\n      };\n\n      const applyViewerLayout = () => {\n        try {\n          collapseDashboard();\n        } catch (_error) {\n        }\n        try {\n          maximizePreferredWindow();\n        } catch (_error) {\n        }\n      };\n\n      if (!window.__klicStreetSmartLayoutTimer) {\n        window.__klicStreetSmartLayoutTimer = window.setInterval(() => {\n          applyViewerLayout();\n        }, 1200);\n        document.addEventListener(\'visibilitychange\', () => {\n          if (!document.hidden) {\n            applyViewerLayout();\n          }\n        }, true);\n      }\n      window.setTimeout(applyViewerLayout, 120);\n      window.setTimeout(applyViewerLayout, 850);\n      window.setTimeout(applyViewerLayout, 2400);\n\n      const authState = window.__klicStreetSmartAuthState || (window.__klicStreetSmartAuthState = {\n        lastActionAt: 0,\n        lastActionKey: \'\',\n        lastFieldFillAt: 0,\n        lastFieldStage: \'\',\n      });\n\n      const getAttempts = () => Number(sessionStorage.getItem(AUTO_LOGIN_COUNTER_KEY) || \'0\');\n      const clearAttempts = () => {\n        sessionStorage.removeItem(AUTO_LOGIN_COUNTER_KEY);\n        authState.lastActionKey = \'\';\n      };\n      const bumpAttempts = () => {\n        const nextAttempts = getAttempts() + 1;\n        sessionStorage.setItem(AUTO_LOGIN_COUNTER_KEY, String(nextAttempts));\n        return nextAttempts;\n      };\n      const canPerformAction = (actionKey) => {\n        const now = Date.now();\n        if (authState.lastActionKey === actionKey && (now - authState.lastActionAt) < ACTION_DEBOUNCE_MS) {\n          return false;\n        }\n        authState.lastActionAt = now;\n        authState.lastActionKey = actionKey;\n        return true;\n      };\n      const noteFieldFill = (stage) => {\n        authState.lastFieldStage = stage;\n        authState.lastFieldFillAt = Date.now();\n      };\n      const fieldRecentlyFilled = (stage) => {\n        return authState.lastFieldStage === stage && (Date.now() - authState.lastFieldFillAt) < FIELD_SETTLE_MS;\n      };\n      const syncFieldValue = (element, value, stage) => {\n        if (!(element instanceof HTMLInputElement) && !(element instanceof HTMLTextAreaElement)) {\n          return false;\n        }\n        if (String(element.value || \'\') === String(value || \'\')) {\n          return false;\n        }\n        element.focus();\n        setFieldValue(element, value);\n        noteFieldFill(stage);\n        return true;\n      };\n\n      const loginFields = () => {\n        return {\n          usernameInput: firstVisible([\n            \'input[name="username"]\',\n            \'input[name="email"]\',\n            \'input[name="identifier"]\',\n            \'input[name="loginfmt"]\',\n            \'input[type="email"]\',\n            \'input[autocomplete="username"]\',\n            \'input[id*="user"]\',\n            \'input[id*="email"]\',\n            \'input[id*="login"]\',\n          ]),\n          passwordInput: firstVisible([\n            \'input[name="password"]\',\n            \'input[name="passwd"]\',\n            \'input[type="password"]\',\n            \'input[autocomplete="current-password"]\',\n            \'input[id*="pass"]\',\n          ]),\n        };\n      };\n\n      const runAutoLoginStep = () => {\n        const fields = loginFields();\n        const usernameInput = fields.usernameInput;\n        const passwordInput = fields.passwordInput;\n        if (!usernameInput && !passwordInput) {\n          clearAttempts();\n          return \'no-login-form\';\n        }\n\n        if (!USERNAME || !PASSWORD) {\n          return \'missing-saved-credentials\';\n        }\n\n        if (getAttempts() >= MAX_AUTO_LOGIN_ACTIONS) {\n          return \'max-attempts-reached\';\n        }\n\n        let changed = false;\n        if (usernameInput) {\n          changed = syncFieldValue(usernameInput, USERNAME, passwordInput ? \'username-password\' : \'username-only\') || changed;\n        }\n        if (passwordInput) {\n          changed = syncFieldValue(passwordInput, PASSWORD, \'password-only\') || changed;\n        }\n        if (changed) {\n          return passwordInput ? \'filled-login-form\' : \'filled-username-form\';\n        }\n\n        if (\n          fieldRecentlyFilled(\'username-only\')\n          || fieldRecentlyFilled(\'username-password\')\n          || fieldRecentlyFilled(\'password-only\')\n        ) {\n          return \'awaiting-field-settle\';\n        }\n\n        const form =\n          (passwordInput && passwordInput.form)\n          || (usernameInput && usernameInput.form)\n          || document.querySelector(\'form\');\n\n        if (passwordInput) {\n          const submitButton = findActionButton(SUBMIT_LABELS) || findActionButton(NEXT_LABELS);\n          if (submitButton && canPerformAction(\'password-button\')) {\n            bumpAttempts();\n            clickElement(submitButton);\n            return \'clicked-login-button\';\n          }\n          if (form && canPerformAction(\'password-form\')) {\n            bumpAttempts();\n            submitForm(form, submitButton);\n            return \'submitted-login-form\';\n          }\n          return \'filled-login-form\';\n        }\n\n        const nextButton = findActionButton(NEXT_LABELS) || findActionButton(SUBMIT_LABELS);\n        if (nextButton && canPerformAction(\'username-button\')) {\n          bumpAttempts();\n          clickElement(nextButton);\n          return \'clicked-next-button\';\n        }\n        if (form && canPerformAction(\'username-form\')) {\n          bumpAttempts();\n          submitForm(form, nextButton);\n          return \'submitted-username-form\';\n        }\n\n        return \'filled-username-form\';\n      };\n\n      const scheduleAutoLogin = (delay = 80) => {\n        if (!USERNAME || !PASSWORD) {\n          return;\n        }\n        window.clearTimeout(window.__klicStreetSmartAutoLoginHandle || 0);\n        window.__klicStreetSmartAutoLoginHandle = window.setTimeout(() => {\n          try {\n            window.__klicStreetSmartLastLoginResult = runAutoLoginStep();\n          } catch (_error) {\n          }\n        }, delay);\n      };\n\n      if (!window.__klicStreetSmartAutoLoginObserverAdded) {\n        window.__klicStreetSmartAutoLoginObserverAdded = true;\n        const observerTarget = document.documentElement || document.body;\n        if (observerTarget) {\n          const observer = new MutationObserver(() => {\n            scheduleAutoLogin(90);\n          });\n          observer.observe(observerTarget, {\n            childList: true,\n            subtree: true,\n            attributes: true,\n            attributeFilter: [\'class\', \'style\', \'hidden\', \'aria-hidden\', \'type\', \'value\'],\n          });\n        }\n        document.addEventListener(\'input\', () => scheduleAutoLogin(160), true);\n        document.addEventListener(\'change\', () => scheduleAutoLogin(160), true);\n        window.addEventListener(\'focus\', () => scheduleAutoLogin(60), true);\n        window.addEventListener(\'pageshow\', () => scheduleAutoLogin(60), true);\n        window.__klicStreetSmartAutoLoginTimer = window.setInterval(() => {\n          scheduleAutoLogin(0);\n        }, 1200);\n      }\n\n      const initialResult = runAutoLoginStep();\n      scheduleAutoLogin(320);\n      return initialResult;\n    })()\n    '


def streetsmart_data_dir() -> Path:
    local_appdata = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
    return local_appdata / "KlicTiffKaarten" / "streetsmart"


def _safe_streetsmart_slug(value: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9._-]+", "_", str(value or "").strip().lower())
    return sanitized.strip("._-") or "default"


def streetsmart_browser_sessions_dir() -> Path:
    return streetsmart_data_dir() / "browser_sessions"


def streetsmart_browser_storage_dir(username: str) -> Path:
    return streetsmart_browser_sessions_dir() / _safe_streetsmart_slug(username)


def streetsmart_embedded_storage_dir() -> Path:
    return streetsmart_data_dir() / "embedded_webview2"


def clear_streetsmart_storage(username: str = "") -> list[Path]:
    normalized_username = str(username or "").strip()
    targets = [
        streetsmart_embedded_storage_dir(),
        streetsmart_browser_storage_dir(normalized_username)
        if normalized_username
        else streetsmart_browser_sessions_dir(),
    ]
    removed: list[Path] = []
    seen: set[str] = set()
    for target in targets:
        key = str(target).lower()
        if key in seen or not target.exists():
            continue
        seen.add(key)
        last_error: OSError | None = None
        for _attempt in range(20):
            try:
                shutil.rmtree(target)
                last_error = None
                break
            except FileNotFoundError:
                last_error = None
                break
            except OSError as exc:
                last_error = exc
                time.sleep(0.2)
        if last_error is not None:
            raise last_error
        removed.append(target)
    return removed


def streetsmart_state_path() -> Path:
    return streetsmart_data_dir() / "state.json"


def default_streetsmart_state() -> dict[str, Any]:
    return {"version": 0, "selection": None}


def load_streetsmart_state() -> dict[str, Any]:
    state_path = streetsmart_state_path()
    try:
        payload = json.loads(state_path.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, json.JSONDecodeError, TypeError, ValueError):
        return default_streetsmart_state()
    if not isinstance(payload, dict):
        return default_streetsmart_state()
    version = payload.get("version", 0)
    try:
        normalized_version = int(version)
    except (TypeError, ValueError):
        normalized_version = 0
    selection = payload.get("selection")
    if selection is not None and not isinstance(selection, dict):
        selection = None
    return {"version": normalized_version, "selection": selection}


def save_streetsmart_state(selection: dict[str, Any] | None) -> Path:
    state_path = streetsmart_state_path()
    state_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {"version": time.time_ns(), "selection": selection}
    atomic_write_text(
        state_path,
        json.dumps(payload, ensure_ascii=True, indent=2),
        encoding="utf-8",
    )
    return state_path


def streetsmart_selection_center(selection: dict[str, Any] | None) -> tuple[float, float] | None:
    if not isinstance(selection, dict):
        return None
    center = selection.get("center")
    if not isinstance(center, (list, tuple)) or len(center) < 2:
        return None
    try:
        x = float(center[0])
        y = float(center[1])
    except (TypeError, ValueError):
        return None
    if x != x or y != y:
        return None
    return x, y


def streetsmart_selection_url(selection: dict[str, Any] | None) -> str:
    center = streetsmart_selection_center(selection)
    if center is None:
        return STREETSMART_WEB_URL
    query = f"{center[0]:.2f};{center[1]:.2f};{STREETSMART_RD_SRS}"
    return f"{STREETSMART_WEB_URL}?q={quote(query, safe=';:.-')}"


def streetsmart_login_script(username: str, password: str) -> str:
    return (
        _LOGIN_SCRIPT_PREFIX
        + json.dumps(username)
        + _LOGIN_SCRIPT_MIDDLE
        + json.dumps(password)
        + _LOGIN_SCRIPT_SUFFIX
    )
