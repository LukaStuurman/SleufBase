"""Fresh font bytes and complete original simple MTEXT vs optimized native."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,os,subprocess,sys
from fontTools.ttLib import TTFont
from generate_mtext_render_fixtures import make_fonts
from generate_stroke_font_fixtures import shx
from generate_mtext_simple_fixtures import requests,evaluate,estimate_requests,estimate
from verify_native_mtext_layout import compare

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-simple-MTEXT-runtime-') as temp:
        root=Path(temp);make_fonts(root)
        for n,path in enumerate(sorted((root/'fonts').rglob('*.ttf'))):
            f=TTFont(path);f.recalcTimestamp=False
            for name in ['A','x','p','M']:
                glyph=f['glyf'][name]
                for j,(x,y) in enumerate(glyph.coordinates):glyph.coordinates[j]=(x+5*n,y+3*n)
                w,b=f['hmtx'].metrics[name];f['hmtx'].metrics[name]=(w+17*n,b-3*n)
            f.save(path)
        programs={32:[2,8,7,0,0],65:[1,8,10,0,8,0,23,8,-10,0,8,0,-23,2,8,12,0,3,3,8,1,0,4,3,0],
            120:[1,8,7,0,8,0,11,8,-7,0,8,0,-11,2,8,9,0,0],
            112:[1,8,0,-6,8,8,0,8,0,19,8,-8,0,2,8,10,-13,0]}
        for path in (root/'fonts').rglob('*.shx'):path.write_bytes(shx(programs,above=23,below=6,unicode=True))
        for path in (root/'fonts').rglob('*.shp'):
            data='*0,4,Own fresh simple SHP\n23,6,0,0\n'+''.join(f'*{code},{len(p)},shape\n'+','.join(map(str,p))+'\n' for code,p in programs.items())
            path.write_text(data,encoding='utf-8')
        for n,path in enumerate(sorted((root/'fonts').rglob('*.lff'))):
            path.write_text(f'''#Name: Own fresh simple LFF {n}
#LetterSpacing: {1.375+n*.25}
#WordSpacing: {7.125+n*.5}
[0041]
0,0;10,0;10,23,A0.5;0,23;0,0
[0078]
0,-1;7,-1;7,11;0,11;0,-1
[0070]
0,-6;8,-6,A0.3;8,14;0,14;0,-6
''',encoding='utf-8')
        cases=list(requests(seed=519731,count=192))
        for n,r in enumerate(cases):
            r['width']+=.125*(n%3);r['insert'][0]+=.375;r['insert'][1]-=.625
        expected=[evaluate(r,root) for r in cases]
        estimate_cases=list(estimate_requests())
        expected += [dict(output=estimate(r,root)) for r in estimate_cases]
        cases += estimate_cases
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(cases,ensure_ascii=False,allow_nan=False),encoding='utf-8')
        env={**os.environ,'SLEUFBASE_MTEXT_RENDER_FIXTURES':str(root)}
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120,env=env)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        lines=0
        for n,(found,want) in enumerate(zip(actual,expected)):
            assert 'output' in want,(n,want)
            assert 'output' in found,(n,found)
            compare(found['output'],want['output'],str(n));lines+=len(want['output'].get('lines',[]))
        before=target.read_bytes();failure=subprocess.run([driver,str(source),str(target)],capture_output=True,timeout=120,env=env)
        assert failure.returncode!=0 and target.read_bytes()==before,'Existing simple MTEXT output was replaced'
        print(f'{len(cases)-len(estimate_cases)} fresh simple MTEXT documents / {lines} wrapped lines / {len(estimate_cases)} fresh extent estimates match original decoding, widths, all anchors, matrices and changed physical glyphs; atomic overwrite refusal checked')

if __name__=='__main__':main()
