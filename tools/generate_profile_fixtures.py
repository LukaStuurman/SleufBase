"""Freeze the original, V9-patched profile builder, without UI/network/settings.

Python 3.11, ezdxf 1.4.4, Pillow 11.3, numpy 2.2.6, shapely 2.0.7,
requests 2.32.4, pyproj 3.7.1 and mapbox-vector-tile 2.2.0. No Rust code is invoked.
"""
from pathlib import Path
import importlib
import json
import random
import unicodedata
from generate_object_export_fixtures import reference

ROOT = Path(__file__).resolve().parents[1]


def main():
    assert unicodedata.unidata_version == "14.0.0"
    ref = reference()
    module = importlib.import_module(ref.__package__ + ".cadastral_export")
    patches = importlib.import_module(ref.__package__ + ".dxf_template_pipeline_v9_patch")
    patches.install_dxf_template_pipeline_v9_patch()
    exporter = module.CadastralDxfExporter(wfs_client=None)
    rules = ref.build_object_layer_rules()

    def point(x, y=0, z=10, source="punt", diameter="", extra=""):
        return ref.KickTheMapObjectPoint("object", source, x, y, z, "", diameter, extra)

    def line(vertices, source="water", extra=""):
        return ref.KickTheMapObjectPolyline("lijn", source, tuple(ref.KickTheMapPolylineVertex(*v) for v in vertices), "", "", extra)

    def vertex(p):
        return dict(x=p.x, y=p.y, z=p.z)

    def feature(f):
        geometry = {"Point": vertex(f)} if isinstance(f, ref.KickTheMapObjectPoint) else {"Polyline": [vertex(v) for v in f.vertices]}
        return dict(object_name=f.object_name, source_name=f.source_name, geometry=geometry, attributes=[f.attribute_1, f.attribute_2, f.attribute_3])

    cases = []

    def add(name, features, roads=(), terrain=(), forced=None, scale=0.1, case_rules=None):
        points = tuple(f for f in features if isinstance(f, ref.KickTheMapObjectPoint))
        lines = tuple(f for f in features if isinstance(f, ref.KickTheMapObjectPolyline))
        dataset = ref.KickTheMapObjectDataset(1, name, Path("fixture.json"), points, lines, forced)
        for reverse in [False, True]:
            effective_rules = rules if case_rules is None else ref.build_object_layer_rules(case_rules)
            result = exporter._build_template_cross_section_profile(dataset, effective_rules, list(roads), list(terrain), scale, reverse)
            expected = None
            projected = []
            surface = []
            visible_range = None
            if result is not None:
                entries = []
                for p in result.points:
                    # Synthetic endpoints retain the chosen endpoint's metadata.
                    if p.is_endpoint:
                        candidates = [i for i, f in enumerate(features) if isinstance(f, ref.KickTheMapObjectPoint) and (f.source_name, f.z, f.attribute_2, f.attribute_3) == (p.point.source_name, p.point.z, p.point.attribute_2, p.point.attribute_3)]
                        # Distinct names in the general fixtures make metadata identity unique.
                        candidates = [i for i in candidates if features[i].object_name == p.point.object_name]
                        index = candidates[0]
                    else:
                        index = next(i for i, f in enumerate(features) if f is p.point)
                    pos = exporter._cross_section_feature_position(p.point)
                    entries.append(dict(feature_index=index, position=dict(x=pos[0], y=pos[1], z=pos[2]), chainage=p.chainage, point_z=p.point_z, layer_name=p.layer_name, color=p.color, description=p.description, is_endpoint=p.is_endpoint))
                expected = dict(start=vertex(result.start_point), end=vertex(result.end_point), axis_dx=result.axis_dx, axis_dy=result.axis_dy, axis_length=result.axis_length, reference_level=result.reference_level, points=entries)
                for f in lines:
                    projected.append(exporter._cross_section_polyline_profile_vertices(f, result, 3.0, 4.0, -2.0, 1.2, 0.8))
                surface = [exporter._template_profile_surface_z(result, c) for c in [-1.0, 0.0, result.axis_length * 0.37, result.axis_length, result.axis_length + 1.0]]
                visible_range = exporter._cross_section_visible_chainage_range(result, scale)
            cases.append(dict(name=name + ("-reverse" if reverse else "-normal"), features=[feature(f) for f in features], roads=roads, terrain=terrain, forced_start=forced, marker_scale=scale, reverse=reverse, expected=expected, projected=projected, surface=surface, visible_range=visible_range, rules=None if case_rules is None else [dict(keywords=list(r.keywords), target_layer=r.target_layer, color_aci=r.color, label=r.profile_label) for r in effective_rules]))

    from dataclasses import replace

    def named(features):
        return [replace(f, object_name=f"object-{i}") for i, f in enumerate(features)]

    base = named([point(0), point(10), point(3, z=9, source="water", diameter="110"), point(7, z=8, source="ls", diameter="Ø 160,5", extra="dubbel")])
    add("basic", base)
    add("road-right", base, roads=[[(12, -20), (12, 20)]])
    add("terrain-weight", base, roads=[[(-3, -20), (-3, 20)]], terrain=[[(14, -20), (14, 20)]])
    add("road-projection-outside-segment", base, roads=[[(12, 20), (12, 30)]])
    add("degenerate-road", base, roads=[[(12, 0), (12, 0)]])
    for forced in [(0, 0), (10.5, 0), (10.500001, 0), (3, 0)]:
        add(f"forced-{forced[0]}", base, roads=[[(12, -20), (12, 20)]], forced=forced)
    add("no-points", [])
    add("one-height", named([point(0, z=None), point(10)]))
    add("coincident", named([point(0), point(0, z=9)]))
    add("tiny-axis", named([point(0), point(1e-7)]))
    add("all-matched", named([point(0, source="ls"), point(10, source="water"), point(3, z=-2.01, source="hs")]))
    add("named-unmatched", named([point(0, source="a"), point(10, source="b"), point(30, source="water")]))
    add("numeric-markers", named([point(0, source="²"), point(10, source="٣"), point(-5, source="naam")]))
    add("polyline-heights", named([*base, line([(-2, 0, 7), (0, 0, None), (3, 4, 9), (15, 4, 11)]), line([(0, 1, 10), (0, 1, 12), (0, 1, 14)], source="stoeprand", extra="'dekband'")]))
    add("polyline-missing-heights", named([*base, line([(0, 0, None), (4, 0, 8), (9, 0, None)]), line([(0, 0, None), (4, 0, None)])]))
    add("endpoint-tie", named([point(0), point(10), point(-10), point(3, source="water")]), forced=(0, 0))
    add("midpoint-at-endpoint", named([point(0), point(10), line([(-1, 2, 8), (1, -2, 9)])]))
    for layer in ["0", "WARM", "GLASVEZEL", "DATA", "WATER", "ET_LS", "ET_MS", "GAS_LD", "GAS_HD", "MONITORING", "VRIJVERVAL", "DRUK", "GEVAARLIJKE", "custom"]:
        add("custom-layer-" + layer, named([point(0), point(10), point(3, source="special", diameter="75")]), case_rules=[("special", layer, 256, "")])
    add("custom-label-unicode", named([point(0), point(10), point(3, source="special"), point(3, source="special")]), case_rules=[("special", "test", 3, "\u001cMijn leiding \u00d8")])
    add("unmatched-dekband", named([point(0), point(10), line([(2, 0, 9), (4, 0, 9)], source="Stoeprand", extra="'dekband'")]))
    for diameter in ["-20", "+١١٠,٥", "Ø .5", "0", "1e3", "12.3.4", "abc", "５００", "²"]:
        add("diameter-" + diameter, named([point(0), point(10), point(10.2, source="water", diameter=diameter, extra=";DUBBEL;")]))
    for extra in ["dubbel", "dubbelwand", "a dubbel b", "dubbel\u001c", "dubbel_", "'dubbel'", "dubbel, mantelbuis"]:
        add("bundle-" + extra, named([point(0), point(10), point(10.2, source="water", diameter="110", extra=extra)]))
    rng = random.Random(48291)
    for i in range(60):
        origin = (rng.uniform(100000, 200000), rng.uniform(400000, 500000))
        features = [point(*origin, z=rng.uniform(-5, 20)), point(origin[0] + rng.uniform(-15, 15), origin[1] + rng.uniform(-15, 15), rng.uniform(-5, 20))]
        for j in range(12):
            features.append(point(origin[0] + rng.uniform(-10, 10), origin[1] + rng.uniform(-10, 10), None if j == 0 else rng.uniform(-5, 20), rng.choice(["water", "ls", "warm", "onbekend", "PUNT"]), rng.choice(["110", "160,5", "", "-5"]), rng.choice(["", "dubbel", "mantelbuis"])))
        features.append(line([(origin[0] - 2, origin[1], 0), (origin[0] + 8, origin[1] + 3, 2)]))
        roads = [[(origin[0] + 3, origin[1] - 50), (origin[0] + 3, origin[1] + 50)]]
        add(f"random-{i:02}", named(features), roads=roads, forced=origin if i % 3 == 0 else None)

    data = dict(reference="Python 3.11 Unicode 14; original CadastralDxfExporter with pipeline V9", rules=[dict(keywords=list(r.keywords), target_layer=r.target_layer, color_aci=r.color, label=r.profile_label) for r in rules], cases=cases)
    out = ROOT / "rust/tests/fixtures/profiles"
    out.mkdir(parents=True, exist_ok=True)
    (out / "reference.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    ranges = []
    for n in range(0x110000):
        if chr(n).isdigit():
            if ranges and ranges[-1][1] == n - 1:
                ranges[-1][1] = n
            else:
                ranges.append([n, n])
    (ROOT / "rust/src/profile_unicode.rs").write_text("// Python 3.11 Unicode 14 str.isdigit(); generated by generate_profile_fixtures.py.\nconst DIGIT_RANGES: &[(u32,u32)] = &[\n" + "".join(f"({a},{b}),\n" for a, b in ranges) + "];\n", encoding="utf-8")
    print(f"Frozen {len(cases)} independent original profile results")


if __name__ == "__main__":
    main()
