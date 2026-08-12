from __future__ import annotations

import base64
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parent
ASSETS = ROOT / "assets"
PART_PATTERN = "techbase-logo.part*.b64"
PNG_PATH = ASSETS / "techbase-logo.png"
ICO_PATH = ASSETS / "sleufbase_icon.ico"


def main() -> int:
    parts = sorted(ASSETS.glob(PART_PATTERN))
    if not parts:
        raise FileNotFoundError(f"Geen Techbase-logo bronbestanden gevonden: {ASSETS / PART_PATTERN}")

    encoded = "".join(part.read_text(encoding="ascii") for part in parts)
    encoded = "".join(encoded.split())
    raw = base64.b64decode(encoded, validate=True)
    PNG_PATH.write_bytes(raw)

    with Image.open(PNG_PATH) as source:
        image = source.convert("RGBA")
        # Windows Explorer/taskbar uses several icon sizes. Supplying the full
        # set avoids blurry scaling from a single bitmap.
        image.save(
            ICO_PATH,
            format="ICO",
            sizes=[
                (16, 16),
                (24, 24),
                (32, 32),
                (48, 48),
                (64, 64),
                (128, 128),
                (256, 256),
            ],
        )

    print(f"Techbase logo: {PNG_PATH} ({PNG_PATH.stat().st_size} bytes)")
    print(f"Windows icon: {ICO_PATH} ({ICO_PATH.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
