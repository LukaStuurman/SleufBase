"""Original ezdxf SHP/SHX traces and text extents; entirely original test fonts.

Does not import SleufBase or read user preferences. The two deliberate corrections
to the old interpreter have separate, hand-specified Autodesk regression cases.
"""
from pathlib import Path
from unittest.mock import patch
import json
import os
import struct

import ezdxf
from ezdxf import path
from ezdxf.disassemble import make_primitive
from ezdxf.fonts import fonts, shapefile as sh
from ezdxf.tools.text import plain_text

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('SLEUFBASE_STROKE_FONT_FIXTURES', ROOT / 'rust/tests/fixtures/stroke-font'))


def rectangle(width, bottom, top, advance):
    return [2, 8, 0, bottom, 1, 8, width, 0, 8, 0, top-bottom,
            8, -width, 0, 8, 0, bottom-top, 2, 8, advance, -bottom, 0]


def programs():
    shapes = {32: [2, 8, 7, 0, 0], 65: rectangle(13, 0, 21, 17),
              120: rectangle(9, -2, 12, 12), 112: rectangle(9, -7, 12, 12)}
    for i in range(16):
        shapes[128+i] = [(15 << 4) | i, 0]
    shapes.update({
        66: [4, 3, 8, 2, 1, 3, 2, 8, 2, -1, 4, 2, 8, 1, 1, 0],
        67: [5, 8, 4, 8, 6, 8, 7, 0, 0],
        68: [2, 8, 5, 0, 8, 3, 1, 1, 8, 4, 2, 2, 8, 5, -3, 8, 2, 0, 0],
        69: [9, 1, 2, -3, 4, -128, 127, 0, 0, 0],
        70: [4, 2, 7, 67, 3, 2, 8, 1, 0, 0],
        71: [14, 8, 12, -3, 8, 1, 1, 14, 4, 3, 8, 2, 1, 0],
        72: [14, 9, 3, 4, 5, 6, 0, 0, 8, 1, 2, 0],
        73: [14, 13, 8, 2, -70, 3, -4, 20, 0, 0, 8, 1, 2, 0],
        74: [5, 5, 5, 5, 8, 3, 2, 6, 6, 6, 6, 0],
        75: [2, 10, 30, 4, 1, 8, 2, 0, 0],
        76: [2, 12, 11, -5, -80, 1, 8, 2, 0, 0],
        77: [8, 0, 0, 8, 0, 0, 0],
        78: [13, 10, 2, 0, -10, -2, 127, 8, -5, -127, 0, 0, 0],
    })
    n = 144
    for octant in range(8):
        for span in [0, 1, 3, 7]:
            for sign in [1, -1]:
                shapes[n] = [10, 21, sign * (octant*16+span), 0]; n += 1
    for start, end, high, low, spec in [(1, 0, 0, 21, 3), (64, 192, 0, 43, 0x37),
            (192, 32, 0, 13, -0x63), (0, 0, 1, 7, 0x52),
            (0, 0, 0, 27, 0), (1, 200, 0, 31, -0x71), (255, 1, 0, 10, 0x11)]:
        shapes[n] = [11, start, end, high, low, spec, 0]; n += 1
    for bulge in [1, -1, 27, -27, 63, -63, 127, -127, 0]:
        shapes[n] = [12, 10, -7, bulge, 0]; n += 1
    return shapes


def shp(shapes, above=21, below=5, unicode=False):
    header = (f'*UNIFONT,6,SleufBase Original Fixture\n{above},{below},2,0,0,0\n'
              if unicode else f'*0,4,SleufBase Original Fixture\n{above},{below},2,0\n')
    body = []
    for num, codes in sorted(shapes.items()):
        body.extend([f'*{num},0,G{num}\n', ','.join(map(str, codes))+'\n'])
    return (header+''.join(body)).encode('ascii')


