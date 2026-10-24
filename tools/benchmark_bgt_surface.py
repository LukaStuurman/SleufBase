"""Same geometry and output checksum; core time excludes parsing/startup/UI."""
from pathlib import Path
from types import SimpleNamespace
import importlib
import json
import platform
import statistics
import subprocess
import sys
import time
import shapely
from shapely.geometry import shape,GeometryCollection,LineString
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]

def geometry(item):
    kind,coordinates=next(iter(item.items()))
    return GeometryCollection([geometry(g) for g in coordinates]) if kind=='Collection' else shape(dict(type=kind,coordinates=coordinates))

def main():
    exe=Path(sys.argv[1]).resolve();output=Path(sys.argv[2])
    fixture=json.loads((ROOT/'rust/tests/fixtures/bgt-surface/reference.json').read_text(encoding='utf-8'))
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export');exporter=module.CadastralDxfExporter(wfs_client=None)
    cases=[]
    for c in fixture:
        features=[module.BgtSurfaceFeature('wegdeel',str(i),f['physical_appearance'],geometry(f['geometry'])) for i,f in enumerate(c['features'])]
        profile=SimpleNamespace(start_point=SimpleNamespace(x=c['start'][0],y=c['start'][1]),end_point=SimpleNamespace(x=c['end'][0],y=c['end'][1]),axis_length=c['axis_length'])
        cases.append((profile,features,LineString([c['start'],c['end']])))
    def run_python():
        h=0xcbf29ce484222325
        def add(text):
            nonlocal h
            for b in (text or '').encode('utf-8')+b'\xff':h=((h^b)*0x100000001b3)&0xffffffffffffffff
        start=time.perf_counter()
        for _ in range(10):
            for p,features,line in cases:
                add(exporter._dominant_bgt_physical_appearance(line,features))
                texts=exporter._template_bgt_surface_texts(p,features)
                for key in ['start','middle','end']:add(texts.get(key))
        return dict(queries_ms=(time.perf_counter()-start)*1000,checksum=f'{h:016x}')
    rounds=[]
    for _ in range(3):
        native=json.loads(subprocess.check_output([str(exe)],text=True,encoding='utf-8'));original=run_python()
        assert native['checksum']==original['checksum'],(native,original)
        rounds.append(dict(native=native,original=original))
    result=dict(cases=len(cases),loops=10,rounds=rounds,python=sys.version.split()[0],shapely=shapely.__version__,platform=platform.platform(),scope='Prepared valid geometries, appearance voting and three profile probes; excludes JSON/geometry decoding, process startup, network, UI, DXF writing and process peak memory. Native preparation measured separately.')
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('Median queries: native',round(statistics.median(r['native']['queries_ms'] for r in rounds),3),'ms; original',round(statistics.median(r['original']['queries_ms'] for r in rounds),3),'ms; identical checksum')

if __name__=='__main__':main()
