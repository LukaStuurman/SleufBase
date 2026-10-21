"""Fresh LFF geometry/metrics exercised through the compiled native driver."""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import subprocess
import sys

import ezdxf
from ezdxf.disassemble import make_primitive
from ezdxf.fonts import fonts, lff
from ezdxf.tools.text import plain_text
from verify_native_template_extents import compare


def main():
    with TemporaryDirectory(prefix='SleufBase-lff-runtime-') as temp:
        out=Path(temp); requests=[]; expected=[]; paths={}
        for i in range(8):
            name=f'fresh{i}.lff';p=out/name
            text=f'''# Name: Fresh LFF {i}
# LetterSpacing: {i/7+.25}
# WordSpacing: {3+i/2}
[0041]
0,0;{10+i},12,A{.4+i/20};{18+i},0
4,8;12,18
[0078]
0,-2;{7+i},12,A{-1+i/20};14,-2
[0070]
0,0;9,-7,A-1;10,11
[00c1]
C0041
2,22;7,{24+i}
[0042]
one
'''
            p.write_text(text,encoding='utf-8');paths[name]=str(p)
            cache=lff.GlyphCache(lff.loads(text))
            doc=ezdxf.new('R2018');doc.styles.get('Standard').dxf.font=name
            with patch.object(fonts.font_manager,'get_lff_glyph_cache',return_value=cache):
                for h,v in [(h,0) for h in range(6)]+[(h,v) for v in [1,2,3] for h in range(3)]:
                    e=doc.modelspace().add_text('Axp Á B?漢^Ix',dxfattribs=dict(insert=(1,-2,3),align_point=(20,4,1),
                        height=.7+i/3,width=.8+i/20,rotation=i*7-20,oblique=i*2,halign=h,valign=v,
                        text_generation_flag=(i%4)*2,extrusion=(1+i/10,2,3)))
                    d=e.dxf;f=fonts.make_font(name,d.height,d.width);fm=f.measurements
                    spec=dict(content=d.text,insert=list(d.insert),align_point=list(d.align_point),height=d.height,
                        width=d.width,rotation=d.rotation,oblique=d.oblique,halign=h,valign=v,
                        generation=d.text_generation_flag,normal=list(d.extrusion))
                    primitive=make_primitive(e);points=list(primitive.path.control_vertices())
                    requests.append(dict(font=name,text=spec))
                    expected.append(dict(width=f.text_width(plain_text(d.text)),measurements=dict(zip(fm._fields,fm)),
                        corners=[list(v) for v in points[:4]]))
        source=out/'input.json';target=out/'actual.json'
        source.write_text(json.dumps(dict(fonts=paths,texts=requests)),encoding='utf-8')
        subprocess.run([str(Path(sys.argv[1]).resolve()),str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert actual['groups']==[]
        compare(actual['texts'],expected,'fresh LFF curves/inheritance/empty/control/OCS')
        print(f'{len(paths)} fresh LFF fonts / {len(expected)} compiled text extents match original ezdxf without saved expected data')


if __name__=='__main__':main()
