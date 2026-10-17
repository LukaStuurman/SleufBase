"""Dense primary ezdxf spline oracles, using params()' valid knot domain.

No native geometry is called. Source control/fit data are retained separately
from references; periodic/unclamped samples use the documented B-spline domain.
"""
import json
import math
import os
from pathlib import Path
os.environ["EZDXF_DISABLE_C_EXT"] = "1"  # primary Python kernel also handles degree 11
import ezdxf
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/splines"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    doc = ezdxf.new("R2018")
    msp = doc.modelspace()
    entities = []

    def record(name, entity):
        entity.set_xdata("SOURCE_ID", [(1000, name)])
        entities.append(entity)
        return entity

    record("linear", msp.add_open_spline([(0, 0, 1), (2, 5, 2), (8, -3, 4)], degree=1))
    controls = [(0, 0, 2), (2, 8, 3), (5, -6, 5), (9, 7, 1), (13, -2, 4), (17, 1, 3)]
    record("cubic", msp.add_open_spline(controls))
    record("quarter-circle", msp.add_rational_spline([(3, 0, 2), (3, 3, 2), (0, 3, 2)], [1, math.sqrt(0.5), 1], degree=2))
    record("weighted", msp.add_rational_spline(controls, [0.1, 3, 0.2, 1, 5, 0.7]))
    uniform = msp.add_spline()
    uniform.set_uniform(controls)
    record("unclamped", uniform)
    periodic = msp.add_spline()
    periodic.set_closed(controls)
    record("periodic", periodic)
    record("degree-5", msp.add_open_spline(controls, degree=5))
    record("degree-11", msp.add_open_spline([(i, (-1)**i * 20, i % 3) for i in range(12)], degree=11))
    record("disconnected", msp.add_open_spline([(0, 0, 0), (1, 8, 1), (2, 0, 0), (20, 0, 2), (21, -8, 0), (22, 0, 2)], degree=2, knots=[0]*3 + [0.5]*3 + [1]*3))
    record("cusp", msp.add_open_spline(controls + [(20, 0, 4)], knots=[0]*4 + [0.5]*3 + [1]*4))
    no_knots = msp.add_open_spline(controls)
    no_knots.knots = []
    record("default-knots", no_knots)
    fits = [(0, 0, 1), (1, 5, 2), (5, -4, 4), (10, 8, 1), (20, 0, 3)]
    record("natural-fit", msp.add_spline(fits))
    constrained = msp.add_spline(fits)
    constrained.dxf.start_tangent = (10, 10, 0)
    constrained.dxf.end_tangent = (1, -1, 1)
    record("clamped-fit", constrained)
    record("fit-degree-ignored", msp.add_spline(fits[:3], degree=5))
    record("two-fit-points", msp.add_spline(fits[:2]))
    closed_fit = msp.add_spline(fits + [fits[0]])
    closed_fit.dxf.flags = 3
    closed_fit.dxf.start_tangent = closed_fit.dxf.end_tangent = (1, 0, 1)
    record("closed-fit", closed_fit)
    record("offset-knots", msp.add_open_spline(controls[:5], knots=[-3]*4 + [-1] + [1]*4))
    record("tiny-weights", msp.add_rational_spline(controls, [1e-300]*6))
    record("large-weights", msp.add_rational_spline(controls, [1e300]*6))
    record("loop", msp.add_open_spline([(0, 0, 0), (10, 20, 0), (-10, -20, 0), (0, 0, 0)]))
    for entity in entities:
        entity.dxf.extrusion = (0, 0, -1)  # SPLINE points remain WCS.
    block = doc.blocks.new("Curves", base_point=(1, 2, 3))
    for entity in entities:
        block.add_entity(entity.copy())
    msp.add_blockref("Curves", (168000, 401000, 5), dxfattribs={"xscale": -3, "yscale": 1.5, "zscale": 2, "rotation": 37, "extrusion": (0.3, 0.5, 0.9)})
    nested = doc.blocks.new("Nested")
    nested.add_blockref("Curves", (5, 3, 2), dxfattribs={"xscale": 2, "yscale": 0.4, "zscale": 1.5, "rotation": -23})
    msp.add_blockref("Nested", (168030, 401010, 8), dxfattribs={"xscale": 1, "yscale": -4, "rotation": 31})
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes = dict(sorted(doc.classes.classes.items()))
    doc.saveas(OUT / "splines-ascii.dxf", fmt="asc")
    doc.saveas(OUT / "splines-binary.dxf", fmt="bin")
    doc = ezdxf.readfile(OUT / "splines-ascii.dxf")
    expected = []
    sources = {}

    def source(entity):
        optional = lambda name: list(entity.dxf.get(name)) if entity.dxf.hasattr(name) else None
        return dict(degree=entity.dxf.degree, flags=entity.dxf.flags, controls=list(map(list, entity.control_points)),
                    weights=list(entity.weights), knots=list(entity.knots), fit_points=list(map(list, entity.fit_points)),
                    start_tangent=optional("start_tangent"), end_tangent=optional("end_tangent"), normal=optional("extrusion"),
                    knot_tolerance=entity.dxf.get("knot_tolerance", 1e-7), control_tolerance=entity.dxf.get("control_point_tolerance", 1e-7), fit_tolerance=entity.dxf.get("fit_tolerance", 1e-10))

    def paths(entity):
        curve = entity.construction_tool()
        knots = curve.knots()
        p, count = curve.degree, curve.count
        result = []
        current = []
        for a, b in zip(knots[p:count], knots[p+1:count+1]):
            if a == b:
                continue
            if a > knots[p] and knots.count(a) == p + 1:
                result.append(current)
                current = []
            stop = math.nextafter(b, a) if b < knots[count] and knots.count(b) == p + 1 else b
            segment = list(curve.points(np.linspace(a, stop, 257)))
            current.extend(segment[1:] if current else segment)
        result.append(current)
        return result

    def walk(entities, transform=lambda p: p, path=()):
        for entity in entities:
            if entity.dxftype() == "INSERT":
                block = entity.block()
                matrix = entity.matrix44()
                walk(block, lambda p, t=transform, m=matrix: t(m.transform(p)), path + (f"{entity.dxf.handle}:{block.name}[0,0]",))
            elif entity.dxftype() == "SPLINE":
                name = entity.get_xdata("SOURCE_ID")[0].value
                sources[name] = source(entity)
                expected.append(dict(id=name, path=" / ".join(path), paths=[[list(transform(p)) for p in part] for part in paths(entity)]))
    walk(doc.modelspace())
    (OUT / "expected.json").write_text(json.dumps(dict(sources=sources, draws=expected), separators=(",", ":"), allow_nan=False) + "\n", encoding="utf-8")
    print(f"Generated {len(sources)} source splines / {len(expected)} world curves / {sum(len(d['paths']) for d in expected)} paths")


if __name__ == "__main__":
    main()
