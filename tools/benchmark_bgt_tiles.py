"""Frozen real tile, same output counts; parsing/world preparation only."""
from pathlib import Path
import importlib,json,platform,statistics,subprocess,sys,time
import mapbox_vector_tile
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]

def main():
    exe=Path(sys.argv[1]).resolve();output=Path(sys.argv[2]);source=(ROOT/'rust/tests/fixtures/bgt-tiles/pdok-12-2420-2075.pbf').read_bytes()
    ref=reference();m=importlib.import_module(ref.__package__+'.bgt_vector_tiles');client=m.BgtVectorTileClient();rounds=[]
    for _ in range(3):
        native=json.loads(subprocess.check_output([str(exe)],text=True,encoding='utf-8'))
        features=0;surfaces=0;start=time.perf_counter()
        for _ in range(native['loops']):
            decoded=mapbox_vector_tile.decode(source);client._decoded_tile=lambda *args:decoded
            paths=client._tile_features(2420,2075);parts=client._tile_surface_features(2420,2075)
            features+=len(paths);surfaces+=len(parts);del paths,parts,decoded
        cold_ms=(time.perf_counter()-start)*1000
        assert features==native['features'] and surfaces==native['surfaces']
        assert native['transport_calls']==1 and native['cache_requests']==20000
        rounds.append(dict(native=native,original=dict(cold_ms=cold_ms,features=features,surfaces=surfaces)))
    result=dict(rounds=rounds,python=sys.version.split()[0],platform=platform.platform(),scope='Fixed 226591-byte PDOK tile, twenty cold decodes + per-tile RD feature/surface preparation. Excludes process startup, file/network I/O, source-feature union/repair/clipping, profile voting, GUI/DXF and process peak memory. Native cache requests measured separately; cached_memory_estimate is a conservative data-size estimate, not a process-peak measurement.')
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('Median twenty cold tiles: native',round(statistics.median(r['native']['cold_ms'] for r in rounds),3),'ms; original',round(statistics.median(r['original']['cold_ms'] for r in rounds),3),'ms; same feature/surface counts')

if __name__=='__main__':main()
