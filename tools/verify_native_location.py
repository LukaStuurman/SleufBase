"""Fresh original location/pyproj/PDOK parsing against the optimized Rust driver."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,subprocess,sys
import generate_location_fixtures as oracle

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-location-runtime-') as temp:
        root=Path(temp);oracle.OUT=root/'original';oracle.SEED=92117;oracle.COUNT=140
        expected=json.loads(json.dumps(oracle.main()));requests=[];wanted=[]
        for case in expected['coordinates']:
            requests.append(dict(kind='coordinate',query=case['query']));wanted.append(('coordinate',case))
        for case in expected['searches']:
            requests.append(dict(kind='search',query=case['query'],rows=case['rows'],payload=case['payload']));wanted.append(('search',case['output']))
        for case in expected['reverses']:
            requests.append(dict(kind='reverse',x=155000.1254,y=463000.7506,rows=99,payload=case['payload']));wanted.append(('reverse',case['output']))
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(requests,ensure_ascii=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(wanted)
        worst=0.
        for n,(found,(kind,want)) in enumerate(zip(actual,wanted)):
            if kind!='coordinate':assert found==want,(n,kind,found,want);continue
            assert found['rd']==want['rd'],(n,found,want)
            if want['coordinate'] is None:assert found['coordinate'] is None,n;continue
            a,b=found['coordinate'],want['coordinate']
            assert a is not None and a['label']==b['label'] and a['location_type']==b['location_type'],(n,a,b)
            error=((a['x']-b['x'])**2+(a['y']-b['y'])**2)**.5;worst=max(worst,error)
            assert error<.002,(n,error,a,b)
            assert found['scale']==expected['zooms'][b['location_type']],n
        print(f'{len(expected["coordinates"])} fresh coordinate queries / 66 Unicode decimal alphabets / 38 original service responses match optimized native parser; maximum pyproj horizontal difference {worst:.9f} m, below 0.002 m navigation budget')

if __name__=='__main__':main()
