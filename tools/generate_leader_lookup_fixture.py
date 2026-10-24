"""Run only the original MarXact bucket lookup, without importing the application."""
import ast
import json
import math
from pathlib import Path
import random
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]


def main():
    tree = ast.parse((ROOT / "marxact_import.py").read_text(encoding="utf-8"))
    lookup = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "_leader_lookup")
    future = ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0)
    namespace = {"LEADER_MATCH_TOLERANCE": 0.08}
    exec(compile(ast.fix_missing_locations(ast.Module(body=[future, lookup], type_ignores=[])), "marxact_import.py", "exec"), namespace)
    cases = []

    def case(points, queries):
        leaders = [dict(x=x, y=y, name=str(i), height=i / 10) for i, (x, y) in enumerate(points)]
        nearest = namespace["_leader_lookup"]([SimpleNamespace(**p) for p in leaders])
        original = []
        expected = []
        ordered = sorted(enumerate(leaders), key=lambda item: (round(item[1]["x"] * 12.5), round(item[1]["y"] * 12.5), item[0]))
        for x, y in queries:
            found = nearest(x, y)
            original.append(None if found is None else int(found.name))
            # Independent exhaustive reference: retain Python's tie visitation
            # order, but examine every label within the actual 8 cm tolerance.
            best = None
            distance = 0.08**2
            for index, candidate in ordered:
                dx, dy = candidate["x"] - x, candidate["y"] - y
                if abs(dx) > 0.08 or abs(dy) > 0.08:
                    continue
                d = dx**2 + dy**2
                if d <= distance:
                    best, distance = index, d
            expected.append(best)
        cases.append(dict(leaders=leaders, queries=queries, original=original, expected=expected))

    # Cross-bucket ties follow bucket visitation, not global source order.
    case([(0.08, 0), (-0.08, 0)], [(0, 0)])
    case([(0.01, 0), (-0.01, 0), (0.01, -0.0)], [(0, 0), (0.01, 0)])
    case([(0, 0)], [(0.08, 0), (math.nextafter(0.08, 0), 0), (math.nextafter(0.08, math.inf), 0)])
    # round() uses ties to even; original Python misses these two-cell edges.
    for sign in (-1, 1):
        case([(sign * 0.12, 0)], [(sign * 0.04, 0), (math.nextafter(sign * 0.04, -math.inf), 0), (math.nextafter(sign * 0.04, math.inf), 0)])
    case([(1e300, -1e300), (1e30, 0), (0, -0.0), (-0.0, 0)], [(1e300, -1e300), (2e300, -1e300), (1e30, 0), (0, 0)])
    threshold = 2**53 / 12.5
    case([(threshold, 0), (math.nextafter(threshold, -math.inf), 0)], [(threshold, 0), (math.nextafter(threshold, -math.inf), 0)])
    rng = random.Random(8234)
    for number in range(200):
        base_x, base_y = ((168000, 401000) if number % 2 else (0, 0))
        points = [(base_x + rng.randint(-20, 20) * 0.04, base_y + rng.randint(-20, 20) * 0.04) for _ in range(32)]
        points.extend(points[:4])
        queries = [(base_x + rng.randint(-20, 20) * 0.04, base_y + rng.randint(-20, 20) * 0.04) for _ in range(30)]
        queries.extend((x + rng.uniform(-0.08, 0.08), y + rng.uniform(-0.08, 0.08)) for x, y in points[:20])
        case(points, queries)
    path = ROOT / "rust/tests/fixtures/leader_lookup.json"
    path.write_text(json.dumps(cases, separators=(",", ":"), allow_nan=False) + "\n", encoding="utf-8")
    corrections = sum(a != b for c in cases for a, b in zip(c["original"], c["expected"]))
    print(f"Generated {len(cases)} cases / {sum(len(c['queries']) for c in cases)} Python queries / {corrections} rounding corrections")


if __name__ == "__main__":
    main()
