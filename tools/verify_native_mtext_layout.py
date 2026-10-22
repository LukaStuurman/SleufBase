"""Fresh full original layout vs the compiled native measured-cell engine."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json
import math
import subprocess
import sys

from generate_mtext_layout_fixtures import requests,evaluate


def compare(a,b,path=''):
    if isinstance(b,dict):
        assert a.keys()==b.keys(),(path,a.keys(),b.keys())
        for k,v in b.items():compare(a[k],v,path+'.'+k)
    elif isinstance(b,(list,tuple)):
        assert len(a)==len(b),(path,len(a),len(b))
        for i,(x,y) in enumerate(zip(a,b)):compare(x,y,f'{path}[{i}]')
    elif isinstance(b,(int,float)) and not isinstance(b,bool):
        assert math.isclose(a,b,rel_tol=64*sys.float_info.epsilon,abs_tol=1e-9),(path,a,b)
    else:assert a==b,(path,a,b)


def main():
    driver=str(Path(sys.argv[1]).resolve());cases=list(requests(seed=973581,random_count=256))
    for n,r in enumerate(cases):
        r['at']=[168000.125+n*.013,401000.375-n*.017]
        r['width']+=n%5*.37
        for e in r['events']:
            if e['kind']=='paragraph':e['spacing']=e.get('spacing',1)*(1+n%4*.07)
    expected=[evaluate(r) for r in cases]
    with TemporaryDirectory(prefix='SleufBase-mtext-layout-runtime-') as temp:
        root=Path(temp);source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(cases,ensure_ascii=False,allow_nan=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        primitives=0;errors=0;columns=0
        for n,(found,want) in enumerate(zip(actual,expected)):
            if 'output' not in want:assert 'error' in found,(n,found);errors+=1;continue
            assert 'output' in found,(n,found)
            compare(found['output'],want['output'],str(n));primitives+=len(found['output']['primitives']);columns+=len(found['output']['columns'])
        before=target.read_bytes();failure=subprocess.run([driver,str(source),str(target)],capture_output=True,timeout=120)
        assert failure.returncode!=0 and target.read_bytes()==before,'Existing layout audit output was replaced'
        print(f'{len(cases)} fresh layouts / {columns} columns / {primitives} primitives / {errors} original errors match all original boxes, text/fraction/stroke order and placement; atomic output refuses overwrite')


if __name__=='__main__':main()
