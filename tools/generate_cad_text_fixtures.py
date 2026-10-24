"""Independent vector TEXT oracle, using only original redistributable fonts.

No SleufBase application initialization, settings, installed fonts or accounts.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import os
import shutil

import ezdxf
from ezdxf.addons import text2path
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from ezdxf.math import Matrix44

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT/'rust/tests/fixtures'
OUT = Path(os.environ.get('SLEUFBASE_CAD_TEXT_FIXTURES', ASSETS/'cad-text'))
FILES = ['dxf-text-metrics/SleufBaseFixture.ttf', 'stroke-font/fixture.shp',
         'stroke-font/fixture-1.0.shx', 'stroke-font/unicode.shx', 'stroke-font/curves.shx',
         'lff-font/polygons.lff', 'lff-font/curves.lff', 'lff-font/inherited.lff']


def paths(entity, font, matrix=None):
    with patch.object(text2path, 'get_font', return_value=font), patch.object(fonts, 'get_font_face', return_value=fonts.FontFace()):
        result = text2path.make_paths_from_entity(entity)
    data=[]
    for p in result:
        if matrix is not None: p=p.transform(matrix)
        if not len(p): continue
        commands=[]
        for c in p:
            points=[]
            if c.type == 2: points.append(list(c.ctrl))
            elif c.type == 3: points.extend([list(c.ctrl1),list(c.ctrl2)])
            points.append(list(c.end))
            commands.append(dict(kind=int(c.type),points=points))
        data.append(dict(start=list(p.start),commands=commands))
    return data


def spec(e):
    d=e.dxf
    return dict(content=d.text,insert=list(d.insert),align_point=list(d.align_point),height=d.height,
                width=d.width,rotation=d.rotation,oblique=d.oblique,halign=d.halign,valign=d.valign,
                generation=d.text_generation_flag,normal=list(d.extrusion))


def attributes(content, n, h=0, v=0):
    return dict(insert=(2.5,-3,1),align_point=(19,3,-2),height=[.25,1.,2.5][n%3],
                width=[.7,1.4,-.8][n%3],rotation=[0,27,-90][n%3],oblique=[0,17,-12][n%3],
                halign=h,valign=v,text_generation_flag=[0,2,4,6][n%4],
                extrusion=[(0,0,1),(0,0,-1),(1,2,3),(.001,.002,5)][n%4])


def main():
    assert ezdxf.__version__=='1.4.4'
    OUT.mkdir(parents=True,exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    cases=[]
    with TemporaryDirectory(prefix='SleufBase-original-CAD-text-') as temp:
        folder=Path(temp)
        for file in FILES: shutil.copyfile(ASSETS/file,folder/Path(file).name)
        manager=FontManager(); manager.build([str(folder)],support_dirs=False)
        fonts.TrueTypeFont._glyph_caches.clear()
        with patch.object(fonts,'font_manager',manager):
            for name in [Path(f).name for f in FILES]+['*monospace']:
                font=fonts.make_font(name,1.)
                doc=ezdxf.new('R2018'); doc.styles.get('Standard').dxf.font=name
                for content in ['Axp MØ', 'BCDEFGHIJ ?漢🛠', 'A%%cp^Jx', '', '   ']:
                    for h,v in [(h,0) for h in range(6)]+[(h,v) for v in [1,2,3] for h in range(3)]:
                        n=len(cases)
                        e=doc.modelspace().add_text(content,dxfattribs=attributes(content,n,h,v))
                        expected=paths(e,font)
                        cases.append(dict(font=name,text=spec(e),paths=expected))
                        doc.modelspace().delete_entity(e)
            doc=ezdxf.new('R2018'); doc.styles.get('Standard').dxf.font='SleufBaseFixture.ttf'
            doc.styles.new('Stroke',dxfattribs=dict(font='curves.shx'))
            doc.styles.new('Libre',dxfattribs=dict(font='curves.lff'))
            doc.layers.new('Root',dxfattribs=dict(true_color=0x317ABC))
            doc.layers.new('Off',dxfattribs=dict(color=-3))
            block=doc.blocks.new('Glyphs',base_point=(2,-3,1))
            for n,style in enumerate(['Standard','Stroke','Libre']):
                a=attributes('Axp BC',n,h=n,v=2);a.update(layer='0',color=0,style=style)
                block.add_text('Axp BC',dxfattribs=a)
            nested=doc.blocks.new('Nested',base_point=(1,2,-1))
            nested.add_blockref('Glyphs',(10,20,3),dxfattribs=dict(xscale=-2,yscale=.3,zscale=1.2,rotation=-17,layer='0',color=0))
            msp=doc.modelspace()
            for n,layer in enumerate(['Root','Off']):
                msp.add_blockref('Nested',(168000+10*n,401000,5),dxfattribs=dict(xscale=.7,yscale=1.7,zscale=.6,rotation=37,extrusion=(1,2,3),layer=layer,color=256,row_count=2,column_count=2,row_spacing=-7,column_spacing=5))
            msp.add_text('Axp MØ',dxfattribs=dict(insert=(168000,401000,0),height=2.5))
            expected=[]
            def walk(entities,matrix=Matrix44(),layer='0',blockpath=()):
                for e in entities:
                    effective=e.dxf.layer if e.dxf.layer!='0' else layer
                    if e.dxftype()=='INSERT':
                        cols=e.dxf.column_count
                        for i,copy in enumerate(e.multi_insert()):
                            chain=blockpath+(f'{e.dxf.handle}:{e.dxf.name}[{i//cols},{i%cols}]',)
                            walk(copy.block(),copy.matrix44()@matrix,effective,chain)
                    elif e.dxftype()=='TEXT':
                        font=fonts.make_font(e.font_name(),1.)
                        ink=paths(e,font,matrix)
                        expected.append(dict(handle=e.dxf.handle,blockpath=' / '.join(blockpath),layer=effective,
                                             visible=effective!='Off',paths=ink))
            walk(msp)
            doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
            doc.saveas(OUT/'text-ascii.dxf',fmt='asc');doc.saveas(OUT/'text-binary.dxf',fmt='bin')
    data=dict(files=FILES,cases=cases,scene=expected)
    (OUT/'reference.json').write_text(json.dumps(data,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} independent vector TEXT cases / {len(expected)} nested scene instances')


if __name__=='__main__':main()
