"""Independent mapbox/Shapely comparison of native tile output and live bytes."""
from pathlib import Path
import sys,json,importlib
import mapbox_vector_tile
from generate_object_export_fixtures import reference
from generate_bgt_surface_fixtures import native
from verify_native_template_images import compare
ROOT=Path(__file__).resolve().parents[1]

def check(actual,features,surfaces,name):
    # Fresh Shapely mappings contain tuples; frozen JSON contains arrays.
    features=json.loads(json.dumps(features));surfaces=json.loads(json.dumps(surfaces))
    basic=[{k:v for k,v in f.items() if k!='physical_appearance'} for f in actual]
    surface=[f for f in actual if f['physical_appearance'] and any(k in f['geometry'] for k in ['Polygon','MultiPolygon'])]
    compare(basic,features,name+'.features');compare(surface,surfaces,name+'.surfaces')
    return len(basic),len(surface)

def main():
    output=Path(sys.argv[1]);count=0;parts=0
    if '--live' in sys.argv:
        metadata=json.loads((output/'live-metadata.json').read_text(encoding='utf-8'))
        assert metadata['cache_reused'] and 1<=metadata['http_calls']<=4
        decoded=mapbox_vector_tile.decode((output/'live.pbf').read_bytes())
        ref=reference();m=importlib.import_module(ref.__package__+'.bgt_vector_tiles');c=m.BgtVectorTileClient();c._decoded_tile=lambda *args:decoded
        row=metadata['id']['row'];col=metadata['id']['col']
        features=[dict(layer_name=n,feature_id=i,geometry=native(g)) for n,i,g in c._tile_features(row,col)]
        surfaces=[dict(layer_name=f.layer_name,feature_id=f.feature_id,physical_appearance=f.physical_appearance,geometry=native(f.geometry)) for f in c._tile_surface_features(row,col)]
        count,parts=check(json.loads((output/'live.json').read_text(encoding='utf-8')),features,surfaces,'live')
        print(f'Native live HTTPS and cache: {count} RD features/{parts} surface parts independently verified from recorded response bytes')
    else:
        cases=json.loads((ROOT/'rust/tests/fixtures/bgt-tiles/reference.json').read_text(encoding='utf-8'))['cases']
        for case in cases:
            n,p=check(json.loads((output/(case['name']+'.json')).read_text(encoding='utf-8')),case['features'],case['surfaces'],case['name']);count+=n;parts+=p
        print(f'{len(cases)} original tile cases: {count} RD features/{parts} surface parts independently verified')

if __name__=='__main__':main()
