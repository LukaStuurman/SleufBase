"""Original synthetic LibreCAD fonts and independent ezdxf 1.4.4 references."""
from pathlib import Path
from unittest.mock import patch
from tempfile import TemporaryDirectory
import json
import os

import ezdxf
from ezdxf.disassemble import make_primitive
from ezdxf.fonts import fonts, lff
from ezdxf.fonts.font_manager import FontManager
from ezdxf.tools.text import plain_text

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('SLEUFBASE_LFF_FONT_FIXTURES', ROOT/'rust/tests/fixtures/lff-font'))


def base():
    return '''# Name: Original SleufBase LFF
# LetterSpacing: 1.25
# WordSpacing: 8
[0041] A
-2,0;11,0;11,21;-2,21;-2,0
[0078] x
0,-2;8,-2;8,12;0,12;0,-2
[0070] p
1,-7;10,-7;10,11;1,11;1,-7
'''


def files():
    polygons = base()+'''[0042] Empty polyline
one
[0043] Single point ignored
99,99
[0044] Multiple disconnected contours
0,0;7,8;0,12
20,12;25,12
[0045] Invalid coordinates become zero, extra dimensions ignored
no,0;7,12,0,ignored;0,12;0,0
[0046] A prefix only before whitespace
0,0;A4, A7;5,8
[0047] Underlines and Unicode decimal values
0,0;1_0,١٢;0,0
'''
    curves = base()
    n = 128
    for x, y in [(10, 7), (-12, 4), (0, -10), (7, 0)]:
        for b in [0, .0000005, .000001, .25, -.25, 1, -1, 1.75, -1.75]:
            curves += f'[{n:04x}] Curved\n1,-2;{x+1},{y-2},A{b};4,3\n'; n += 1
    curved_metrics = curves.replace('-2,0;11,0;11,21;-2,21;-2,0', '0,0;12,9,A.75;20,0\n4,8;12,18')
    curved_metrics = curved_metrics.replace('0,-2;8,-2;8,12;0,12;0,-2', '0,-2;9,12,A-.6;14,-2')
    curved_metrics = curved_metrics.replace('1,-7;10,-7;10,11;1,11;1,-7', '0,0;9,-7,A-1;10,11')
    inherited = base()+'''[00c1] Inherits old A
C0041
2,22;5,25
[00c2] Two ancestors
C00C1
8,23;10,26
[0041] Replacing A must not alter existing children
0,0;7,0;7,20;0,20;0,0
[00c3] Inherits current A
C0041
7,21;4,24
[0044] Missing parent is skipped
CFFFF
0,0;8,8
[0045] Invalid parent is skipped
Cinvalid
0,0;8,8
[0046] Copy only
C00C2
'''
    syntax = polygons.replace('# LetterSpacing: 1.25', '# LetterSpacing: 0.5\n# LetterSpacing: 1.25\n# LetterSpacing: invalid')
    syntax += '# WordSpacing: 9\n# WordSpacing: invalid\n# Name: Last valid name\n# Name: ignored: extra colon\n[wrong004d] M\n0,0;9,5;1,2\n[٠٠٤٨] H\n0,0;8,9\n[0x0049] I\nC0x_0041\n1,22;2,23\n'
    fallback = '''# Name: Only A
# LetterSpacing: .5
# WordSpacing: 4
[0041] A
3,0;15,0;15,18;3,18;3,0
'''
    empty_metrics=base().replace('0,-2;8,-2;8,12;0,12;0,-2','one').replace('1,-7;10,-7;10,11;1,11;1,-7','one')
    return {'polygons.lff': polygons, 'curves.lff': curved_metrics,
            'inherited.lff': inherited, 'syntax.lff': syntax, 'fallback.lff': fallback,'empty-metrics.lff':empty_metrics}


