from __future__ import annotations

import math
from pathlib import Path

import numpy as np

from PIL import Image

from .models import Bounds, GeoTiffLayer, GeoTransform

Image.MAX_IMAGE_PIXELS = None

MODEL_PIXEL_SCALE = 33550
MODEL_TIEPOINT = 33922
MODEL_TRANSFORMATION = 34264
GEO_KEY_DIRECTORY = 34735
GEO_ASCII_PARAMS = 34737


class GeoTiffError(RuntimeError):
    """Raised when a TIFF file has no readable spatial reference."""


def _to_float_sequence(value: object) -> tuple[float, ...]:
    if value is None:
        return tuple()
    if isinstance(value, (list, tuple)):
        return tuple(float(item) for item in value)
    return (float(value),)


def _parse_epsg(tags) -> int | None:
    directory = tags.get(GEO_KEY_DIRECTORY)
    if not directory:
        return None
    values = [int(item) for item in directory]
    if len(values) < 4:
        return None
    number_of_keys = values[3]
    for index in range(number_of_keys):
        start = 4 + index * 4
        if start + 4 > len(values):
            break
        key_id, location, count, offset = values[start : start + 4]
        if key_id not in {2048, 3072}:
            continue
        if location == 0:
            return offset
        if location == GEO_ASCII_PARAMS:
            ascii_values = tags.get(GEO_ASCII_PARAMS, "")
            raw = str(ascii_values).split("|")
            try:
                return int(raw[offset][:count])
            except (IndexError, ValueError):
                return None
    return None


def _build_transform(tags) -> GeoTransform:
    if MODEL_TRANSFORMATION in tags:
        matrix = _to_float_sequence(tags[MODEL_TRANSFORMATION])
        if len(matrix) >= 16:
            return GeoTransform(
                a=matrix[0],
                b=matrix[1],
                c=matrix[3],
                d=matrix[4],
                e=matrix[5],
                f=matrix[7],
            )
    scale = _to_float_sequence(tags.get(MODEL_PIXEL_SCALE))
    tiepoint = _to_float_sequence(tags.get(MODEL_TIEPOINT))
    if len(scale) >= 2 and len(tiepoint) >= 6:
        pixel_x, pixel_y = tiepoint[0], tiepoint[1]
        world_x, world_y = tiepoint[3], tiepoint[4]
        scale_x, scale_y = scale[0], scale[1]
        return GeoTransform(
            a=scale_x,
            b=0.0,
            c=world_x - pixel_x * scale_x,
            d=0.0,
            e=-scale_y,
            f=world_y + pixel_y * scale_y,
        )
    raise GeoTiffError("Geen GeoTIFF-transformatie gevonden. Gebruik een TIFF met RD-georeferentie.")


def _calculate_bounds(transform: GeoTransform, width: int, height: int) -> Bounds:
    corners = [
        transform.pixel_to_world(0, 0),
        transform.pixel_to_world(width, 0),
        transform.pixel_to_world(width, height),
        transform.pixel_to_world(0, height),
    ]
    xs = [point[0] for point in corners]
    ys = [point[1] for point in corners]
    return Bounds(min(xs), min(ys), max(xs), max(ys))


def _transform_from_matrix(matrix: np.ndarray) -> GeoTransform:
    return GeoTransform(
        a=float(matrix[0, 0]),
        b=float(matrix[0, 1]),
        c=float(matrix[0, 2]),
        d=float(matrix[1, 0]),
        e=float(matrix[1, 1]),
        f=float(matrix[1, 2]),
    )


def _normalize_rotation_degrees(angle_degrees: float) -> float:
    normalized = math.fmod(float(angle_degrees), 360.0)
    if normalized <= -180.0:
        normalized += 360.0
    elif normalized > 180.0:
        normalized -= 360.0
    if abs(normalized) < 1e-9:
        return 0.0
    return normalized


def _matrix_to_metadata_value(matrix: np.ndarray) -> tuple[float, ...]:
    return tuple(float(value) for value in matrix.reshape(-1))


def _matrix_from_metadata_value(value: object) -> np.ndarray | None:
    if not isinstance(value, (list, tuple)) or len(value) != 9:
        return None
    try:
        matrix = np.array([float(item) for item in value], dtype=float).reshape((3, 3))
    except (TypeError, ValueError):
        return None
    return matrix


def _rotation_matrix_around_point(center_x: float, center_y: float, angle_degrees: float) -> np.ndarray:
    angle_radians = math.radians(angle_degrees)
    cos_angle = math.cos(angle_radians)
    sin_angle = math.sin(angle_radians)
    return np.array(
        [
            [cos_angle, -sin_angle, center_x - (cos_angle * center_x) + (sin_angle * center_y)],
            [sin_angle, cos_angle, center_y - (sin_angle * center_x) - (cos_angle * center_y)],
            [0.0, 0.0, 1.0],
        ],
        dtype=float,
    )


def set_geotiff_layer_rotation(layer: GeoTiffLayer, angle_degrees: float) -> GeoTiffLayer:
    normalized_angle = _normalize_rotation_degrees(angle_degrees)
    metadata = dict(layer.metadata)
    base_transform_matrix = _matrix_from_metadata_value(metadata.get("base_transform_matrix"))
    if base_transform_matrix is None:
        base_transform_matrix = layer.transform.to_matrix()
        metadata["base_transform_matrix"] = _matrix_to_metadata_value(base_transform_matrix)

    base_transform = _transform_from_matrix(base_transform_matrix)
    center_x, center_y = base_transform.pixel_to_world(layer.image.width / 2.0, layer.image.height / 2.0)
    world_rotation = _rotation_matrix_around_point(center_x, center_y, normalized_angle)
    rotated_transform = _transform_from_matrix(world_rotation @ base_transform_matrix)
    rotated_bounds = _calculate_bounds(rotated_transform, layer.image.width, layer.image.height)
    metadata["rotation_degrees"] = normalized_angle

    return GeoTiffLayer(
        path=layer.path,
        image=layer.image,
        transform=rotated_transform,
        bounds=rotated_bounds,
        epsg=layer.epsg,
        opacity=layer.opacity,
        metadata=metadata,
    )


def rotate_geotiff_layer(layer: GeoTiffLayer, angle_degrees: float) -> GeoTiffLayer:
    normalized_delta = _normalize_rotation_degrees(angle_degrees)
    if normalized_delta == 0.0:
        return layer
    current_rotation = 0.0
    try:
        current_rotation = float(layer.metadata.get("rotation_degrees", 0.0))
    except (TypeError, ValueError):
        current_rotation = 0.0
    return set_geotiff_layer_rotation(layer, current_rotation + normalized_delta)


def load_geotiff(path: str | Path) -> GeoTiffLayer:
    file_path = Path(path)
    image = Image.open(file_path)
    tags = getattr(image, "tag_v2", None)
    if tags is None:
        raise GeoTiffError("Dit TIFF-bestand bevat geen leesbare GeoTIFF-tags.")
    transform = _build_transform(tags)
    bounds = _calculate_bounds(transform, image.width, image.height)
    return GeoTiffLayer(
        path=file_path,
        image=image,
        transform=transform,
        bounds=bounds,
        epsg=_parse_epsg(tags),
        metadata={
            "breedte_px": image.width,
            "hoogte_px": image.height,
        },
    )
