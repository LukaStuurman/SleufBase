from __future__ import annotations

import base64
import json
import os
import re
from dataclasses import dataclass
from io import BytesIO
from typing import Any

import requests
from PIL import Image, ImageDraw, ImageFont

from .models import Bounds, DxfOverlay, GeoTiffLayer, ViewportTransform
from .renderer import MapRenderer
from .vllm_runtime import DEFAULT_VLLM_BASE_URL, DEFAULT_VLLM_MODEL, ensure_vllm_server


DEFAULT_MAAIVELD_COLOR = "#808080"


class AiMaaiveldError(RuntimeError):
    """Raised when the local vLLM maaiveld lookup cannot be completed."""


@dataclass(frozen=True)
class AiMaaiveldPoint:
    key: str
    label: str
    x: float
    y: float


@dataclass(frozen=True)
class AiMaaiveldResult:
    text_by_key: dict[str, str]
    raw_response: str


def build_numbered_maaiveld_image(
    *,
    layer: GeoTiffLayer,
    points: list[AiMaaiveldPoint],
    renderer: MapRenderer,
    dxf_overlays: list[DxfOverlay] | None = None,
    background_provider: Any | None = None,
    size: tuple[int, int] = (1400, 1000),
) -> Image.Image:
    if not points:
        raise AiMaaiveldError("Geen maaiveldpunten beschikbaar om te markeren.")

    point_bounds = _bounds_for_points(points)
    combined_bounds = layer.bounds.union(point_bounds)
    padding = max(2.0, min(10.0, max(combined_bounds.width, combined_bounds.height) * 0.18))
    map_bounds = combined_bounds.padded(padding).expand_to_aspect_ratio(size[0] / size[1])
    background = None
    if background_provider is not None:
        try:
            background = background_provider.fetch_map(map_bounds, size)
        except Exception:
            background = None
    map_image = renderer.render(
        map_bounds,
        size,
        [layer],
        dxf_overlays or [],
        background=background,
        map_comments=None,
    ).convert("RGBA")

    draw = ImageDraw.Draw(map_image, "RGBA")
    transform = ViewportTransform(map_bounds, size[0], size[1])
    font = _load_font(48, bold=True)
    small_font = _load_font(24, bold=True)
    radius = 34
    for index, point in enumerate(points, start=1):
        sx, sy = transform.world_to_screen(point.x, point.y)
        x = int(round(sx))
        y = int(round(sy))
        draw.ellipse(
            (x - radius, y - radius, x + radius, y + radius),
            fill=(255, 255, 255, 245),
            outline=(0, 0, 0, 255),
            width=5,
        )
        text = str(index)
        bbox = draw.textbbox((0, 0), text, font=font)
        draw.text(
            (x - ((bbox[2] - bbox[0]) / 2), y - ((bbox[3] - bbox[1]) / 2) - 4),
            text,
            font=font,
            fill=(0, 0, 0, 255),
        )
        label = point.label
        label_bbox = draw.textbbox((0, 0), label, font=small_font)
        label_x = max(8, min(size[0] - (label_bbox[2] - label_bbox[0]) - 8, x - ((label_bbox[2] - label_bbox[0]) / 2)))
        label_y = min(size[1] - (label_bbox[3] - label_bbox[1]) - 8, y + radius + 8)
        draw.rounded_rectangle(
            (
                label_x - 8,
                label_y - 4,
                label_x + (label_bbox[2] - label_bbox[0]) + 8,
                label_y + (label_bbox[3] - label_bbox[1]) + 6,
            ),
            radius=6,
            fill=(255, 255, 255, 225),
        )
        draw.text((label_x, label_y), label, font=small_font, fill=(0, 0, 0, 255))
    return map_image.convert("RGB")