def json_lines(path, data):
    # One complete case per line keeps the generated corpus inspectable.
    chunks = ['{']
    for i, (key, value) in enumerate(data.items()):
        if isinstance(value, list):
            chunks.append(json.dumps(key)+': [')
            chunks.extend(json.dumps(v, ensure_ascii=False)+(',' if j+1<len(value) else '') for j, v in enumerate(value))
            chunks.append(']'+(',' if i+1<len(data) else ''))
        else: chunks.append(json.dumps(key)+': '+json.dumps(value, ensure_ascii=False)+(',' if i+1<len(data) else ''))
    path.write_text('\n'.join(chunks+['}', '']), encoding='utf-8')


def main():
    assert ezdxf.__version__ == '1.4.4'
    OUT.mkdir(parents=True, exist_ok=True)
    traces = []; extents = []; metadata = []
    for filename, text in files().items():
        (OUT/filename).write_text(text, encoding='utf-8')
        program = lff.loads(text)
        metadata.append(dict(file=filename, name=program.name, letter_spacing=program.letter_spacing,
                             word_spacing=program.word_spacing, codes=sorted(program._glyphs)))
        for code, glyph in sorted(program._glyphs.items()):
            p = glyph.to_path()
            traces.append(dict(file=filename, code=code, expected=dict(controls=[list(v) for v in p.control_vertices()],
                end=list(p.end) if len(p) else [0., 0.], commands=len(p))))
        cache = lff.GlyphCache(program)
        doc = ezdxf.new('R2018'); doc.styles.get('Standard').dxf.font = filename
        with patch.object(fonts.font_manager, 'get_lff_glyph_cache', return_value=cache):
            for content in ['Axp ÁÂ ?漢🛠', 'A^Ix', '  ', 'BCDEFGM']:
                for h, v in [(h, 0) for h in range(6)]+[(h, v) for v in [1, 2, 3] for h in range(3)]:
                    n = len(extents)
                    e = doc.modelspace().add_text(content, dxfattribs=dict(insert=(2.5, -3, 1), align_point=(19, 3, -2),
                        height=[.25, 1., 2.5][n % 3], width=[.7, 1., 1.4][n % 3], rotation=[0, 27, -90][n % 3],
                        oblique=[0, 17, -12][n % 3], halign=h, valign=v, text_generation_flag=[0, 2, 4, 6][n % 4],
                        extrusion=[(0, 0, 1), (1, 2, 3)][n % 2]))
                    d = e.dxf; f = fonts.make_font(filename, d.height, d.width); fm = f.measurements
                    spec = dict(content=d.text, insert=list(d.insert), align_point=list(d.align_point), height=d.height,
                        width=d.width, rotation=d.rotation, oblique=d.oblique, halign=h, valign=v,
                        generation=d.text_generation_flag, normal=list(d.extrusion))
                    p = make_primitive(e).path; corners = [list(v) for v in p.control_vertices()][:4] if p else None
                    extents.append(dict(file=filename, text=spec, expected=dict(width=f.text_width(plain_text(d.text)),
                        measurements=dict(zip(fm._fields, fm)), corners=corners)))
    # Independent proof of the old fallback-list typo, separate from correct known-font cases.
    with TemporaryDirectory(prefix='SleufBase-original-lff-fallback-') as temp:
        folder=Path(temp);(folder/'standard.lff').write_text(base(),encoding='utf-8')
        manager = FontManager(); manager.build([str(folder)], support_dirs=False)
        assert manager.has_font('standard.lff')
        legacy = manager.fallback_lff(); assert legacy == ''
        (folder/'txt.shx').write_bytes((ROOT/'rust/tests/fixtures/stroke-font/fixture-1.0.shx').read_bytes())
        manager=FontManager();manager.build([str(folder)],support_dirs=False)
        wrong_type=manager.fallback_lff();assert wrong_type=='txt.shx'
    json_lines(OUT/'reference.json', dict(metadata=metadata, traces=traces, extents=extents,
        legacy_missing_lff_fallback=legacy,legacy_wrong_lff_fallback=wrong_type))
    print(f'{len(metadata)} original fonts / {len(traces)} glyph traces / {len(extents)} text extents; legacy LFF fallback typo reproduced')


if __name__ == '__main__': main()
