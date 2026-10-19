"""Compare independently generated GEOS and native geometry sets."""
import argparse
import json
from pathlib import Path
from shapely.geometry import shape, GeometryCollection, LineString
from shapely.ops import unary_union


def geometry(value):
    if value is None:
        return GeometryCollection()
    kind, coords = next(iter(value.items()))
    if kind == 'Collection':
        return GeometryCollection([geometry(g) for g in coords])
    return shape(dict(type=kind, coordinates=coords))


def verify(reference, results):
    assert len(reference) == len(results)
    failures = []
    for c, result in zip(reference, results):
        assert c['name'] == result['name']
        expected = geometry(c['expected']); actual = geometry(result['geometry'])
        tolerance = 2e-8  # i64 floating adapter can move RD coordinates by a few ULPs.
        reason = None
        if not actual.is_valid:
            reason = 'invalid native result'
        elif expected.is_empty != actual.is_empty:
            reason = 'empty geometry mismatch'
        elif not expected.is_empty:
            distance = expected.hausdorff_distance(actual)
            def polygons(g):
                if g.geom_type == 'Polygon':
                    return [g]
                return [p for part in getattr(g, 'geoms', []) for p in polygons(part)]
            different_area = unary_union(polygons(expected)).symmetric_difference(unary_union(polygons(actual))).area
            if distance > tolerance or different_area > tolerance * max(1, expected.length) or abs(expected.length-actual.length) > tolerance * max(1, expected.length):
                reason = f'distance={distance}, area={expected.area}/{actual.area}, length={expected.length}/{actual.length}'
        if reason:
            failures.append((c['name'], reason, expected.wkt[:500], actual.wkt[:500]))
    if failures:
        for f in failures[:12]:
            print(f)
        raise AssertionError(f'{len(failures)} native geometry mismatches')
    print(len(reference), 'independent GEOS geometry comparisons passed')


def main():
    p = argparse.ArgumentParser(); p.add_argument('reference', type=Path); p.add_argument('results', type=Path)
    p.add_argument('--groups', action='store_true')
    args = p.parse_args()
    references=json.loads(args.reference.read_text(encoding='utf-8'));results=json.loads(args.results.read_text(encoding='utf-8'))
    if args.groups:
        assert len(references)==len(results)
        for ref, actual in zip(references,results):
            assert ref['name']==actual['name']
            keys=lambda f:(f['layer_name'],f['feature_id'],f['physical_appearance'])
            assert list(map(keys,ref['surfaces']))==list(map(keys,actual['surfaces']))
            verify([dict(name=str(keys(f)),expected=f['geometry']) for f in ref['surfaces']], [dict(name=str(keys(f)),geometry=f['geometry']) for f in actual['surfaces']])
            original_paths=unary_union([LineString(p) for p in ref['paths']]);native_paths=unary_union([LineString(p) for p in actual['paths']])
            assert original_paths.hausdorff_distance(native_paths)<=2e-8,ref['name']
            assert abs(original_paths.length-native_paths.length)<=2e-8*max(1,original_paths.length),ref['name']
            assert actual['votes']==[p['expected'] for p in ref['probes']],ref['name']
            print(ref['name'],len(actual['surfaces']),'normalized surfaces; paths and all appearance votes agree')
    else:
        verify(references,results)


if __name__ == '__main__':
    main()