def encode(codes, unicode):
    result = bytearray(); i = 0
    while i < len(codes):
        code = codes[i]; i += 1; result.append(code)
        if code == 0: break
        if code == 7:
            value = codes[i]; i += 1
            result.extend(struct.pack('<H', value) if unicode else bytes([value]))
            continue
        count = {3: 1, 4: 1, 8: 2, 10: 2, 11: 5, 12: 3}.get(code, 0)
        if code in (9, 13):
            while True:
                x, y = codes[i:i+2]; i += 2; result.extend([x & 255, y & 255])
                if x == y == 0: break
                if code == 13: result.append(codes[i] & 255); i += 1
            continue
        for j in range(count):
            value = codes[i]; i += 1
            if (code == 10 and j == 1) or (code == 11 and j == 4):
                result.append((128 | -value) if value < 0 else value)
            else: result.append(value & 255)
    return bytes(result)


def shx(shapes, version='1.0', above=21, below=5, unicode=False):
    records = [(n, f'G{n}'.encode()+b'\0'+encode(c, unicode)) for n, c in sorted(shapes.items())]
    name = b'SleufBase Original Fixture\0'
    if unicode:
        definition = name + bytes([above, below, 2, 0, 0, 0])
        return (b'AutoCAD-86 unifont 1.0\r\n\x1a' + struct.pack('<HHH', len(records)+1, 0, len(definition))
                + definition + b''.join(struct.pack('<HH', n, len(r))+r for n, r in records))
    records.insert(0, (0, name+bytes([above, below, 2, 0])))
    return (f'AutoCAD-86 shapes {version}\r\n'.encode() + b'\x1a'
            + struct.pack('<HHH', records[0][0], records[-1][0], len(records))
            + b''.join(struct.pack('<HH', n, len(r)) for n, r in records)
            + b''.join(r for _, r in records) + b'EOF')


def trace(font, num, stacked):
    ctx = sh.ShapeRenderer(path.Path(), font.get_codes, stacked=stacked)
    ctx.render(num)
    return dict(controls=[list(p)[:2] for p in ctx.p.control_vertices()],
                end=list(ctx.p.end)[:2], commands=len(ctx.p))


