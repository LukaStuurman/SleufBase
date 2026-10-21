"""Fresh curved glyphs exercise the compiled metric driver against original ezdxf.

No saved expected data is used here. Fonts are original temporary test programs.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import subprocess
import sys

import ezdxf
from ezdxf.disassemble import make_primitive
from ezdxf.fonts import fonts, shapefile
from ezdxf.tools.text import plain_text
from generate_stroke_font_fixtures import shp, shx
from verify_native_template_extents import compare


def main():
    driver = str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-stroke-font-runtime-') as temp:
        out = Path(temp); requests = []; expected = []; paths = {}
        for i in range(8):
            programs = {32: [2, 8, 5+i, 0, 0],
                65: [10, 12+i, 4, 2, 8, 41+3*i, 0, 0],
                120: [2, 8, 0, -2, 1, 12, 7+i, 5, 60+i, 2, 8, 5, -3, 0],
                112: [12, 8+i, -7, -127+i, 2, 8, 6, 7, 0],
                0x431: [7, 65, 1, 8, 3, 0, 0]}
            name = f'fresh{i}'+('.shp' if i % 2 == 0 else '.shx')
            p = out/name
            p.write_bytes(shp(programs, 19+i, 0, unicode=True) if i % 2 == 0 else
                          shx(programs, above=19+i, below=0, unicode=True))
            paths[name] = str(p)
            cache = shapefile.GlyphCache(shapefile.readfile(str(p)))
            doc = ezdxf.new('R2018'); doc.styles.get('Standard').dxf.font = name
            with patch.object(fonts.font_manager, 'get_shapefile_glyph_cache', return_value=cache):
                for h, v in [(h, 0) for h in range(6)]+[(h, v) for v in [1, 2, 3] for h in range(3)]:
                    e = doc.modelspace().add_text('Axp б ?%%c', dxfattribs=dict(insert=(1, -2, 3),
                        align_point=(20, 4, 1), height=.7+i/3, width=.8+i/20,
                        rotation=i*7-20, oblique=i*2, halign=h, valign=v,
                        text_generation_flag=(i % 4)*2, extrusion=(1+i/10, 2, 3)))
                    d = e.dxf
                    text = dict(content=d.text, insert=list(d.insert), align_point=list(d.align_point),
                        height=d.height, width=d.width, rotation=d.rotation, oblique=d.oblique,
                        halign=h, valign=v, generation=d.text_generation_flag, normal=list(d.extrusion))
                    f = fonts.make_font(name, d.height, d.width); fm = f.measurements
                    primitive = make_primitive(e); vertices = list(primitive.path.control_vertices())
                    requests.append(dict(font=name, text=text))
                    expected.append(dict(width=f.text_width(plain_text(d.text)),
                        measurements=dict(zip(fm._fields, fm)), corners=[list(v) for v in vertices[:4]]))
        source = out/'input.json'; target = out/'actual.json'
        source.write_text(json.dumps(dict(fonts=paths, texts=requests)), encoding='utf-8')
        subprocess.run([driver, str(source), str(target)], check=True, timeout=120)
        actual = json.loads(target.read_text(encoding='utf-8'))
        assert actual['groups'] == []
        compare(actual['texts'], expected, 'fresh SHP/SHX curves/metrics/OCS')
        print(f'{len(paths)} fresh stroke fonts / {len(expected)} compiled text extents match original ezdxf, including curved A/x/p, missing glyphs and Unicode subshapes')


if __name__ == '__main__': main()
