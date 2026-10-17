"""Freeze Python direction/midpoint outputs for native Rust regression tests.

Run from any directory with Python 3.10+: no GUI, account, or external packages.
Only the original geometry function definitions are executed, avoiding unrelated
application startup patches and their CAD/image/browser dependencies.
"""
from __future__ import annotations

import ast
import json
import math
from pathlib import Path
from statistics import median
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
KEY = "virtual_trench"


def functions(filename, names, namespace):
    tree = ast.parse((ROOT / filename).read_text(encoding="utf-8"))
    definitions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
    assert {node.name for node in definitions} == set(names)
    source = "from __future__ import annotations\n" + "\n".join(map(ast.unparse, definitions))
    exec(compile(source, str(ROOT / filename), "exec"), namespace)
    return SimpleNamespace(**namespace)


def reference():
    original = functions("marxact_import.py", ["_polyline_z_at_xy"], {"math": math})
    direction = functions("marxact_direction_patch.py", [
        "_principal_axis", "_cross", "_unique_xy", "_robust_principal_axis",
        "_axis_angle", "_axis_angle_difference", "_boundary_axis_near", "_automatic_object_axis",
        "_line_polygon_intersections", "_endpoint_parameters", "_axis_from_polygon",
        "_width_for_axis", "_centerline_from_axis", "trench_centerline", "_layer_boundary",
        "_layer_points", "_point_xyz", "_ensure_alignment_baseline", "alignment_rotation_degrees",
        "apply_virtual_layer_alignment_rotation", "recalculate_virtual_layer_automatic_alignment",
    ], {"math": math, "median": median, "_marxact": original,
        "_EPSILON": 1e-10, "_INTERSECTION_TOLERANCE": 1e-8,
        "_BOUNDARY_SNAP_TOLERANCE_DEGREES": 12.5, "_MIN_ROBUST_RESIDUAL_METERS": 0.03,
        "VIRTUAL_TRENCH_METADATA_KEY": KEY,
        "MARXACT_ALIGNMENT_ROTATION_KEY": "rotation", "MARXACT_ALIGNMENT_AUTO_START_KEY": "auto_start",
        "MARXACT_ALIGNMENT_AUTO_END_KEY": "auto_end", "MARXACT_ALIGNMENT_AUTO_WIDTH_KEY": "auto_width"})
    midpoint = functions("marxact_midpoint_snap_patch.py", [
        "_distance_point_to_segment", "_segment_midpoint", "_snap_intersection_to_nearest_segment_midpoint",
        "_snap_centerline_result", "_snap_loaded_automatic_alignment",
    ], {"math": math, "_direction": direction, "_SEGMENT_TOLERANCE_METERS": 1e-6,
        "VIRTUAL_TRENCH_METADATA_KEY": KEY})
    return direction, midpoint


def rotated(angle, values, center=(168000, 401000)):
    a = math.radians(angle)
    return [[center[0] + x * math.cos(a) - y * math.sin(a),
             center[1] + x * math.sin(a) + y * math.cos(a), z] for x, y, z in values]


def cases():
    rectangle = [[-2, -0.6, 9.6], [2, -0.6, 10.4], [2, 0.6, 10.4], [-2, 0.6, 9.6]]
    skew = rotated(13, [[t, 0, 9 + 0.1 * t] for t in [-1.3, -0.4, 0.5, 1.25]])
    polygon = rotated(10, rectangle)
    yield "skew_snapped_to_boundary", polygon, skew
    yield "sideways_offset_midpoints", polygon, rotated(10, [[t, 0.28, 9] for t in [-1.3, -0.4, 0.45, 1.25]])
    yield "no_objects_sloped_3d", [[0, 0, 10], [4, 0, 14], [4, 2, 18], [0, 2, 12]], []
    yield "vertical_objects", [[0, 0, 10], [8, 0, 18], [8, 3, 22], [0, 3, 14]], [[4, t, 3] for t in [0.5, 1.5, 2.5]]
    yield "duplicates_and_outlier", polygon, skew + [skew[0]] * 8 + rotated(10, [[0.1, 0.5, 8]])
    yield "single_object_fallback", polygon, [skew[0]]
    yield "irregular_missing_heights", [[0, 0, None], [6, 0, 12], [5, 2, None], [0, 3, 10]], [[1, 1, 3], [4, 1.4, 3]]
    yield "concave_bracket", [[x, y, 10 + x * 0.1] for x, y in [(0, 0), (8, 0), (8, 4), (6, 4), (6, 1), (2, 1), (2, 4), (0, 4)]], [[0.5, 2, 3], [1.5, 2, 3]]
    yield "collinear_edge_hits", [[0, 0, 10], [4, 0, 14], [4, 2, 18], [0, 2, 12]], [[1, 0, 3], [3, 0, 3]]
    # Corner hits exercise the edge-midpoint tie break.
    yield "vertex_hits", [[0, 0, 10], [4, 0, 14], [4, 2, 18], [0, 2, 12]], [[1, 0.5, 3], [3, 1.5, 3]]


def main():
    direction, midpoint = reference()
    fixtures = []
    for name, polygon, objects in cases():
        trench = SimpleNamespace(polygon=polygon, objects=[SimpleNamespace(x=p[0], y=p[1]) for p in objects])
        auto = midpoint._snap_centerline_result(polygon, direction.trench_centerline(trench))
        rows = [{"role": role, "x": p[0], "y": p[1], "z": p[2]} for role, p in [("start", auto[0]), ("end", auto[1])]]
        rows += [{"role": "object", "x": p[0], "y": p[1], "z": p[2]} for p in objects]
        layer = SimpleNamespace(metadata={KEY: {"boundary_3d": polygon, "points": rows, "width_meters": auto[2]}})
        assert direction.recalculate_virtual_layer_automatic_alignment(layer)
        assert midpoint._snap_loaded_automatic_alignment(layer)
        manual = []
        for degrees in [5, 25, 5, -15, 0, 95, -95]:
            assert direction.apply_virtual_layer_alignment_rotation(layer, degrees)
            manual.append({"degrees": degrees, "expected": [
                [rows[0][k] for k in ("x", "y", "z")],
                [rows[1][k] for k in ("x", "y", "z")], layer.metadata[KEY]["width_meters"]]})
        fixtures.append({"name": name, "polygon": polygon, "objects": objects, "automatic": auto, "manual": manual})
    output = ROOT / "rust/tests/fixtures/marxact_alignment.json"
    output.write_text(json.dumps(fixtures, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(f"Wrote {len(fixtures)} Python reference cases to {output}")


if __name__ == "__main__":
    main()
