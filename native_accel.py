from __future__ import annotations

from ctypes import POINTER, byref, c_double, c_int, c_int32, c_int64, c_uint8, cdll
from pathlib import Path
import sys

import numpy as np


_UINT8_PTR = POINTER(c_uint8)
_INT32_PTR = POINTER(c_int32)
_DOUBLE_PTR = POINTER(c_double)


def _candidate_paths() -> list[Path]:
    module_dir = Path(__file__).resolve().parent
    paths = [module_dir / "native" / "ktk_accel.dll"]
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        base = Path(meipass)
        paths.extend(
            [
                base / "app" / "native" / "ktk_accel.dll",
                base / "native" / "ktk_accel.dll",
                base / "ktk_accel.dll",
            ]
        )
    return paths


def _load_library():
    for path in _candidate_paths():
        if path.exists():
            try:
                return cdll.LoadLibrary(str(path))
            except OSError:
                continue
    return None


_LIB = _load_library()

if _LIB is not None:
    _LIB.ktk_accel_version.argtypes = []
    _LIB.ktk_accel_version.restype = c_int

    _LIB.ktk_edge_connected_mask.argtypes = [_UINT8_PTR, c_int, c_int, _UINT8_PTR]
    _LIB.ktk_edge_connected_mask.restype = c_int

    _LIB.ktk_component_from_seed.argtypes = [_UINT8_PTR, c_int, c_int, c_int, c_int, _UINT8_PTR]
    _LIB.ktk_component_from_seed.restype = c_int

    _LIB.ktk_fill_holes.argtypes = [_UINT8_PTR, c_int, c_int, _UINT8_PTR]
    _LIB.ktk_fill_holes.restype = c_int

    _LIB.ktk_best_component.argtypes = [_UINT8_PTR, c_int, c_int, c_double, c_double, _UINT8_PTR, POINTER(c_double)]
    _LIB.ktk_best_component.restype = c_int

    _LIB.ktk_boundary_edge_count.argtypes = [_UINT8_PTR, c_int, c_int]
    _LIB.ktk_boundary_edge_count.restype = c_int64

    _LIB.ktk_mask_to_loops.argtypes = [
        _UINT8_PTR,
        c_int,
        c_int,
        _INT32_PTR,
        c_int32,
        _INT32_PTR,
        c_int32,
        POINTER(c_int32),
        POINTER(c_int32),
    ]
    _LIB.ktk_mask_to_loops.restype = c_int

    _LIB.ktk_render_dxf_overlay.argtypes = [
        _UINT8_PTR,
        c_int,
        c_int,
        _DOUBLE_PTR,
        c_int32,
        _INT32_PTR,
        c_int32,
        _DOUBLE_PTR,
        _UINT8_PTR,
        _UINT8_PTR,
        _UINT8_PTR,
        c_double,
        c_double,
        c_double,
        c_double,
        c_double,
    ]
    _LIB.ktk_render_dxf_overlay.restype = c_int

    _LIB.ktk_paint_axis_aligned_tiff.argtypes = [
        _UINT8_PTR,
        c_int,
        c_int,
        _UINT8_PTR,
        c_int,
        c_int,
        c_double,
        c_double,
        c_double,
        c_double,
        c_int,
        c_int,
        c_int,
        c_int,
        c_double,
    ]
    _LIB.ktk_paint_axis_aligned_tiff.restype = c_int


def is_available() -> bool:
    return _LIB is not None


def _bool_array(mask: np.ndarray) -> np.ndarray:
    if mask.ndim != 2:
        raise ValueError("mask must be a 2D array")
    return np.ascontiguousarray(mask, dtype=np.bool_)


def _ptr(array: np.ndarray):
    return array.ctypes.data_as(_UINT8_PTR)


def _double_ptr(array: np.ndarray):
    return array.ctypes.data_as(_DOUBLE_PTR)


def _int32_ptr(array: np.ndarray):
    return array.ctypes.data_as(_INT32_PTR)


def edge_connected_mask(mask: np.ndarray) -> np.ndarray | None:
    if _LIB is None:
        return None
    source = _bool_array(mask)
    out = np.zeros(source.shape, dtype=np.bool_)
    ok = _LIB.ktk_edge_connected_mask(_ptr(source), source.shape[0], source.shape[1], _ptr(out))
    return out if ok == 1 else None


def component_from_seed(mask: np.ndarray, seed_row: int, seed_col: int) -> np.ndarray | None:
    if _LIB is None:
        return None
    source = _bool_array(mask)
    out = np.zeros(source.shape, dtype=np.bool_)
    ok = _LIB.ktk_component_from_seed(
        _ptr(source),
        source.shape[0],
        source.shape[1],
        int(seed_row),
        int(seed_col),
        _ptr(out),
    )
    return out if ok == 1 else None


def fill_holes(mask: np.ndarray) -> np.ndarray | None:
    if _LIB is None:
        return None
    source = _bool_array(mask)
    out = np.zeros(source.shape, dtype=np.bool_)
    ok = _LIB.ktk_fill_holes(_ptr(source), source.shape[0], source.shape[1], _ptr(out))
    return out if ok == 1 else None


