from __future__ import annotations

import json

import webview
from webview.menu import Menu, MenuAction, MenuSeparator

from .kickthemap import KickTheMapClient, kickthemap_browser_session_dir
from .settings import (
    KICKTHEMAP_MATERIAL_CHOICES_KEY,
    KickTheMapSavedAccount,
    load_settings,
    normalize_kickthemap_material_choices,
)


HOME_URL = KickTheMapClient.BASE_URL + "/"
SIGNIN_PATH = "/signin"
WINDOW_TITLE = "SleufBase Browser"
WINDOW_WIDTH = 1440
WINDOW_HEIGHT = 960
WINDOW_MIN_SIZE = (980, 640)

def _storage_path_for_account(account: KickTheMapSavedAccount | None) -> str:
    target_dir = kickthemap_browser_session_dir(account.email if account is not None else "")
    target_dir.mkdir(parents=True, exist_ok=True)
    return str(target_dir)


def _selected_account() -> KickTheMapSavedAccount | None:
    settings = load_settings()
    selected_email = str(settings.kickthemap_last_email or "").strip().lower()
    if not selected_email:
        return None
    for account in settings.kickthemap_saved_accounts or []:
        if str(account.email or "").strip().lower() == selected_email:
            return account
    return None


def _profile_options() -> list[dict[str, str]]:
    settings = load_settings()
    options: list[dict[str, str]] = []
    seen_codes: set[str] = set()
    for rule in settings.kickthemap_object_layer_rules or []:
        raw_keywords = str(getattr(rule, "keywords", "") or "")
        code = next((part.strip() for part in raw_keywords.split(",") if part.strip()), "")
        if not code:
            continue
        profile_label = str(getattr(rule, "profile_label", "") or "").strip()
        label = profile_label if profile_label else code
        normalized_code = code.lower()
        if normalized_code in seen_codes:
            continue
        seen_codes.add(normalized_code)
        options.append({"label": label, "code": code, "keywords": raw_keywords})
    return options


def _material_options() -> list[str]:
    settings = load_settings()
    return normalize_kickthemap_material_choices(
        getattr(settings, KICKTHEMAP_MATERIAL_CHOICES_KEY, [])
    )


