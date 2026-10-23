"""Fresh original transformation and pointer chains against optimized native code."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,subprocess,sys
import generate_raster_rotation_fixtures as oracle
def same(actual,expected,tolerance=2e-8):
    if isinstance(expected,dict):
        assert actual.keys()==expected.keys(),(actual.keys(),expected.keys())
        for k in expected:same(actual[k],expected[k],2e-6 if k=='overlay' else tolerance)
    elif isinstance(expected,(list,tuple)):
        assert len(actual)==len(expected),(len(actual),len(expected))
        for a,b in zip(actual,expected):same(a,b,tolerance)
    elif isinstance(expected,(int,float)) and not isinstance(expected,bool):assert abs(actual-expected)<tolerance,(actual,expected)
    else:assert actual==expected,(actual,expected)
def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-geotiff-rotation-runtime-') as temp:
        folder=Path(temp);oracle.OUT=folder/'original';oracle.SEED=78231;oracle.COUNT=53;cases=oracle.main()['cases'];source=folder/'input.json';target=folder/'output.json';source.write_text(json.dumps(cases,ensure_ascii=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));expected=[dict(steps=[{k:s[k] for k in ['expected','overlay','hits','screen_angles']} for s in c['steps']],drag=[m['expected'] for m in c['drag']['moves']]) for c in cases];same(actual,expected)
        print(f'{len(cases)} fresh original rotation chains / {len(cases)*7} affine transforms / {len(cases)*49} ring decisions and screen angles / {len(cases)*4} actual original pointer handlers match optimized native code: 0.00000002 m/degrees and 0.000002 screen pixels')
if __name__=='__main__':main()
