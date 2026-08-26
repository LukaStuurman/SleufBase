from __future__ import annotations

import base64
import struct
import zlib
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parent
ASSETS = ROOT / "assets"
PART_PATTERN = "techbase-logo.part*.b64"
PNG_PATH = ASSETS / "techbase-logo.png"
ICO_PATH = ASSETS / "sleufbase_icon.ico"
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def _validate_png(raw: bytes) -> None:
    if not raw.startswith(PNG_SIGNATURE):
        raise ValueError("Techbase-logo heeft geen geldige PNG-signature")

    offset = len(PNG_SIGNATURE)
    chunk_index = 0
    saw_iend = False
    while offset < len(raw):
        if offset + 12 > len(raw):
            raise ValueError(f"PNG afgebroken bij byte {offset}: onvoldoende bytes voor chunk-header")
        length = struct.unpack(">I", raw[offset : offset + 4])[0]
        chunk_type = raw[offset + 4 : offset + 8]
        end = offset + 12 + length
        if end > len(raw):
            raise ValueError(
                f"PNG chunk {chunk_index} {chunk_type!r} bij byte {offset} claimt {length} bytes, "
                f"maar bestand eindigt op {len(raw)}"
            )
        if not all(65 <= byte <= 90 or 97 <= byte <= 122 for byte in chunk_type):
            raise ValueError(
                f"PNG chunk-type ongeldig bij byte {offset}: {chunk_type!r}; "
                f"waarschijnlijk beschadigde base64-bron"
            )
        data = raw[offset + 8 : offset + 8 + length]
        expected_crc = struct.unpack(">I", raw[offset + 8 + length : end])[0]
        actual_crc = zlib.crc32(chunk_type)
        actual_crc = zlib.crc32(data, actual_crc) & 0xFFFFFFFF
        if actual_crc != expected_crc:
            raise ValueError(
                f"PNG CRC-fout in chunk {chunk_index} {chunk_type.decode('ascii', 'replace')} "
                f"bij byte {offset}; verwacht {expected_crc:08x}, berekend {actual_crc:08x}"
            )
        offset = end
        chunk_index += 1
        if chunk_type == b"IEND":
            saw_iend = True
            break

    if not saw_iend:
        raise ValueError(f"PNG mist IEND-chunk; parser stopte bij byte {offset} van {len(raw)}")
    if offset != len(raw):
        raise ValueError(f"PNG bevat {len(raw) - offset} onverwachte bytes na IEND")


def main() -> int:
    parts = sorted(ASSETS.glob(PART_PATTERN))
    if not parts:
        raise FileNotFoundError(f"Geen Techbase-logo bronbestanden gevonden: {ASSETS / PART_PATTERN}")

    encoded = "".join(part.read_text(encoding="ascii") for part in parts)
    encoded = "".join(encoded.split())
    raw = base64.b64decode(encoded, validate=True)
    print(f"Techbase base64: {len(parts)} delen, {len(encoded)} tekens, {len(raw)} bytes")
    _validate_png(raw)
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
