"""Fresh fonts/drawings through the optimized actual MTEXT scene and palette."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json, subprocess, sys
from fontTools.ttLib import TTFont
import generate_cad_mtext_paint_fixtures as oracle
from generate_mtext_render_fixtures import make_fonts
from verify_native_cad_attrib import compare_paths

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-MTEXT-paint-runtime-') as temp:
        root=Path(temp);assets=root/'assets';make_fonts(assets)
        for n,path in enumerate(sorted((assets/'fonts').rglob('*.ttf'))):
            f=TTFont(path);f.recalcTimestamp=False
            for name in ['A','x','p','M']:
                glyph=f['glyf'][name]
                for j,(x,y) in enumerate(glyph.coordinates):glyph.coordinates[j]=(x+3*n,y+2*n)
                w,b=f['hmtx'].metrics[name];f['hmtx'].metrics[name]=(w+17*n,b-3*n)
            f.save(path)
        # Also change physical stroke-font advances/coordinates: no fixed
        # canonical font bytes or expected source positions can pass this audit.
        shp=next((assets/'fonts').rglob('own.shp'))
        old=shp.read_text(encoding='utf-8')
        changed=old.replace('1,8,13,0,8,0,21,8,-13,0,8,0,-21,2,8,17,0',
                            '1,8,15,0,8,0,21,8,-15,0,8,0,-21,2,8,19,0')
        assert changed!=old
        shp.write_text(changed,encoding='utf-8')
        oracle.OUT=root/'drawings';oracle.FONTS=assets;oracle.SHIFT=9.125
        wanted=oracle.main()['scene']
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(dict(directories=[str(p) for p in sorted((assets/'fonts').iterdir())],
            drawings=[str(oracle.OUT/n) for n in ['paint-ascii.dxf','paint-binary.dxf']],painted=True)),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==2
        count=0;contours=0
        for scene in actual:
            assert len(scene)==len(wanted)
            for n,(found,want) in enumerate(zip(scene,wanted)):
                for field in ['handle','content','layer','rgb','visible']:assert found[field]==want[field],(n,field,found[field],want[field])
                assert found['colors']==[p['rgb'] for p in want['paths']],(n,found['colors'])
                compare_paths(found['paths'],[{k:v for k,v in p.items() if k!='rgb'} for p in want['paths']],f'{count} {want["handle"]}')
                count+=1;contours+=len(found['colors'])
        print(f'2 fresh ASCII/binary DXFs / {count} colored MTEXT instances / {contours} painted contours match original world curves and every palette role, with changed physical fonts and inherited reflected grids')

if __name__=='__main__':main()