def _injected_script(account: KickTheMapSavedAccount | None, start_url: str | None = None) -> str:
    email = account.email if account is not None else ""
    password = account.password if account is not None else ""
    profile_options = _profile_options()
    material_options = _material_options()
    return f"""
    (() => {{
      const HOME_URL = {json.dumps(HOME_URL)};
      const START_URL = {json.dumps(start_url or "")};
      const EMAIL = {json.dumps(email)};
      const PASSWORD = {json.dumps(password)};
      const SIGNIN_PATH = {json.dumps(SIGNIN_PATH)};
      const PROFILE_OPTIONS = {json.dumps(profile_options)};
      const MATERIAL_OPTIONS = {json.dumps(material_options)};
      const desiredUrlKey = 'sleufbase-kickthemap-desired-url';
      const labelMap = new Map([
        ['Name', 'Kabel/Leiding afkorting'],
        ['Attribute 1', 'Materiaal'],
        ['Attribute 2', 'Diameter'],
        ['Attribute 3', 'Bundel (aantal)'],
      ]);

      const normalizeText = (value) => String(value || '').replace(/\\s+/g, ' ').trim();

      const navigateSameWindow = (rawUrl) => {{
        if (!rawUrl) {{
          return window;
        }}
        const urlText = String(rawUrl).trim();
        if (!urlText || urlText.startsWith('javascript:') || urlText.startsWith('mailto:') || urlText.startsWith('tel:')) {{
          return window;
        }}
        try {{
          window.location.href = new URL(urlText, window.location.href).href;
        }} catch (_error) {{
          window.location.href = urlText;
        }}
        return window;
      }};

      const normalizeAnchors = () => {{
        document.querySelectorAll('a[href][target]').forEach((anchor) => {{
          const target = String(anchor.getAttribute('target') || '').trim().toLowerCase();
          if (target === '_blank' || target === '_new') {{
            anchor.setAttribute('target', '_self');
          }}
        }});
      }};

      const relabelKickTheMapFields = () => {{
        const root = document.body || document.documentElement;
        if (!root) {{
          return;
        }}
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
        const textNodes = [];
        while (walker.nextNode()) {{
          textNodes.push(walker.currentNode);
        }}
        for (const node of textNodes) {{
          const parent = node.parentElement;
          if (!parent) {{
            continue;
          }}
          const normalized = normalizeText(node.nodeValue || '');
          const replacement = labelMap.get(normalized);
          if (!replacement) {{
            continue;
          }}
          const rawValue = String(node.nodeValue || '');
          const leading = rawValue.match(/^\\s*/)?.[0] || '';
          const trailing = rawValue.match(/\\s*$/)?.[0] || '';
          node.nodeValue = `${{leading}}${{replacement}}${{trailing}}`;
        }}
      }};

      const firstVisibleInputAfter = (labelElement) => {{
        const visited = new Set();
        const candidates = [];
        const pushInputs = (root) => {{
          if (!root || visited.has(root)) {{
            return;
          }}
          visited.add(root);
          root.querySelectorAll?.('input:not([type="hidden"]), textarea').forEach((input) => {{
            if (!(input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement)) {{
              return;
            }}
            if (input.disabled || input.readOnly || input.offsetParent === null) {{
              return;
            }}
            candidates.push(input);
          }});
        }};

        let cursor = labelElement;
        for (let depth = 0; cursor && depth < 5; depth += 1) {{
          pushInputs(cursor);
          let sibling = cursor.nextElementSibling;
          for (let count = 0; sibling && count < 5; count += 1) {{
            pushInputs(sibling);
            sibling = sibling.nextElementSibling;
          }}
          cursor = cursor.parentElement;
        }}
        return candidates[0] || null;
      }};

      const compactField = (element) => {{
        if (!(element instanceof HTMLElement)) {{
          return;
        }}
        element.style.setProperty('box-sizing', 'border-box', 'important');
        element.style.setProperty('width', '104px', 'important');
        element.style.setProperty('min-width', '104px', 'important');
        element.style.setProperty('max-width', '104px', 'important');
        element.style.setProperty('position', 'relative', 'important');
        element.style.setProperty('left', '-14px', 'important');
        element.style.setProperty('margin-left', '0', 'important');
      }};

      const visiblePanelText = (root) => {{
        const panel = root?.closest?.('#sidebar_properties, [id*="sidebar"], [class*="sidebar"], [class*="properties"], [class*="panel"]')
          || document.querySelector('#sidebar_properties')
          || document.body;
        const parts = [];
        const walker = document.createTreeWalker(panel, NodeFilter.SHOW_TEXT);
        while (walker.nextNode()) {{
          const node = walker.currentNode;
          const parent = node.parentElement;
          if (!parent || parent.offsetParent === null) {{
            continue;
          }}
          const text = normalizeText(node.nodeValue || '');
          if (text) {{
            parts.push(text);
          }}
        }}
        return parts.join(' ').toLowerCase();
      }};

      const isPolylineContext = (labelElement) => {{
        const text = visiblePanelText(labelElement);
        if (/\\b(point|punt)\\b/.test(text)) {{
          return false;
        }}
        if (/\\b(polyline|line string|linestring|lijn|polylijn)\\b/.test(text)) {{
          return true;
        }}
        const path = labelElement?.closest?.('[class*="polyline"], [data-type*="polyline"], [data-geometry*="polyline"], [data-type*="LineString"], [data-geometry*="LineString"]');
        return Boolean(path);
      }};

      const valueMatchesOption = (value, option) => {{
        const normalized = normalizeText(value).toLowerCase();
        if (!normalized) {{
          return false;
        }}
        if (normalized === String(option.code || '').toLowerCase()) {{
          return true;
        }}
        if (normalized === String(option.label || '').toLowerCase()) {{
          return true;
        }}
        return String(option.keywords || '')
          .split(',')
          .map((part) => normalizeText(part).toLowerCase())
          .filter(Boolean)
          .includes(normalized);
      }};

      const setNativeValue = (input, value) => {{
        const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        const descriptor = Object.getOwnPropertyDescriptor(prototype, 'value');
        if (descriptor && descriptor.set) {{
          descriptor.set.call(input, value);
        }} else {{
          input.value = value;
        }}
        input.dispatchEvent(new Event('input', {{ bubbles: true }}));
        input.dispatchEvent(new Event('change', {{ bubbles: true }}));
        input.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: value }}));
      }};

      const applyProfileNameDropdown = () => {{
        if (!PROFILE_OPTIONS.length) {{
          return;
        }}
        const labelElements = [];
        document.querySelectorAll('body *').forEach((element) => {{
          if (!(element instanceof HTMLElement) || element.children.length !== 0) {{
            return;
          }}
          if (normalizeText(element.textContent || '') === 'Kabel/Leiding afkorting') {{
            labelElements.push(element);
          }}
        }});

        labelElements.forEach((labelElement) => {{
          const input = firstVisibleInputAfter(labelElement);
          if (!input || input.dataset.klicProfileDropdownAttached === '1') {{
            return;
          }}

          const select = document.createElement('select');
          select.dataset.klicProfileDropdown = '1';
          select.style.boxSizing = 'border-box';
          select.style.setProperty('width', '104px', 'important');
          select.style.setProperty('min-width', '104px', 'important');
          select.style.setProperty('max-width', '104px', 'important');
          select.style.setProperty('position', 'relative', 'important');
          select.style.setProperty('left', '-14px', 'important');
          select.style.setProperty('margin-left', '0', 'important');
          select.style.setProperty('padding-left', '2px', 'important');
          select.style.setProperty('text-indent', '0', 'important');
          select.style.height = `${{Math.max(30, input.getBoundingClientRect().height || 30)}}px`;
          select.style.font = window.getComputedStyle(input).font;
          select.style.color = window.getComputedStyle(input).color;
          select.style.background = window.getComputedStyle(input).backgroundColor || '#fff';
          select.style.border = window.getComputedStyle(input).border || '1px solid #bbb';
          select.style.borderRadius = window.getComputedStyle(input).borderRadius || '3px';

          const emptyOption = document.createElement('option');
          emptyOption.value = '';
          emptyOption.textContent = '';
          select.appendChild(emptyOption);

          PROFILE_OPTIONS.forEach((option) => {{
            const item = document.createElement('option');
            item.value = option.code;
            item.textContent = option.label;
            item.title = option.code;
            select.appendChild(item);
          }});

          const syncSelectFromInput = () => {{
            const match = PROFILE_OPTIONS.find((option) => valueMatchesOption(input.value, option));
            select.value = match ? match.code : '';
          }};

          syncSelectFromInput();
          select.addEventListener('change', () => {{
            setNativeValue(input, select.value);
            input.dataset.klicProfileVisualLabel = select.selectedOptions[0]?.textContent || '';
          }});
          input.addEventListener('input', syncSelectFromInput);
          input.addEventListener('change', syncSelectFromInput);

          input.dataset.klicProfileDropdownAttached = '1';
          input.style.position = 'absolute';
          input.style.left = '-10000px';
          input.style.width = '1px';
          input.style.height = '1px';
          input.style.opacity = '0';
          input.tabIndex = -1;
          input.setAttribute('aria-hidden', 'true');
          input.insertAdjacentElement('afterend', select);
        }});
      }};

      const applyMaterialDropdown = () => {{
        if (!MATERIAL_OPTIONS.length) {{
          return;
        }}
        const labelElements = [];
        document.querySelectorAll('body *').forEach((element) => {{
          if (!(element instanceof HTMLElement) || element.children.length !== 0) {{
            return;
          }}
          if (normalizeText(element.textContent || '') === 'Materiaal') {{
            labelElements.push(element);
          }}
        }});

        labelElements.forEach((labelElement) => {{
          const input = firstVisibleInputAfter(labelElement);
          if (!input || input.dataset.klicMaterialDropdownAttached === '1') {{
            return;
          }}

          const select = document.createElement('select');
          select.dataset.klicMaterialDropdown = '1';
          select.style.boxSizing = 'border-box';
          select.style.setProperty('width', '104px', 'important');
          select.style.setProperty('min-width', '104px', 'important');
          select.style.setProperty('max-width', '104px', 'important');
          select.style.setProperty('position', 'relative', 'important');
          select.style.setProperty('left', '-14px', 'important');
          select.style.setProperty('margin-left', '0', 'important');
          select.style.setProperty('padding-left', '2px', 'important');
          select.style.height = `${{Math.max(30, input.getBoundingClientRect().height || 30)}}px`;
          select.style.font = window.getComputedStyle(input).font;
          select.style.color = window.getComputedStyle(input).color;
          select.style.background = window.getComputedStyle(input).backgroundColor || '#fff';
          select.style.border = window.getComputedStyle(input).border || '1px solid #bbb';
          select.style.borderRadius = window.getComputedStyle(input).borderRadius || '3px';

          const emptyOption = document.createElement('option');
          emptyOption.value = '';
          emptyOption.textContent = '';
          select.appendChild(emptyOption);
          MATERIAL_OPTIONS.forEach((material) => {{
            const item = document.createElement('option');
            item.value = material;
            item.textContent = material;
            select.appendChild(item);
          }});

          const syncSelectFromInput = () => {{
            const normalizedValue = normalizeText(input.value).toLowerCase();
            const match = MATERIAL_OPTIONS.find(
              (material) => normalizeText(material).toLowerCase() === normalizedValue
            );
            select.value = match || '';
          }};
          syncSelectFromInput();
          select.addEventListener('change', () => setNativeValue(input, select.value));
          input.addEventListener('input', syncSelectFromInput);
          input.addEventListener('change', syncSelectFromInput);

          input.dataset.klicMaterialDropdownAttached = '1';
          input.style.position = 'absolute';
          input.style.left = '-10000px';
          input.style.width = '1px';
          input.style.height = '1px';
          input.style.opacity = '0';
          input.tabIndex = -1;
          input.setAttribute('aria-hidden', 'true');
          input.insertAdjacentElement('afterend', select);
        }});
      }};

      const applyDiverseSplit = () => {{
        const cleanupDuplicateRows = () => {{
          const seenInputs = new Set();
          document.querySelectorAll('[data-klic-diverse-split="1"]').forEach((row) => {{
            const input = row.previousElementSibling;
            if (input?.dataset?.klicDiverseSplitAttached === '1') {{
              if (seenInputs.has(input)) {{
                row.remove();
                return;
              }}
              seenInputs.add(input);
            }}
          }});
        }};
        cleanupDuplicateRows();

        const labelElements = [];
        document.querySelectorAll('body *').forEach((element) => {{
          if (!(element instanceof HTMLElement) || element.children.length !== 0) {{
            return;
          }}
          if (element.closest('[data-klic-diverse-split="1"], [data-klic-diverse-labels="1"]')) {{
            return;
          }}
          const text = normalizeText(element.textContent || '');
          if (text === 'Bundel (aantal)' || text === 'Dekband') {{
            labelElements.push(element);
          }}
        }});

        labelElements.forEach((labelElement) => {{
          const attachedInput = labelElement.parentElement?.querySelector?.('input[data-klic-diverse-split-attached="1"], textarea[data-klic-diverse-split-attached="1"]');
          const visibleInput = firstVisibleInputAfter(labelElement);
          const input = attachedInput
            || (visibleInput?.dataset?.klicBundleInput === '1' || visibleInput?.dataset?.klicDekbandCheckbox === '1' ? null : visibleInput);
          if (!input) {{
            return;
          }}
          const targetMode = 'bundle-dekband';
          const existingRow = labelElement.parentElement?.querySelector?.('[data-klic-diverse-split="1"]');
          if (input.dataset.klicDiverseSplitMode === targetMode && existingRow) {{
            if (!labelElement.querySelector?.('[data-klic-diverse-labels="1"]')) {{
              labelElement.textContent = 'Bundel (aantal)';
            }}
            return;
          }}

          existingRow?.remove();
          delete input.dataset.klicDiverseSplitAttached;
          delete input.dataset.klicDiverseSplitMode;

          labelElement.textContent = '';
          labelElement.style.display = 'inline-flex';
          labelElement.style.flexDirection = 'column';
          labelElement.style.alignItems = 'flex-start';
          labelElement.style.gap = '15px';
          labelElement.style.verticalAlign = 'top';
          const diverseLabels = document.createElement('span');
          diverseLabels.dataset.klicDiverseLabels = '1';
          diverseLabels.style.display = 'contents';
          const bundleLabel = document.createElement('span');
          bundleLabel.textContent = 'Bundel (aantal)';
          const dekbandLabel = document.createElement('span');
          dekbandLabel.textContent = 'Dekband';
          diverseLabels.appendChild(bundleLabel);
          diverseLabels.appendChild(dekbandLabel);
          labelElement.appendChild(diverseLabels);

          const row = document.createElement('div');
          row.dataset.klicDiverseSplit = '1';
          row.style.display = 'inline-flex';
          row.style.flexDirection = 'column';
          row.style.alignItems = 'stretch';
          row.style.gap = '7px';
          row.style.verticalAlign = 'top';
          row.style.maxWidth = '104px';
          row.style.position = 'relative';
          row.style.left = '-14px';

          const rawValue = normalizeText(input.value || '');
          const startsAsDekband = rawValue.toLowerCase() === 'dekband';

          const bundleInput = document.createElement('input');
          bundleInput.type = 'text';
          bundleInput.inputMode = 'numeric';
          bundleInput.autocomplete = 'off';
          bundleInput.dataset.klicBundleInput = '1';
          bundleInput.style.font = window.getComputedStyle(input).font;
          bundleInput.style.color = window.getComputedStyle(input).color;
          bundleInput.style.background = window.getComputedStyle(input).backgroundColor || '#fff';
          bundleInput.style.border = window.getComputedStyle(input).border || '1px solid #bbb';
          bundleInput.style.borderRadius = window.getComputedStyle(input).borderRadius || '3px';
          bundleInput.style.height = `${{Math.max(30, input.getBoundingClientRect().height || 30)}}px`;
          bundleInput.value = startsAsDekband ? '' : input.value;
          compactField(bundleInput);
          bundleInput.style.setProperty('left', '14px', 'important');
          row.appendChild(bundleInput);

          const checkRow = document.createElement('label');
          checkRow.dataset.klicDekbandRow = '1';
          checkRow.style.display = 'flex';
          checkRow.style.alignItems = 'center';
          checkRow.style.gap = '0';
          checkRow.style.minHeight = '22px';
          checkRow.style.whiteSpace = 'nowrap';
          checkRow.style.cursor = 'pointer';
          checkRow.style.color = window.getComputedStyle(labelElement).color;
          checkRow.style.font = window.getComputedStyle(labelElement).font;

          const dekbandCheck = document.createElement('input');
          dekbandCheck.type = 'checkbox';
          dekbandCheck.dataset.klicDekbandCheckbox = '1';
          dekbandCheck.checked = startsAsDekband;
          dekbandCheck.style.width = '16px';
          dekbandCheck.style.height = '16px';
          dekbandCheck.style.margin = '0';

          checkRow.appendChild(dekbandCheck);
          row.appendChild(checkRow);

          const syncHiddenValue = () => {{
            if (dekbandCheck.checked) {{
              setNativeValue(input, 'dekband');
              return;
            }}
            setNativeValue(input, bundleInput.value || '');
          }};

          bundleInput.addEventListener('input', syncHiddenValue);
          bundleInput.addEventListener('change', syncHiddenValue);
          dekbandCheck.addEventListener('change', syncHiddenValue);

          input.dataset.klicDiverseSplitAttached = '1';
          input.dataset.klicDiverseSplitMode = targetMode;
          input.style.position = 'absolute';
          input.style.left = '-10000px';
          input.style.width = '1px';
          input.style.height = '1px';
          input.style.opacity = '0';
          input.tabIndex = -1;
          input.setAttribute('aria-hidden', 'true');
          input.insertAdjacentElement('afterend', row);
          syncHiddenValue();
        }});
      }};

      const shortenKickTheMapFields = () => {{
        const targetLabels = new Set([...labelMap.values(), 'Dekband']);
        document.querySelectorAll('body *').forEach((element) => {{
          if (!(element instanceof HTMLElement) || element.children.length !== 0) {{
            return;
          }}
          if (element.closest('[data-klic-diverse-split="1"], [data-klic-diverse-labels="1"]')) {{
            return;
          }}
          if (!targetLabels.has(normalizeText(element.textContent || ''))) {{
            return;
          }}
          const input = firstVisibleInputAfter(element);
          if (input && input.dataset.klicProfileDropdownAttached !== '1' && input.dataset.klicMaterialDropdownAttached !== '1' && input.dataset.klicDiverseSplitAttached !== '1') {{
            compactField(input);
          }}
        }});
        document.querySelectorAll('select[data-klic-profile-dropdown], select[data-klic-material-dropdown], input[data-klic-bundle-input]').forEach(compactField);
      }};

      const normalizeFieldLayout = () => {{
        const targetLabels = new Set([...labelMap.values(), 'Dekband']);
        const labelElements = [];
        document.querySelectorAll('body *').forEach((element) => {{
          if (!(element instanceof HTMLElement)) {{
            return;
          }}
          if (element.children.length !== 0) {{
            return;
          }}
          if (element.closest('[data-klic-diverse-split="1"], [data-klic-diverse-labels="1"]')) {{
            return;
          }}
          const text = normalizeText(element.textContent || '');
          if (targetLabels.has(text)) {{
            labelElements.push(element);
          }}
        }});

        if (!labelElements.length) {{
          return;
        }}

        const preferredWidth = 96;

        labelElements.forEach((element) => {{
          element.style.display = 'inline-block';
          element.style.width = `${{preferredWidth}}px`;
          element.style.minWidth = `${{preferredWidth}}px`;
          element.style.maxWidth = `${{preferredWidth}}px`;
          element.style.whiteSpace = 'nowrap';
          element.style.verticalAlign = 'top';
          element.style.boxSizing = 'border-box';
        }});
      }};

      if (!window.__klicKickTheMapBrowserPatched) {{
        window.__klicKickTheMapBrowserPatched = true;

        window.open = function(url) {{
          return navigateSameWindow(url);
        }};

        document.addEventListener('click', (event) => {{
          if (event.defaultPrevented || event.button !== 0) {{
            return;
          }}
          if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {{
            return;
          }}
          const anchor = event.target instanceof Element ? event.target.closest('a[href]') : null;
          if (!anchor || anchor.hasAttribute('download')) {{
            return;
          }}
          const href = String(anchor.getAttribute('href') || '').trim();
          if (!href || href.startsWith('#') || href.startsWith('javascript:') || href.startsWith('mailto:') || href.startsWith('tel:')) {{
            return;
          }}
          event.preventDefault();
          navigateSameWindow(anchor.href || href);
        }}, true);
      }}

      const applyKickTheMapBrowserChrome = () => {{
        normalizeAnchors();
        relabelKickTheMapFields();
        normalizeFieldLayout();
        applyProfileNameDropdown();
        applyMaterialDropdown();
        applyDiverseSplit();
        shortenKickTheMapFields();
      }};

      applyKickTheMapBrowserChrome();

      if (!window.__klicKickTheMapObserverAdded) {{
        window.__klicKickTheMapObserverAdded = true;
        const observer = new MutationObserver(() => {{
          if (!window.__klicKickTheMapMutationScheduled) {{
            window.__klicKickTheMapMutationScheduled = true;
            window.requestAnimationFrame(() => {{
              window.__klicKickTheMapMutationScheduled = false;
              applyKickTheMapBrowserChrome();
            }});
          }}
        }});
        observer.observe(document.documentElement || document.body, {{ childList: true, subtree: true }});
      }}

      const currentPath = window.location.pathname || '';
      const autoLoginCounterKey = 'klic-kickthemap-auto-login-count';
      const shouldRememberStartUrl = START_URL && START_URL !== HOME_URL && !START_URL.includes(SIGNIN_PATH);
      if (shouldRememberStartUrl && currentPath.includes(SIGNIN_PATH)) {{
        sessionStorage.setItem(desiredUrlKey, START_URL);
      }}

      if (!currentPath.includes(SIGNIN_PATH)) {{
        sessionStorage.removeItem(autoLoginCounterKey);
        const desiredUrl = sessionStorage.getItem(desiredUrlKey);
        if (desiredUrl) {{
          const currentUrl = new URL(window.location.href);
          const targetUrl = new URL(desiredUrl, window.location.href);
          if (currentUrl.href !== targetUrl.href) {{
            sessionStorage.removeItem(desiredUrlKey);
            window.location.href = targetUrl.href;
            return 'redirecting-to-requested-job';
          }}
          sessionStorage.removeItem(desiredUrlKey);
        }}
        return 'patched';
      }}

      if (!EMAIL || !PASSWORD) {{
        return 'signin-no-credentials';
      }}

      const attempts = Number(sessionStorage.getItem(autoLoginCounterKey) || '0');
      if (attempts >= 1) {{
        return 'signin-already-tried';
      }}

      const emailInput = document.querySelector('input[name="email"], input[type="email"]');
      const passwordInput = document.querySelector('input[name="password"], input[type="password"]');
      if (!emailInput || !passwordInput) {{
        return 'signin-form-missing';
      }}

      sessionStorage.setItem(autoLoginCounterKey, String(attempts + 1));
      emailInput.focus();
      emailInput.value = EMAIL;
      emailInput.dispatchEvent(new Event('input', {{ bubbles: true }}));
      emailInput.dispatchEvent(new Event('change', {{ bubbles: true }}));
      passwordInput.value = PASSWORD;
      passwordInput.dispatchEvent(new Event('input', {{ bubbles: true }}));
      passwordInput.dispatchEvent(new Event('change', {{ bubbles: true }}));

      const form = emailInput.form || passwordInput.form || document.querySelector('form');
      if (form) {{
        form.submit();
        return 'signin-submitted';
      }}

      const submitButton = document.querySelector('button[type="submit"], input[type="submit"]');
      if (submitButton) {{
        submitButton.click();
        return 'signin-clicked';
      }}

      return 'signin-filled';
    }})()
    """


