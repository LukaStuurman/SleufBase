"""Fresh compiled MTEXT audit, with changed font bytes and no saved expected data."""
from pathlib import Path
from tempfile import TemporaryDirectory
import copy,json,os,subprocess,sys
from fontTools.ttLib import TTFont
from generate_mtext_render_fixtures import make_fonts,requests,evaluate
from verify_native_mtext_layout import compare

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-MTEXT-font-runtime-') as temp:
        root=Path(temp);make_fonts(root)
        # Change both physical contours and advances independently of the
        # checked-in fixture; native must load and interpret these actual bytes.
        for n,path in enumerate(sorted((root/'fonts').rglob('*.ttf'))):
            f=TTFont(path);f.recalcTimestamp=False
            for name in ['A','x','p','M']:
                glyph=f['glyf'][name]
                for j,(x,y) in enumerate(glyph.coordinates):glyph.coordinates[j]=(x+3*n,y+2*n)
                w,b=f['hmtx'].metrics[name];f['hmtx'].metrics[name]=(w+13*n,b-2*n)
            f.save(path)
        cases=list(requests(seed=197531,count=192))
        for n,r in enumerate(cases):
            r['width']+=.37*(n%5);r['at']=[r['at'][0]+7.125,r['at'][1]-11.375]
            r['initial']['paragraph']['tab_stops']=[.125,'c2.50','r4.75'] if n%11==0 else []
        expected=[evaluate(r,root) for r in cases]
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(cases,ensure_ascii=False,allow_nan=False),encoding='utf-8')
        env={**os.environ,'SLEUFBASE_MTEXT_RENDER_FIXTURES':str(root)}
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120,env=env)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        words=draws=errors=0
        for n,(found,want) in enumerate(zip(actual,expected)):
            if 'output' not in want:assert 'error' in found,(n,found);errors+=1;continue
            assert 'output' in found,(n,found)
            compare(found['output'],want['output'],str(n))
            words+=len(want['output']['words']);draws+=len(want['output']['draws'])
        before=target.read_bytes();failure=subprocess.run([driver,str(source),str(target)],capture_output=True,timeout=120,env=env)
        assert failure.returncode!=0 and target.read_bytes()==before,'Existing MTEXT render audit output was replaced'
        print(f'{len(cases)} fresh composed MTEXT documents / {words} measured words / {draws} glyph and decoration draws / {errors} original errors match; changed physical font bytes and atomic overwrite protection checked')

if __name__=='__main__':main()
