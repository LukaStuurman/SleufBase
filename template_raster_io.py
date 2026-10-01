from __future__ import annotations

import os
from pathlib import Path
import threading
import time

from PIL import Image


TEMPLATE_PNG_COMPRESS_LEVEL = 1


def save_template_png(image: Image.Image, path: str | Path) -> None:
    """Publish a complete, lossless template raster with bounded encoding work."""
    destination = Path(path)
    temporary = destination.with_name(
        f".{destination.name}.{os.getpid()}.{threading.get_ident()}.{time.time_ns()}.tmp"
    )
    try:
        image.save(
            temporary, format="PNG",
            compress_level=TEMPLATE_PNG_COMPRESS_LEVEL, optimize=False,
        )
        os.replace(temporary, destination)
    finally:
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            # Retain the original encoding/publication error if cleanup fails.
            pass