def _inject_browser_script(
    window: webview.Window,
    account: KickTheMapSavedAccount | None,
    start_url: str | None = None,
    *,
    prelogin: bool = False,
) -> None:
    try:
        result = window.evaluate_js(_injected_script(account, start_url))
        if prelogin and result == "patched":
            window.destroy()
    except Exception:
        pass


def _browser_go_back() -> None:
    active_window = webview.active_window()
    if active_window is None:
        return
    try:
        active_window.run_js("history.back();")
    except Exception:
        pass


def _browser_go_home() -> None:
    active_window = webview.active_window()
    if active_window is None:
        return
    try:
        active_window.load_url(HOME_URL)
    except Exception:
        pass


def _browser_refresh() -> None:
    active_window = webview.active_window()
    if active_window is None:
        return
    try:
        active_window.run_js("window.location.reload();")
    except Exception:
        pass


def main(start_url: str | None = None, window_title: str | None = None, *, prelogin: bool = False) -> None:
    account = _selected_account()
    storage_path = _storage_path_for_account(account)
    webview.settings["OPEN_EXTERNAL_LINKS_IN_BROWSER"] = False
    webview.settings["SHOW_DEFAULT_MENUS"] = False
    title = (window_title or "").strip()
    if not title:
        title = WINDOW_TITLE if account is None else f"{WINDOW_TITLE} - {account.email}"
    initial_url = start_url or (KickTheMapClient.SIGNIN_URL if prelogin else HOME_URL)
    menu = [
        Menu(
            "Navigatie",
            [
                MenuAction("Terug", _browser_go_back),
                MenuAction("Jobs", _browser_go_home),
                MenuSeparator(),
                MenuAction("Vernieuwen", _browser_refresh),
            ],
        )
    ]
    window = webview.create_window(
        title,
        url=initial_url,
        width=WINDOW_WIDTH,
        height=WINDOW_HEIGHT,
        min_size=WINDOW_MIN_SIZE,
        hidden=prelogin,
        text_select=True,
        background_color="#FFFFFF",
        menu=menu,
    )
    window.events.loaded += lambda window: _inject_browser_script(window, account, start_url, prelogin=prelogin)
    webview.start(
        gui=None,
        debug=False,
        private_mode=False,
        storage_path=storage_path,
    )
