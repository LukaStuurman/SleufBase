"""Fresh original point-mode data against optimized native code; no app/account."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,subprocess,sys
import generate_raster_point_fixtures as oracle
import generate_profile_context_fixtures as context_oracle
def same(actual,expected):
    if isinstance(expected,dict):
        assert actual.keys()==expected.keys(),(actual.keys(),expected.keys())
        for k in expected:same(actual[k],expected[k])
    elif isinstance(expected,(list,tuple)):
        assert len(actual)==len(expected),(len(actual),len(expected))
        for a,b in zip(actual,expected):same(a,b)
    elif isinstance(expected,(int,float)) and not isinstance(expected,bool):assert abs(actual-expected)<2e-8,(actual,expected)
    else:assert actual==expected,(actual,expected)
def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-raster-points-runtime-') as temp:
        folder=Path(temp);oracle.OUT=folder/'original';oracle.SEED=671822;oracle.COUNT=129;cases=oracle.main()['cases']
        context_oracle.OUT=folder/'context';context_oracle.SEED=691129;context_oracle.COUNT=31;contexts=context_oracle.main()['cases']
        requests=cases+[dict(context_pages=c['kind'],bounds=c['bounds'],pages=c['pages']) for c in contexts]
        source=folder/'input.json';target=folder/'output.json';source.write_text(json.dumps(requests,ensure_ascii=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'))
        expected=[{k:c[k] for k in ['automatic','rows','after_reference','after_start','reference_display']}|{'picks':[p['expected'] for p in c['picks']]} for c in cases]
        expected += [dict(paths=c['paths'],requests=len(c['calls'])) for c in contexts]
        same(actual,expected)
        print(f'{len(cases)} fresh original metadata/rotation/automatic/candidate contracts, {len(cases)*4} hit selections and {len(contexts)} complete original context batches match optimized native code within 0.00000002 m')
if __name__=='__main__':main()