def main():
    assert ezdxf.__version__ == '1.4.4'
    OUT.mkdir(parents=True, exist_ok=True)
    base = programs(); uni = {**base, 0x431: [8, 4, 8, 2, 8, 7, -8, 0],
                              0x432: [7, 0x431, 1, 8, 4, 1, 0]}
    curves = {**base, 65: [1, 10, 12, 4, 2, 8, 44, 0, 0],
              120: [2, 8, 0, -2, 1, 12, 10, 7, 63, 2, 8, 5, -5, 0],
              112: [1, 12, 8, -7, -127, 2, 8, 6, 7, 0]}
    syntax = [b'; Native fixture: comments, leading-0 hex, parentheses and continuation',
              b'*0,4,Native', b', Fixture', b'021,005,2,0']
    for n, codes in sorted(base.items()):
        lines = sh.Symbol(n, 0, f'G{n}'.encode(), codes).export_str()
        syntax.extend(lines)
    # Explicit short pair syntax plus a trailing-comma continuation.
    syntax.extend([b'*0250,0,Extra', b'002,08,(1,-02),001,', b'08,(3,04),0 ; end'])
    files = {'fixture.shp': shp(base), 'fixture-1.0.shx': shx(base),
             'fixture-1.1.shx': shx(base, '1.1'), 'unicode.shp': shp(uni, unicode=True),
             'unicode.shx': shx(uni, unicode=True),
             'derived.shp': shp(base, 0, 0),
             'fallback.shp': shp({k: v for k, v in base.items() if k not in [32, 65, 120, 112]}),
             'space-fallback.shp': shp({k: v for k, v in base.items() if k != 32}),
             'curves.shp': shp(curves, 0, 0), 'curves.shx': shx(curves),
             'syntax.shp': b'\n'.join(syntax)+b'\n'}
    traces = []; extents = []
    for filename, data in files.items():
        (OUT/filename).write_bytes(data)
        font = sh.shp_load(data) if filename.endswith('.shp') else sh.shx_load(data)
        # Both parser formats are independently exercised before traces are saved.
        for num in sorted(font.shapes):
            if num == 0: continue
            for stacked in [False, True]:
                traces.append(dict(file=filename, id=num, stacked=stacked, expected=trace(font, num, stacked)))
        cache = sh.GlyphCache(font)
        doc = ezdxf.new('R2018'); ms = doc.modelspace()
        doc.styles.get('Standard').dxf.font = filename
        with patch.object(fonts.font_manager, 'get_shapefile_glyph_cache', return_value=cache):
            for content in ['Axp BC', '  ', 'A%%cx', 'бв ?漢🛠', 'HINO', 'x p']:
                for h, v in [(h, 0) for h in range(6)]+[(h, v) for v in [1, 2, 3] for h in range(3)]:
                    n = len(extents)
                    e = ms.add_text(content, dxfattribs=dict(insert=(10.25, -3.75, 2), align_point=(22, -1.5, 4),
                        height=[.25, 1., 2.5][n % 3], width=[.7, 1., 1.4][n % 3], halign=h, valign=v,
                        rotation=[0, 27, -90][n % 3], oblique=[0, 17, -12][n % 3],
                        text_generation_flag=[0, 2, 4, 6][n % 4], extrusion=[(0, 0, 1), (1, 2, 3)][n % 2]))
                    d = e.dxf
                    spec = dict(content=d.text, insert=list(d.insert), align_point=list(d.align_point), height=d.height,
                        width=d.width, halign=h, valign=v, rotation=d.rotation, oblique=d.oblique,
                        generation=d.text_generation_flag, normal=list(d.extrusion))
                    f = fonts.make_font(filename, d.height, d.width); fm = f.measurements
                    primitive = make_primitive(e); vertices = list(primitive.path.control_vertices()) if primitive.path else []
                    extents.append(dict(file=filename, text=spec, expected=dict(width=f.text_width(plain_text(d.text)),
                        measurements=dict(zip(fm._fields, fm)), corners=[list(p) for p in vertices[:4]] if vertices else None)))
                    ms.delete_entity(e)
    # Keep actual legacy output visible; expected fixes are independent hand calculations.
    fixes = {66: [2, 7, 67, 0], 67: [8, 0, 10, 0], 68: [14, 14, 8, 3, 4, 0]}
    (OUT/'corrections.shp').write_bytes(shp(fixes))
    font = sh.shp_load((OUT/'corrections.shp').read_bytes())
    corrections = [dict(id=66, legacy=trace(font, 66, False),
                       expected=dict(controls=[], end=[0, 10], commands=0)),
                   dict(id=68, legacy=trace(font, 68, False),
                       expected=dict(controls=[[0, 0], [3, 4]], end=[3, 4], commands=1))]
    (OUT/'header12.shx').write_bytes(shx(base, above=12))
    try: sh.shx_load((OUT/'header12.shx').read_bytes())
    except sh.ShapeFileException as exc: header_error = type(exc).__name__
    else: raise AssertionError('legacy header-12 failure unexpectedly fixed')
    cache = sh.GlyphCache(sh.shp_load(files['fixture.shp']))
    try: cache.get_advance_width(9)
    except IndexError: control_error = 'IndexError'
    else: raise AssertionError('legacy first-use control glyph failure unexpectedly fixed')
    # The source has already cached its intended space-width fallback before
    # raising. Preserve the working cached behavior without the first-use crash.
    control_width = cache.get_text_length('A\tx', 1.)
    (OUT/'reference.json').write_text(json.dumps(dict(traces=traces, extents=extents,
        corrections=corrections, header12_legacy_error=header_error,
        control_legacy_error=control_error, control_expected_width=control_width), ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    print(f'{len(files)} fonts; {len(traces)} original traces; {len(extents)} original text extents; 4 explicit fixes')


if __name__ == '__main__': main()