def best_component(
    mask: np.ndarray,
    min_area_ratio: float,
    max_area_ratio: float,
) -> tuple[np.ndarray | None, float] | None:
    if _LIB is None:
        return None
    source = _bool_array(mask)
    out = np.zeros(source.shape, dtype=np.bool_)
    score = c_double(float("inf"))
    ok = _LIB.ktk_best_component(
        _ptr(source),
        source.shape[0],
        source.shape[1],
        float(min_area_ratio),
        float(max_area_ratio),
        _ptr(out),
        byref(score),
    )
    if ok != 1:
        return None
    return (out if bool(out.any()) else None, float(score.value))


def mask_to_loops(mask: np.ndarray) -> list[list[tuple[int, int]]] | None:
    if _LIB is None:
        return None
    source = _bool_array(mask)
    edge_count = int(_LIB.ktk_boundary_edge_count(_ptr(source), source.shape[0], source.shape[1]))
    if edge_count < 0:
        return None
    if edge_count == 0:
        return []
    if edge_count > 12_000_000:
        return None

    coords = np.empty((edge_count, 2), dtype=np.int32)
    loop_offsets = np.empty(edge_count + 1, dtype=np.int32)
    out_coord_count = c_int32(0)
    out_loop_count = c_int32(0)
    ok = _LIB.ktk_mask_to_loops(
        _ptr(source),
        source.shape[0],
        source.shape[1],
        coords.ctypes.data_as(_INT32_PTR),
        c_int32(edge_count),
        loop_offsets.ctypes.data_as(_INT32_PTR),
        c_int32(edge_count + 1),
        byref(out_coord_count),
        byref(out_loop_count),
    )
    if ok != 1:
        return None
    coord_count = int(out_coord_count.value)
    loop_count = int(out_loop_count.value)
    loops: list[list[tuple[int, int]]] = []
    for index in range(loop_count):
        start = int(loop_offsets[index])
        end = int(loop_offsets[index + 1]) if index + 1 <= loop_count else coord_count
        loop = [(int(x), int(y)) for x, y in coords[start:end]]
        if len(loop) >= 4:
            loops.append(loop)
    return loops


def render_dxf_overlay(
    rgba: np.ndarray,
    points_xy: np.ndarray,
    feature_offsets: np.ndarray,
    feature_bounds: np.ndarray,
    feature_colors: np.ndarray,
    selected_flags: np.ndarray,
    highlighted_flags: np.ndarray,
    view_bounds: tuple[float, float, float, float],
    meters_per_pixel: float,
) -> int | None:
    if _LIB is None:
        return None
    if rgba.ndim != 3 or rgba.shape[2] != 4 or rgba.dtype != np.uint8:
        return None
    if not rgba.flags.c_contiguous or not rgba.flags.writeable:
        return None

    points = np.ascontiguousarray(points_xy, dtype=np.float64)
    offsets = np.ascontiguousarray(feature_offsets, dtype=np.int32)
    bounds = np.ascontiguousarray(feature_bounds, dtype=np.float64)
    colors = np.ascontiguousarray(feature_colors, dtype=np.uint8)
    selected = np.ascontiguousarray(selected_flags, dtype=np.uint8)
    highlighted = np.ascontiguousarray(highlighted_flags, dtype=np.uint8)
    feature_count = int(colors.shape[0])
    point_count = int(points.shape[0])

    if (
        points.ndim != 2
        or points.shape[1] != 2
        or offsets.ndim != 1
        or offsets.shape[0] != feature_count + 1
        or bounds.shape != (feature_count, 4)
        or colors.shape != (feature_count, 3)
        or selected.shape != (feature_count,)
        or highlighted.shape != (feature_count,)
    ):
        return None

    min_x, min_y, max_x, max_y = view_bounds
    result = _LIB.ktk_render_dxf_overlay(
        _ptr(rgba),
        int(rgba.shape[0]),
        int(rgba.shape[1]),
        _double_ptr(points),
        c_int32(point_count),
        _int32_ptr(offsets),
        c_int32(feature_count),
        _double_ptr(bounds),
        _ptr(colors),
        _ptr(selected),
        _ptr(highlighted),
        float(min_x),
        float(min_y),
        float(max_x),
        float(max_y),
        float(meters_per_pixel),
    )
    return int(result) if result >= 0 else None


def paint_axis_aligned_tiff(
    rgba: np.ndarray,
    source_rgba: np.ndarray,
    source_rect: tuple[float, float, float, float],
    dest_rect: tuple[int, int, int, int],
    opacity: float,
) -> int | None:
    if _LIB is None:
        return None
    if rgba.ndim != 3 or rgba.shape[2] != 4 or rgba.dtype != np.uint8:
        return None
    if source_rgba.ndim != 3 or source_rgba.shape[2] != 4 or source_rgba.dtype != np.uint8:
        return None
    if not rgba.flags.c_contiguous or not rgba.flags.writeable:
        return None
    source = np.ascontiguousarray(source_rgba, dtype=np.uint8)
    left, top, right, bottom = source_rect
    dest_left, dest_top, dest_right, dest_bottom = dest_rect
    result = _LIB.ktk_paint_axis_aligned_tiff(
        _ptr(rgba),
        int(rgba.shape[0]),
        int(rgba.shape[1]),
        _ptr(source),
        int(source.shape[0]),
        int(source.shape[1]),
        float(left),
        float(top),
        float(right),
        float(bottom),
        int(dest_left),
        int(dest_top),
        int(dest_right),
        int(dest_bottom),
        float(opacity),
    )
    return int(result) if result >= 0 else None
