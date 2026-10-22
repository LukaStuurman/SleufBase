"""Fresh original parser vs the compiled native core, not saved expectations."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json
import math
import subprocess
import sys

from generate_mtext_fixtures import requests,evaluate


def compare(a,b,path=''):
    if isinstance(b,dict):
        assert a.keys()==b.keys(),(path,a.keys(),b.keys())
        for k,v in b.items():compare(a[k],v,path+'.'+k)
    elif isinstance(b,(list,tuple)):
        assert len(a)==len(b),(path,len(a),len(b))
        for i,(x,y) in enumerate(zip(a,b)):compare(x,y,f'{path}[{i}]')
    elif isinstance(b,(int,float)) and not isinstance(b,bool):
        assert math.isclose(a,b,rel_tol=8*sys.float_info.epsilon,abs_tol=1e-12),(path,a,b)
    else:assert a==b,(path,a,b)


def main():
    driver=str(Path(sys.argv[1]).resolve())
    cases=list(requests(seed=94159,random_count=192))
    for n,r in enumerate(cases):
        if r['initial']:
            r['initial']['cap_height']*=1.+n%7*.13
            r['initial']['font_face']['family']=f'Fresh Font {n%17}'
            r['initial']['rgb']=[(n+23)%256,(n+37)%256,(n+61)%256]
            r['initial']['paragraph']['tab_stops']=[1.3*(1+n%5*.07),'c2.5','r3']
    expected=[evaluate(r) for r in cases]
    with TemporaryDirectory(prefix='SleufBase-mtext-runtime-') as temp:
        root=Path(temp);source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(cases,ensure_ascii=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        tokens=0;errors=0
        for n,(found,want) in enumerate(zip(actual,expected)):
            if 'tokens' not in want:
                assert 'error' in found and found['error']['kind'] in ['invalid','limit'],(n,found);errors+=1;continue
            assert 'tokens' in found,(n,found)
            expanded=[dict(content=t['content'],context=found['contexts'][t['context']]) for t in found['tokens']]
            compare(expanded,want['tokens'],str(n));tokens+=len(expanded)
        before=target.read_bytes()
        retry=subprocess.run([driver,str(source),str(target)],capture_output=True,timeout=120)
        assert retry.returncode!=0 and target.read_bytes()==before,'Atomic driver must refuse overwrite'
        # Shared font names stay small in core memory but can still make a
        # serialized context dictionary huge. Test the actual output guard
        # and atomic cleanup, without turning this into a memory/speed claim.
        large=root/'large.json';blocked=root/'must-not-exist.json'
        initial=json.loads(json.dumps(cases[0]['initial']))
        initial['font_face']['family']='F'*32768
        text=''.join(f'\\H{n}.5;word' for n in range(3000))
        large.write_text(json.dumps([dict(content=text,initial=initial,properties=False)]),encoding='utf-8')
        files=set(root.iterdir())
        failure=subprocess.run([driver,str(large),str(blocked)],capture_output=True,timeout=120)
        assert failure.returncode!=0 and b'exceeds 64 MiB' in failure.stderr and not blocked.exists(),failure.stderr
        assert set(root.iterdir())==files,'Failed serialization left a partial temporary file'
        oversized=root/'oversized.json';oversized.write_bytes(b' '*(32*1024*1024+1))
        failure=subprocess.run([driver,str(oversized),str(blocked)],capture_output=True,timeout=120)
        assert failure.returncode!=0 and b'exceeds 32 MiB' in failure.stderr and not blocked.exists(),failure.stderr
        print(f'{len(cases)} fresh MTEXT cases / {tokens} tokens / {errors} original errors or finite guards match the optimized native parser; overwrite, input/output limits and atomic cleanup verified')


if __name__=='__main__':main()
