from __future__ import annotations

import json
import os
from pathlib import Path
import threading
import time
import uuid
from typing import Any


def atomic_write_text(
    path: str | Path,
    content: str,
    *,
    encoding: str = "utf-8",
) -> Path:
    """Write text through a same-directory temporary file and atomically replace the target."""

    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(
        f".{target.name}.{os.getpid()}.{threading.get_ident()}.{time.time_ns()}.{uuid.uuid4().hex}.tmp"
    )
    try:
        with temporary.open("w", encoding=encoding, newline="") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, target)
    finally:
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            pass
    return target


def atomic_write_json(
    path: str | Path,
    payload: Any,
    *,
    ensure_ascii: bool = False,
    indent: int | None = 2,
    trailing_newline: bool = True,
) -> Path:
    content = json.dumps(payload, ensure_ascii=ensure_ascii, indent=indent)
    if trailing_newline:
        content += "\n"
    return atomic_write_text(path, content, encoding="utf-8")
