"""Original font-factory routes, including filesystem changes after indexing.

No installed fonts/settings/accounts are read. All fonts are original fixtures.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import os

import ezdxf
from ezdxf.disassemble import make_primitive
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from ezdxf.tools.text import plain_text
from fontTools.ttLib import TTFont

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_FONT_RESOLUTION_FIXTURES',ROOT/'rust/tests/fixtures/font-resolution'))
ASSETS=ROOT/'rust/tests/fixtures/font-resolution'
SOURCES={'ttf':'../dxf-text-metrics/SleufBaseFixture.ttf','shx':'../stroke-font/fixture-1.0.shx',
         'shp':'../stroke-font/fixture.shp','lff':'../lff-font/polygons.lff','no-outline':'no-outline.ttf'}
SPEC=dict(content='Axp ?Ø',insert=[2.5,-3,1],align_point=[19,3,-2],height=2.5,width=.8,
          rotation=23,oblique=-11,halign=1,valign=2,generation=2,normal=[1,2,3])
BAD_METRICS='*0,4,own bad font\n21,5,0,0\n*65,3,A\n7,65,0\n*120,1,x\n0\n'

def actual_path(font,manager):
    """Inspect the font actually backing the glyph cache, including replacements."""
    if isinstance(font,fonts.MonospaceFont):return None
    if isinstance(font,fonts.TrueTypeFont):
        key=next(k for k,v in manager._loaded_ttf_fonts.items() if v is font.glyph_cache.font)
        return manager._font_cache.get(key,manager.fallback_font_name()).file_path
    caches=manager._loaded_lff_glyph_caches if isinstance(font,fonts.LibreCadFont) else manager._loaded_shape_file_glyph_caches
    key=next(k for k,v in caches.items() if v is font.glyph_cache)
    fallback=manager.fallback_lff() if isinstance(font,fonts.LibreCadFont) else manager.fallback_shapefile()
    return manager._font_cache.get(key,fallback).file_path


def definitions():
    def case(name,files,names,removals=None):
        return dict(name=name,files=[dict(name=n,source=s) if s in SOURCES else dict(name=n,text=s) for n,s in files],
            queries=[dict(name=n,text=SPEC,remove=(removals or {}).get(i,[])) for i,n in enumerate(names)])
    return [
        case('all-valid',[('Arial.ttf','ttf'),('own.shx','shx'),('own.shp','shp'),('own.lff','lff')],
             ['Arial.ttf','own.shx','own.shp','own.lff','*monospace','unknown.ttf','unknown.ext']),
        case('bigfont-ttf',[('Arial.ttf','ttf'),('big.shx','AutoCAD-86 bigfont 1.0\r\n\u001aOriginal fixture')],['big.shx','BIG.SHX','big']),
        case('unknown-shx-ttf',[('Arial.ttf','ttf'),('unknown.shx','Original unknown SHX signature')],['unknown.shx']),
        case('bad-shape-metrics-ttf',[('Arial.ttf','ttf'),('bad.shp',BAD_METRICS)],['bad.shp']),
        case('bigfont-monospace',[('big.shx','AutoCAD-86 bigfont 1.0\r\n\u001aOriginal fixture')],['big.shx']),
        case('missing-everything',[],['missing.ttf','missing.shx','missing.lff','unknown.ext','txt']),
        case('malformed-known-shx',[('Arial.ttf','ttf'),('bad.shx','AutoCAD-86 shapes 1.0\r\n\u001a')],['bad.shx']),
        case('bad-lff-remains-error',[('Arial.ttf','ttf'),('bad.lff','#Name: Own bad LFF\n[0042]\n0,0;1,1')],['bad.lff']),
        case('unrenderable-ttf',[('Arial.ttf','ttf'),('no-outline.ttf','no-outline')],['no-outline.ttf']),
        case('bad-default-ttf',[('Arial.ttf','no-outline')],['Arial.ttf','missing.ttf']),
        case('known-ttf-disappears',[('Arial.ttf','ttf'),('gone.ttf','ttf')],['gone.ttf'],{0:['gone.ttf']}),
        case('known-shx-disappears',[('Arial.ttf','ttf'),('gone.shx','shx')],['gone.shx'],{0:['gone.shx']}),
        case('missing-shx-fallback-disappears',[('Arial.ttf','ttf'),('txt.shx','shx')],['missing.shx'],{0:['txt.shx']}),
        case('missing-shx-fallback-bigfont',[('Arial.ttf','ttf'),('txt.shx','AutoCAD-86 bigfont 1.0\r\n\u001aOriginal fixture')],['missing.shx']),
        case('known-lff-disappears',[('Arial.ttf','ttf'),('gone.lff','lff')],['gone.lff'],{0:['gone.lff']}),
        case('loaded-ttf-survives-delete',[('Arial.ttf','ttf')],['Arial.ttf','Arial.ttf'],{1:['Arial.ttf']}),
        case('loaded-shx-survives-delete',[('Arial.ttf','ttf'),('own.shx','shx')],['own.shx','own.shx'],{1:['own.shx']}),
    ]


def run_case(case):
    with TemporaryDirectory(prefix='SleufBase-original-font-factory-') as temp:
        root=Path(temp)
        for file in case['files']:
            data=((OUT if file['source']=='no-outline' else ASSETS)/SOURCES[file['source']]).read_bytes() if 'source' in file else file['text'].encode('utf-8')
            (root/file['name']).write_bytes(data)
        manager=FontManager();manager.build([str(root)],support_dirs=False)
        fonts.TrueTypeFont._glyph_caches.clear()
        with patch.object(fonts,'font_manager',manager):
            results=[]
            for query in case['queries']:
                for name in query['remove']:(root/name).unlink(missing_ok=True)
                s=query['text']
                try:
                    f=fonts.make_font(query['name'],s['height'],s['width'])
                    doc=ezdxf.new('R2018');doc.styles.get('Standard').dxf.font=query['name']
                    e=doc.modelspace().add_text(s['content'],dxfattribs=dict(insert=s['insert'],align_point=s['align_point'],
                        height=s['height'],width=s['width'],rotation=s['rotation'],oblique=s['oblique'],halign=s['halign'],
                        valign=s['valign'],text_generation_flag=s['generation'],extrusion=s['normal']))
                    primitive=make_primitive(e);fm=f.measurements
                    points=list(primitive.path.control_vertices()) if primitive.path else []
                    physical=actual_path(f,manager);selected=physical.name if physical else None
                    results.append(dict(status='ok',kind=type(f).__name__,selected=selected,extent=dict(
                        width=f.text_width(plain_text(s['content'])),measurements=dict(zip(fm._fields,fm)),
                        corners=[list(p) for p in points[:4]] if points else None)))
                except Exception as exc:results.append(dict(status='error',legacy_error=type(exc).__name__))
        return results


def main():
    assert ezdxf.__version__=='1.4.4'
    OUT.mkdir(parents=True,exist_ok=True)
    font=TTFont(ASSETS/SOURCES['ttf']);font.recalcBBoxes=False;font.recalcTimestamp=False;del font['glyf'];font.save(OUT/'no-outline.ttf')
    cases=definitions()
    for case in cases:case['expected']=run_case(case)
    data=dict(sources=SOURCES,cases=cases)
    (OUT/'reference.json').write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'{len(cases)} original font-factory scenarios / {sum(len(c["queries"]) for c in cases)} queries')
    for c in cases:print(c['name'],[(x['kind'],x['selected']) if x['status']=='ok' else x for x in c['expected']])


if __name__=='__main__':main()
