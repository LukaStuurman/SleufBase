"""Offline GEOS references for native repair, source-part union and clipping."""
import importlib
import json
import math
import os
import random
from pathlib import Path

import shapely
from shapely.geometry import Polygon, MultiPolygon, LineString, MultiLineString, GeometryCollection, Point, MultiPoint, box
from shapely.ops import unary_union
from shapely.errors import GEOSException

from generate_object_export_fixtures import reference
from generate_bgt_surface_fixtures import native

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('SLEUFBASE_BGT_GEOMETRY_FIXTURES', ROOT / 'rust/tests/fixtures/bgt-geometry/reference.json'))


def main():
    assert shapely.__version__ == '2.0.7'
    ref = reference()
    module = importlib.import_module(ref.__package__ + '.bgt_vector_tiles')
    # Fix completion order only in this isolated reference package. Production
    # still runs workers normally; equivalent GEOS ring starts must not make
    # committed audit JSON depend on thread scheduling.
    module.as_completed = lambda futures: iter(futures)
    cases = []

    def add(name, parts, bounds=(-2, -2, 12, 12)):
        result = module._merge_and_clip(parts, box(*bounds))
        assert result.is_valid, name
        cases.append(dict(name=name, parts=[native(g) for g in parts],
                          bounds=dict(zip(('min_x', 'min_y', 'max_x', 'max_y'), bounds)),
                          expected=None if result.is_empty else native(result),
                          area=result.area, length=result.length))
        x, y, xx, yy = bounds
        probes = [([(x-1, (y+yy)/2), (xx+1, (y+yy)/2)]),
                  ([((x+xx)/2, y-1), ((x+xx)/2, yy+1)]),
                  ([(x-1, y-1), (xx+1, yy+1)])]
        def edges(g):
            if g.is_empty:
                return
            if g.geom_type == 'Polygon':
                yield list(g.exterior.coords)[:2]
            elif g.geom_type == 'LineString':
                yield list(g.coords)[:2]
            elif hasattr(g, 'geoms'):
                for p in g.geoms:
                    yield from edges(p)
        probes.extend(list(edges(result))[:3])
        cases[-1]['probes'] = [dict(start=a, end=b, expected='Asfalt' if result.intersection(LineString([a, b])).length > 1e-8 else None) for a, b in probes]

    bowtie = Polygon([(0, 0), (4, 4), (4, 0), (0, 3), (0, 0)])
    try:
        unary_union([bowtie, box(8, 0, 10, 2)])
    except GEOSException:
        pass
    else:
        raise AssertionError('Pinned GEOS must independently reproduce the original union-before-repair failure')
    square = box(0, 0, 6, 6)
    invalid = [
        ('overlap-multi', MultiPolygon([box(0, 0, 4, 4), box(2, 0, 6, 4)])),
        ('nested-multi', MultiPolygon([square, box(2, 2, 4, 4)])),
        ('duplicate-hole', Polygon(square.exterior.coords, [square.exterior.coords])),
        ('touching-hole', Polygon(square.exterior.coords, [box(0, 1, 2, 3).exterior.coords])),
        ('outside-hole', Polygon(box(0, 0, 4, 4).exterior.coords, [box(8, 0, 10, 2).exterior.coords])),
        ('collapsed', Polygon([(0, 0), (2, 0), (4, 0), (0, 0)])),
        ('collapsed-point', Polygon([(2, 3), (2, 3), (2, 3)])),
        ('bowtie', bowtie),
        ('valid-hole', Polygon(square.exterior.coords, [box(2, 2, 4, 4).exterior.coords])),
    ]
    for name, g in invalid:
        for label, bounds in [('full', (-2, -2, 12, 12)), ('cut', (1, 1, 5, 5)), ('touch', (6, 0, 8, 6)), ('corner', (6, 6, 8, 8))]:
            add(name + '-' + label, [g], bounds)
    add('invalid-and-valid-source-parts', [bowtie, box(8, 0, 10, 2)])
    add('adjacent-tile-parts', [box(0, 0, 4, 4), box(4, 0, 8, 4)])
    add('duplicate-tile-parts', [square, square, square])
    add('deeply-nested-rings', [MultiPolygon([box(i, i, 130-i, 130-i) for i in range(64)])], (-1, -1, 131, 131))
    add('clip-edge-and-point', [MultiPolygon([box(0, 0, 2, 2), box(4, 4, 6, 6)])], (2, 0, 4, 4))
    add('lines-and-area', [GeometryCollection([square, LineString([(-1, 2), (8, 2)]), LineString([(3, -1), (3, 8)]), Point(10, 10)])])
    add('overlap-lines', [MultiLineString([[(0, 0), (4, 0)], [(2, 0), (8, 0)], [(4, -2), (4, 2)]])])
    add('duplicate-reverse-lines', [LineString([(0, 0), (2, 2), (4, 0)]), LineString([(4, 0), (2, 2), (0, 0)])])
    add('points-with-lines', [MultiPoint([(0, 0), (1, 1), (2, 1)]), LineString([(0, 0), (3, 3)])])
    for bounds in [(2, 2, 2, 5), (2, 2, 5, 2), (2, 2, 2, 2)]:
        for label, parts in [('area', [square]), ('line', [LineString([(0, 3), (6, 3)])]), ('along', [LineString([(2, 0), (2, 6)])]), ('point', [Point(2, 3)])]:
            add('degenerate-' + label + '-' + str(bounds), parts, bounds)
    rng = random.Random(314799)
    for i in range(300):
        ox, oy = (120000, 450000) if i % 2 else (0, 0)
        parts = []
        for j in range(rng.randrange(1, 5)):
            x = ox + rng.uniform(-4, 6); y = oy + rng.uniform(-4, 6)
            w = rng.uniform(.1, 8); h = rng.uniform(.1, 8)
            if (i + j) % 5 == 0:
                g = Polygon([(x, y), (x+w, y+h), (x+w, y), (x, y+h*.7), (x, y)])
            elif (i + j) % 5 == 1:
                g = Polygon(box(x, y, x+w, y+h).exterior.coords, [box(x+w*.2, y+h*.2, x+w*.8, y+h*.8).exterior.coords])
            elif (i + j) % 5 == 2:
                g = LineString([(x, y), (x+w, y+h), (x-w, y+h*.5)])
            else:
                g = box(x, y, x+w, y+h)
            parts.append(g)
        add('random-' + str(i), parts, (ox, oy, ox+8, oy+8))
    # Independently decoded recorded PDOK parts; no HTTP or app settings.
    tiles = json.loads((ROOT/'rust/tests/fixtures/bgt-tiles/reference.json').read_text(encoding='utf-8'))
    from verify_native_bgt_geometry import geometry
    real = tiles['cases'][0]
    from collections import defaultdict
    groups = defaultdict(list)
    for f in real['features']:
        groups[(f['layer_name'], f['feature_id'])].append(geometry(f['geometry']))
    for i, parts in enumerate(groups.values()):
        add('pdok-' + str(i), parts, (160000, 382800, 161200, 383400))
    exporter_module = importlib.import_module(ref.__package__ + '.cadastral_export')
    exporter = exporter_module.CadastralDxfExporter(wfs_client=None)
    grouped = []

    def group(name, tiles, bounds):
        client = module.BgtVectorTileClient()
        client._tiles_for_bounds = lambda _: [(0, i) for i in range(len(tiles))]
        client._tile_features = lambda _, i: [(f['layer_name'], f['feature_id'], geometry(f['geometry'])) for f in tiles[i]]
        client._tile_surface_features = lambda _, i: [module.BgtSurfaceFeature(f['layer_name'], f['feature_id'], f['physical_appearance'], geometry(f['geometry'])) for f in tiles[i] if f['physical_appearance'] and next(iter(f['geometry'])) in ('Polygon', 'MultiPolygon')]
        b = importlib.import_module(ref.__package__ + '.models').Bounds(*bounds)
        surfaces = client.fetch_surface_features(b)
        x, y, xx, yy = bounds
        probes = [((x, y+(yy-y)*rng.random()), (xx, y+(yy-y)*rng.random())) for _ in range(40)]
        grouped.append(dict(name=name, tiles=tiles, bounds=dict(zip(('min_x', 'min_y', 'max_x', 'max_y'), bounds)),
                            surfaces=[dict(layer_name=f.layer_name, feature_id=f.feature_id, physical_appearance=f.physical_appearance, geometry=native(f.geometry)) for f in surfaces],
                            paths=client.fetch_paths(b),
                            probes=[dict(start=a, end=b, expected=exporter._dominant_bgt_physical_appearance(LineString([a, b]), surfaces)) for a, b in probes]))

    def feature(g, id='same', layer='wegdeel', appearance='Asfalt'):
        return dict(layer_name=layer, feature_id=id, physical_appearance=appearance, geometry=native(g))
    group('repair-and-group', [[feature(bowtie), feature(box(0, 5, 4, 8), appearance='Gras'), feature(square, layer='pand', appearance='')], [feature(box(8, 0, 10, 2)), feature(box(4, 5, 9, 8), appearance='Gras'), feature(box(10, 0, 11, 3), id='other'), feature(LineString([(-1, 4), (11, 4)]), id='line', appearance='Asfalt')]], (-1, -1, 12, 10))
    real_parts = [dict(f, physical_appearance=next((s['physical_appearance'] for s in real['surfaces'] if (s['layer_name'],s['feature_id']) == (f['layer_name'],f['feature_id'])), '')) for f in real['features']]
    # Prove metadata association against every original surface part, rather
    # than assuming one appearance per identifier in an arbitrary snapshot.
    assert [f for f in real_parts if f['physical_appearance'] and next(iter(f['geometry'])) in ('Polygon','MultiPolygon')] == real['surfaces']
    group('pdok-full', [real_parts], (160600, 382700, 161000, 383100))
    group('pdok-cut', [real_parts, real_parts], (160760, 382860, 160850, 382950))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(cases, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
    client = module.BgtVectorTileClient()
    paths = []
    for v in [0., -0., .0005, .0015, 1.2345, 2.6755, -1.2345, 120000.0005, 450000.0015] + [rng.uniform(-500000, 500000) for _ in range(100)]:
        for n in [v, math.nextafter(v, -math.inf), math.nextafter(v, math.inf)]:
            p = [(n, 0.), (n+1, 1.)]
            paths.extend([p, list(reversed(p)), p])
    (OUT.parent/'dedupe.json').write_text(json.dumps(dict(paths=paths, expected=client._dedupe_paths(paths)), indent=2)+'\n', encoding='utf-8', newline='\n')
    (OUT.parent/'groups.json').write_text(json.dumps(grouped, ensure_ascii=False, indent=2)+'\n', encoding='utf-8', newline='\n')
    print(len(cases), 'GEOS repair/union/clip references')


if __name__ == '__main__':
    main()
