"""Fresh ATTDEF definitions and visible glyphs through the optimized reader."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json
import shutil
import subprocess
import sys

import ezdxf
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from generate_cad_attdef_fixtures import build,evaluate,VERSIONS,FILES,ASSETS
from verify_native_cad_attrib import compare_paths


def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-attdef-runtime-') as temp:
        root=Path(temp)
        for file in FILES:shutil.copyfile(ASSETS/file,root/Path(file).name)
        manager=FontManager();manager.build([str(root)],support_dirs=False);fonts.TrueTypeFont._glyph_caches.clear()
        drawings=[];expected=[]
        for n,version in enumerate(VERSIONS):
            doc=build(version,shift=37*(n+1))
            for e in doc.modelspace().query('ATTDEF'):
                e.dxf.rotation+=13*n;e.dxf.height*=1.+n*.17;e.dxf.oblique-=3*n
                if not e.has_embedded_mtext_entity:e.dxf.tag=e.dxf.tag.replace('A%%cx','M%%cx')
            for fmt in ['asc','bin']:
                path=root/f'fresh-{version}-{fmt}.dxf';doc.saveas(path,fmt=fmt)
                drawings.append(str(path));expected.append(evaluate(ezdxf.readfile(path),manager)['scene'])
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps(dict(directories=[str(root)],drawings=drawings)),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));count=0;definitions=0
        assert len(actual)==len(expected)
        for scene,wanted in zip(actual,expected):
            lookup={(e['handle'],e['blockpath']):e for e in wanted};assert len(scene)==len(lookup)
            for node in scene:
                key=(node['handle'],node['blockpath']);want=lookup.pop(key)
                for field in ['kind','layer','rgb','visible']:assert node[field]==want[field],(key,field)
                if node['kind'] in ['ATTDEF','ATTRIB']:
                    assert node['content']==want['content'],key
                    for field in ['tag','flags','field_length','lock_position']:assert node['attribute'][field]==want[field],(key,field)
                    if node['kind']=='ATTDEF':assert node['attribute']['prompt']==want['prompt'];definitions+=1
                    compare_paths(node['paths'],want['paths'],key)
                else:
                    import numpy as np
                    assert len(node['paths'])==1 and np.max(np.abs(np.asarray(node['paths'][0])-want['points']))<1e-8,key
                count+=1
            assert not lookup
        print(f'{len(drawings)} fresh R12-R2018 ASCII/binary drawings / {count} scene nodes / {definitions} root ATTDEFs match original tag glyphs, metadata, visibility and INSERT exclusion')


if __name__=='__main__':main()