def request_maaiveld_from_vllm(
    image: Image.Image,
    *,
    model: str | None = None,
    base_url: str | None = None,
    api_key: str | None = None,
    timeout_seconds: int = 180,
) -> AiMaaiveldResult:
    model_name = (model or os.environ.get("SLEUFBASE_VLLM_MODEL") or DEFAULT_VLLM_MODEL).strip()
    root_url = (base_url or os.environ.get("SLEUFBASE_VLLM_BASE_URL") or DEFAULT_VLLM_BASE_URL).rstrip("/")
    token = api_key if api_key is not None else os.environ.get("SLEUFBASE_VLLM_API_KEY", "EMPTY")
    if not model_name:
        raise AiMaaiveldError("Geen vLLM-model ingesteld.")
    if not root_url:
        raise AiMaaiveldError("Geen vLLM endpoint ingesteld.")
    try:
        root_url = ensure_vllm_server(base_url=root_url, model=model_name)
    except Exception as exc:
        raise AiMaaiveldError(str(exc)) from exc

    image_url = _image_to_data_url(image)
    prompt = (
        "Lees de afbeelding van een proefsleuf. Er staan drie witte genummerde markers op: "
        "1 = begin, 2 = midden, 3 = einde. Bepaal per marker het maaiveld/N.A.P.-niveau dat bij die plek hoort. "
        "Geef alleen geldig JSON terug met exact deze vorm: "
        '{"1":"Maaiveld: 0.00 N.A.P.","2":"Maaiveld: 0.00 N.A.P.","3":"Maaiveld: 0.00 N.A.P."}. '
        "Gebruik een komma als decimaalteken alleen als die zo in de afbeelding staat; anders punt. "
        "Als je een waarde niet betrouwbaar kunt bepalen, zet dan INVULLEN."
    )
    payload = {
        "model": model_name,
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": image_url}},
                ],
            }
        ],
        "temperature": 0,
        "max_tokens": 256,
    }
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        response = requests.post(
            f"{root_url}/chat/completions",
            headers=headers,
            json=payload,
            timeout=timeout_seconds,
        )
        response.raise_for_status()
        data = response.json()
    except requests.RequestException as exc:
        raise AiMaaiveldError(f"vLLM-aanroep mislukt: {exc}") from exc
    except ValueError as exc:
        raise AiMaaiveldError("vLLM gaf geen geldige JSON-response terug.") from exc

    try:
        content = data["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as exc:
        raise AiMaaiveldError("vLLM-response bevat geen chatbericht.") from exc
    if isinstance(content, list):
        content = "".join(str(item.get("text", "")) if isinstance(item, dict) else str(item) for item in content)
    raw_text = str(content or "").strip()
    parsed = _parse_maaiveld_json(raw_text)
    return AiMaaiveldResult(text_by_key=parsed, raw_response=raw_text)


def number_key_to_segment_key(number_key: str) -> str | None:
    return {"1": "start", "2": "middle", "3": "end"}.get(str(number_key).strip())


def _bounds_for_points(points: list[AiMaaiveldPoint]) -> Bounds:
    xs = [point.x for point in points]
    ys = [point.y for point in points]
    return Bounds(min(xs), min(ys), max(xs), max(ys))


def _image_to_data_url(image: Image.Image) -> str:
    buffer = BytesIO()
    image.save(buffer, format="PNG")
    encoded = base64.b64encode(buffer.getvalue()).decode("ascii")
    return f"data:image/png;base64,{encoded}"


def _parse_maaiveld_json(raw_text: str) -> dict[str, str]:
    candidate = raw_text.strip()
    fenced = re.search(r"```(?:json)?\s*(.*?)```", candidate, flags=re.IGNORECASE | re.DOTALL)
    if fenced is not None:
        candidate = fenced.group(1).strip()
    if not candidate.startswith("{"):
        start = candidate.find("{")
        end = candidate.rfind("}")
        if start >= 0 and end > start:
            candidate = candidate[start : end + 1]
    try:
        data: Any = json.loads(candidate)
    except ValueError as exc:
        raise AiMaaiveldError(f"vLLM gaf geen bruikbare maaiveld-JSON terug: {raw_text[:300]}") from exc
    if not isinstance(data, dict):
        raise AiMaaiveldError("vLLM-JSON moet een object met keys 1, 2 en 3 zijn.")
    result: dict[str, str] = {}
    for number_key in ("1", "2", "3"):
        value = data.get(number_key)
        if value is None:
            value = data.get(int(number_key))
        text = str(value or "").strip() or "INVULLEN"
        result[number_key] = text
    return result


def _load_font(size: int, *, bold: bool = False) -> ImageFont.ImageFont:
    candidates = (
        "C:/Windows/Fonts/arialbd.ttf" if bold else "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/segoeuib.ttf" if bold else "C:/Windows/Fonts/segoeui.ttf",
    )
    for candidate in candidates:
        try:
            return ImageFont.truetype(candidate, size)
        except OSError:
            continue
    return ImageFont.load_default()
