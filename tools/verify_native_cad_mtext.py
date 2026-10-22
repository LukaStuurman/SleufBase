"""Fresh actual DXF import/project-scene geometry with both MTEXT routes."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,subprocess,sys
from fontTools.ttLib import TTFont
import generate_cad_mtext_fixtures as oracle
from generate_mtext_render_fixtures import make_fonts
from verify_native_cad_attrib import compare_paths

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-source-MTEXT-runtime-') as temp:
        root=Path(temp);assets=root/'assets';make_fonts(assets)
        for n,path in enumerate(sorted((assets/'fonts').rglob('*.ttf'))):
            f=TTFont(path);f.recalcTimestamp=False
            for name in ['A','x','p','M']:
                glyph=f['glyf'][name]
                for j,(x,y) in enumerate(glyph.coordinates):glyph.coordinates[j]=(x+4*n,y+3*n)
                w,b=f['hmtx'].metrics[name];f['hmtx'].metrics[name]=(w+19*n,b-2*n)
            f.save(path)
        oracle.OUT=root/'drawings';oracle.FONTS=assets;oracle.SHIFT=7.375;oracle.main()
        wanted=json.loads((oracle.OUT/'reference.json').read_text(encoding='utf-8'))
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(dict(directories=[str(p) for p in sorted((assets/'fonts').iterdir())],drawings=[str(oracle.OUT/n) for n in ['mtext-ascii.dxf','mtext-binary.dxf']])),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==2
        by_handle={c['handle']:c for c in wanted['cases']};count=0
        for scene in actual:
            assert len(scene)==len(wanted['scene'])
            for n,(found,want) in enumerate(zip(scene,wanted['scene'])):
                assert found['handle']==want['handle'] and found['kind']=='MTEXT',(n,found)
                assert found['content']==by_handle[want['handle']]['content'],n
                compare_paths(found['paths'],want['paths'],f'{count} {want["handle"]}')
                count+=1
        print(f'2 fresh ASCII/binary DXFs / {count} MTEXT scene instances match original dense world curves to 0.005 units with changed physical fonts, simple/complex routing, real auto widths and nested reflected nonuniform grids')

if __name__=='__main__':main()
