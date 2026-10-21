"""Fresh modelspace import comparison using the production compiled reader."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json
import subprocess
import sys

import ezdxf
from generate_cad_space_fixtures import VERSIONS,build,mutate,write,evaluate,original


def main():
    driver=str(Path(sys.argv[1]).resolve());ref=original()
    with TemporaryDirectory(prefix='SleufBase-modelspace-runtime-') as temp:
        root=Path(temp);drawings=[];expected=[]
        for n,version in enumerate(VERSIONS):
            doc,markers,text=build(version,shift=137*(n+1))
            for binary in [False,True]:
                mode='moved-model' if binary else 'late-tables'
                path=root/f'fresh-{version}-{mode}.dxf';write(path,mutate(doc,markers,text,mode),doc.dxfversion,binary)
                drawings.append(str(path));expected.append(evaluate(path,ref))
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(dict(directories=[],drawings=drawings,summary=True)),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        total=0
        for name,found,want in zip(drawings,actual,expected):
            assert found['space_policy']=='modelspace',name
            assert not found['warnings'] and not found['block_warnings'],(name,found['warnings'],found['block_warnings'])
            assert [dict(kind=e['kind'],handle=e['handle']) for e in found['roots']]==want['selected'],name
            for p in want['paths']:
                e=next(e for e in found['roots'] if e['handle']==p['handle'])
                assert [[x,y] for x,y,z in e['points']]==[list(v) for v in p['points']],name
            b=found['bounds'];assert [b['min_x'],b['min_y'],b['max_x'],b['max_y']]==want['bounds'],(name,b,want['bounds'])
            total+=len(want['selected'])
        print(f'{len(drawings)} fresh R12/R2000/R2007/R2018 drawings / {total} selected roots match original modelspace import, paths and map bounds; inactive pages and huge paper curves are excluded')


if __name__=='__main__':main()
