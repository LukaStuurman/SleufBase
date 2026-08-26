from __future__ import annotations

import ctypes
from datetime import datetime, timezone
import json
import logging
import os
import platform
import sys
import threading
import time
import traceback
import uuid
from logging.handlers import RotatingFileHandler
from pathlib import Path
from typing import Any

from SleufBase.version import APP_USER_MODEL_ID, COMPANY_NAME, PRODUCT_NAME, __version__

_START_TIME = time.perf_counter()
_LOGGER = logging.getLogger("SleufBase")
_LOG_PATH: Path | None = None


def app_data_dir() -> Path:
    base = os.environ.get("LOCALAPPDATA")
    if base:
        root = Path(base)
    else:
        root = Path.home() / "AppData" / "Local"
    return root / COMPANY_NAME / PRODUCT_NAME


def logs_dir() -> Path:
    return app_data_dir() / "logs"


def diagnostics_dir() -> Path:
    return app_data_dir() / "diagnostics"


def configure_environment() -> None:
    data = app_data_dir()
    log_dir = logs_dir()
    diag_dir = diagnostics_dir()
    for path in (data, log_dir, diag_dir):
        path.mkdir(parents=True, exist_ok=True)
    os.environ.setdefault("SLEUFBASE_DATA_DIR", str(data))
    os.environ.setdefault("SLEUFBASE_LOG_DIR", str(log_dir))
    os.environ.setdefault("SLEUFBASE_DIAGNOSTICS_DIR", str(diag_dir))


def configure_logging() -> Path:
    global _LOG_PATH
    configure_environment()
    path = logs_dir() / "sleufbase.log"
    _LOG_PATH = path

    root = logging.getLogger()
    root.setLevel(logging.INFO)
    if not any(isinstance(handler, RotatingFileHandler) for handler in root.handlers):
        handler = RotatingFileHandler(
            path,
            maxBytes=2 * 1024 * 1024,
            backupCount=5,
            encoding="utf-8",
        )
        handler.setFormatter(
            logging.Formatter(
                "%(asctime)s | %(levelname)s | %(name)s | %(threadName)s | %(message)s"
            )
        )
        root.addHandler(handler)

    _LOGGER.info(
        "SleufBase %s start | frozen=%s | python=%s | platform=%s",
        __version__,
        bool(getattr(sys, "frozen", False)),
        platform.python_version(),
        platform.platform(),
    )
    return path


def _windows_message_box(title: str, message: str, *, error: bool = False) -> None:
    if os.name != "nt":
        return
    try:
        flags = 0x00000010 if error else 0x00000040
        ctypes.windll.user32.MessageBoxW(None, message, title, flags)
    except Exception:
        pass


def install_windows_integration() -> None:
    if os.name != "nt":
        return

    try:
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(APP_USER_MODEL_ID)
    except Exception:
        _LOGGER.debug("AppUserModelID kon niet worden ingesteld", exc_info=True)

    # Per-monitor-v2 DPI awareness on modern Windows. Fall back gracefully on
    # older hosts/Citrix images where the API may not exist.
    try:
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except Exception:
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except Exception:
            _LOGGER.debug("DPI awareness kon niet worden ingesteld", exc_info=True)


def _atomic_write_text(path: Path, content: str) -> None:
    temp_path = path.with_name(f".{path.name}.{os.getpid()}.{time.time_ns()}.tmp")
    try:
        temp_path.write_text(content, encoding="utf-8")
        os.replace(temp_path, path)
    finally:
        try:
            temp_path.unlink(missing_ok=True)
        except OSError:
            pass


def _write_crash_report(exc_type: type[BaseException], exc: BaseException, tb: Any) -> Path:
    configure_environment()
    now = datetime.now(timezone.utc)
    stamp = now.strftime("%Y%m%d-%H%M%S-%f")
    unique = uuid.uuid4().hex[:8]
    path = diagnostics_dir() / f"crash-{stamp}-p{os.getpid()}-{unique}.txt"
    lines = [
        f"Product: {PRODUCT_NAME}",
        f"Versie: {__version__}",
        f"Tijd UTC: {now.isoformat()}",
        f"Proces-ID: {os.getpid()}",
        f"Thread: {threading.current_thread().name}",
        f"Platform: {platform.platform()}",
        f"Python: {platform.python_version()}",
        f"Frozen: {bool(getattr(sys, 'frozen', False))}",
        "",
        "Traceback:",
        "".join(traceback.format_exception(exc_type, exc, tb)),
    ]
    _atomic_write_text(path, "\n".join(lines))
    return path


def _handle_uncaught_exception(exc_type: type[BaseException], exc: BaseException, tb: Any) -> None:
    if issubclass(exc_type, KeyboardInterrupt):
        sys.__excepthook__(exc_type, exc, tb)
        return

    _LOGGER.critical("Onverwerkte fout", exc_info=(exc_type, exc, tb))
    try:
        report = _write_crash_report(exc_type, exc, tb)
    except Exception:
        report = _LOG_PATH or logs_dir() / "sleufbase.log"

    _windows_message_box(
        f"{PRODUCT_NAME} - fout",
        "SleufBase heeft een onverwachte fout aangetroffen.\n\n"
        f"Diagnosebestand:\n{report}\n\n"
        "Stuur dit bestand mee wanneer je ondersteuning vraagt.",
        error=True,
    )


def _thread_exception_hook(args: threading.ExceptHookArgs) -> None:
    _handle_uncaught_exception(args.exc_type, args.exc_value, args.exc_traceback)


def install_exception_handlers() -> None:
    sys.excepthook = _handle_uncaught_exception
    if hasattr(threading, "excepthook"):
        threading.excepthook = _thread_exception_hook


def install_tk_exception_handler(app: Any) -> None:
    def report_callback_exception(exc_type: type[BaseException], exc: BaseException, tb: Any) -> None:
        _handle_uncaught_exception(exc_type, exc, tb)

    try:
        app.report_callback_exception = report_callback_exception
    except Exception:
        _LOGGER.debug("Tk callback exception handler kon niet worden ingesteld", exc_info=True)


def mark_startup_complete() -> None:
    elapsed = time.perf_counter() - _START_TIME
    _LOGGER.info("UI ready na %.3f seconden", elapsed)


def write_diagnostics() -> Path:
    configure_environment()
    data = {
        "product": PRODUCT_NAME,
        "version": __version__,
        "company": COMPANY_NAME,
        "app_user_model_id": APP_USER_MODEL_ID,
        "platform": platform.platform(),
        "python": platform.python_version(),
        "frozen": bool(getattr(sys, "frozen", False)),
        "executable": sys.executable,
        "data_dir": str(app_data_dir()),
        "log_path": str(_LOG_PATH or logs_dir() / "sleufbase.log"),
    }
    path = diagnostics_dir() / "system-info.json"
    _atomic_write_text(path, json.dumps(data, indent=2, ensure_ascii=False))
    return path


def initialize_professional_runtime() -> None:
    configure_logging()
    install_windows_integration()
    install_exception_handlers()
