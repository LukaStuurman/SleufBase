"""Independent attached-ATTRIB contours and explicit MINSERT correction.

The original multi_insert() moves insert but leaves align_point unchanged.
Keep that evidence separately; correct copies translate both OCS anchors.
No application/settings/accounts or installed fonts are read.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import os
import shutil
import sys

import ezdxf
from ezdxf import colors
from ezdxf.entities import MText
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from ezdxf.math import Matrix44, Vec3
from generate_cad_text_fixtures import paths

ROOT=Path(__file__).resolve().parents[1]
ASSETS=ROOT/'rust/tests/fixtures'
OUT=Path(os.environ.get('SLEUFBASE_CAD_ATTRIB_FIXTURES',ASSETS/'cad-attrib'))
FILES=['dxf-text-metrics/SleufBaseFixture.ttf','stroke-font/curves.shx','lff-font/curves.lff']


def equivalent(a,b):
    # Ignore arithmetic-order noise when deciding whether legacy behavior is
    # materially wrong; retain every coordinate in both versions as evidence.
    if isinstance(b,(int,float)):
        return abs(a-b)<=1e-10+64*sys.float_info.epsilon*max(abs(a),abs(b))
    if isinstance(b,list):return len(a)==len(b) and all(equivalent(x,y) for x,y in zip(a,b))
    if isinstance(b,dict):return a.keys()==b.keys() and all(equivalent(a[k],b[k]) for k in b)
    return a==b


def rgb(doc,e,layer,parent):
    if e.dxf.hasattr('true_color'):return list(colors.int2rgb(e.dxf.true_color))
    if e.dxf.color==0 and parent is not None:return parent
    style=doc.layers.get(layer)
    if e.dxf.color in [0,256] and style.dxf.hasattr('true_color'):return list(colors.int2rgb(style.dxf.true_color))
    color=abs(style.dxf.color) if e.dxf.color in [0,256] else e.dxf.color
    value=list(colors.aci2rgb(max(1,min(255,color))))
    return [220,20,60] if value==[255,255,255] else value


def build():
    doc=ezdxf.new('R2018')
    doc.styles.get('Standard').dxf.font='SleufBaseFixture.ttf'
    doc.styles.new('Stroke',dxfattribs=dict(font='curves.shx'))
    doc.styles.new('Libre',dxfattribs=dict(font='curves.lff'))
    doc.layers.new('Root',dxfattribs=dict(true_color=0x317abc))
    doc.layers.new('Water',dxfattribs=dict(true_color=0x1283dd))
    doc.layers.new('Off',dxfattribs=dict(color=-3))
    leaf=doc.blocks.new('Leaf',base_point=(2,-3,1))
    # A real line lets the scene verify attribute-vs-block coordinates/order.
    leaf.add_line((0,0,0),(1,2,3),dxfattribs=dict(color=0))
    alignments=[(h,0) for h in range(6)]+[(h,v) for v in [1,2,3] for h in range(3)]
    for style in ['Standard','Stroke','Libre']:
        for h,v in alignments:
            n=len(doc.modelspace())
            owner=doc.modelspace().add_blockref('Leaf',(1000+100*n,2000,7),dxfattribs=dict(
                layer='Root',color=256,rotation=37,xscale=-2.3,yscale=.3,zscale=1.7,extrusion=(1,2,3),
                row_count=2,column_count=3,row_spacing=-7,column_spacing=5))
            owner.add_attrib(f'ALIGN_{h}_{v}', 'Axp BC',insert=(20+25*n,-3,1),dxfattribs=dict(
                style=style,align_point=(34+25*n,4,-2),height=[.25,1.,2.5][n%3],width=.8,
                rotation=-23,oblique=17,halign=h,valign=v,field_length=12345,lock_position=1,
                text_generation_flag=[0,2,4,6][n%4],extrusion=[(0,0,1),(1,2,3),(0,0,-1)][n%3],color=0))
    nested=doc.blocks.new('Nested',base_point=(1,2,-1))
    owner=nested.add_blockref('Leaf',(30,40,5),dxfattribs=dict(rotation=-17,xscale=.7,yscale=-2,zscale=.4,
        row_count=2,column_count=2,row_spacing=9,column_spacing=-8,color=0))
    owner.add_attrib('INNER','Axp',insert=(3,4,2),dxfattribs=dict(align_point=(15,-1,-2),halign=5,height=2.5,
        oblique=-11,extrusion=(1,2,3),color=0))
    owner.add_attrib('DUP','first',insert=(2,5,0),dxfattribs=dict(layer='water',color=256))
    owner.add_attrib('DUP','last',insert=(2,8,0),dxfattribs=dict(layer='water',color=256,flags=2|4|8))
    owner.add_attrib('HIDDEN_FLAG','never',insert=(1e6,1e6,0),dxfattribs=dict(flags=1))
    owner.add_attrib('HIDDEN_ENTITY','never',insert=(1e6,-1e6,0),dxfattribs=dict(invisible=1))
    owner.add_attrib('HIDDEN_LAYER','never',insert=(-1e6,1e6,0),dxfattribs=dict(layer='Off'))
    for n,layer in enumerate(['Root','Off']):
        owner=doc.modelspace().add_blockref('Nested',(168000+10*n,401000,5),dxfattribs=dict(layer=layer,
            xscale=-1.5,yscale=.6,zscale=1.2,rotation=29,extrusion=(.3,-.4,.7),
            row_count=2,column_count=2,row_spacing=11,column_spacing=-13))
        owner.add_attrib('OUTER','Axp',insert=(5,7,1),dxfattribs=dict(align_point=(17,4,2),halign=1,valign=3,height=2.5,color=0))
    # R2018 embedded MTEXT has repeated text codes and type 71=2. Keep it
    # verbatim and report unsupported layout; never mirror/draw its fallback.
    owner=doc.modelspace().add_blockref('Leaf',(50,60,0))
    attr=owner.add_attrib('MULTI','fallback')
    mtext=MText.new(dxfattribs=dict(text='first\\Psecond',insert=(50,60,0),char_height=2.5,width=20))
    attr.set_mtext(mtext)
    return doc


def evaluate(doc,manager):
    expected=[];corrections=[];parents=[]
    with patch.object(fonts,'font_manager',manager):
        def walk(entities,matrix=Matrix44(),parent_layer='0',parent_rgb=None,visible=True,blockpath=()):
            for e in entities:
                layer=e.dxf.layer if e.dxf.layer!='0' else parent_layer
                canonical=next(name for name in doc.layers.entries if name.lower()==layer.lower())
                layer=doc.layers.get(canonical).dxf.name
                col=rgb(doc,e,layer,parent_rgb);style=doc.layers.get(layer)
                shown=visible and not e.dxf.invisible and not style.is_off() and not style.is_frozen()
                if e.dxftype()=='INSERT':
                    parents.append(dict(handle=e.dxf.handle,attributes={a.dxf.tag:a.dxf.text for a in e.attribs},count=len(e.attribs)))
                    cols=e.dxf.column_count if e.dxf.column_spacing else 1
                    for i,copy in enumerate(e.multi_insert()):
                        chain=blockpath+(f'{e.dxf.handle}:{e.dxf.name}[{i//cols},{i%cols}]',)
                        walk(copy.block(),copy.matrix44()@matrix,layer,col,shown,chain)
                        offset=Vec3((i%cols)*e.dxf.column_spacing,(i//cols)*e.dxf.row_spacing).rotate_deg(e.dxf.rotation)
                        for a,legacy in zip(e.attribs,copy.attribs):
                            adjusted=a.copy();adjusted.dxf.insert+=offset
                            if a.dxf.hasattr('align_point'):adjusted.dxf.align_point+=offset
                            attr_layer=a.dxf.layer if a.dxf.layer!='0' else layer
                            attr_layer=doc.layers.get(attr_layer).dxf.name
                            attr_shown=shown and not a.dxf.invisible and not a.is_invisible and not doc.layers.get(attr_layer).is_off()
                            f=fonts.make_font(a.font_name(),1.)
                            want=[] if a.has_embedded_mtext_entity else paths(adjusted,f,matrix)
                            old=[] if a.has_embedded_mtext_entity else paths(legacy,f,matrix)
                            item=dict(kind='ATTRIB',handle=a.dxf.handle,blockpath=' / '.join(chain),layer=attr_layer,
                                rgb=rgb(doc,a,attr_layer,col),visible=bool(attr_shown),tag=a.dxf.tag,content=a.dxf.text,
                                flags=a.dxf.flags,field_length=a.dxf.field_length,lock_position=bool(a.dxf.lock_position),paths=want)
                            expected.append(item)
                            if not equivalent(old,want):corrections.append(dict(handle=a.dxf.handle,blockpath=item['blockpath'],legacy_paths=old,expected_paths=want))
                elif e.dxftype()=='LINE':
                    expected.append(dict(kind='LINE',handle=e.dxf.handle,blockpath=' / '.join(blockpath),layer=layer,rgb=col,visible=bool(shown),
                        points=[list(matrix.transform(e.dxf.start)),list(matrix.transform(e.dxf.end))]))
        walk(doc.modelspace())
    # Parent metadata is source-shared, so records are unique.
    parents=list({p['handle']:p for p in parents}.values())
    return dict(parents=parents,scene=expected,grid_corrections=corrections)


def main():
    assert ezdxf.__version__=='1.4.4'
    OUT.mkdir(parents=True,exist_ok=True);ezdxf.options.write_fixed_meta_data_for_testing=True
    doc=build();doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/'attributes-ascii.dxf',fmt='asc');doc.saveas(OUT/'attributes-binary.dxf',fmt='bin')
    # Reload the serialized DXF so group ordering/defaults are part of the oracle.
    doc=ezdxf.readfile(OUT/'attributes-ascii.dxf');expected=[];corrections=[];parents=[]
    with TemporaryDirectory(prefix='SleufBase-original-attributes-') as temp:
        directory=Path(temp)
        for file in FILES:shutil.copyfile(ASSETS/file,directory/Path(file).name)
        manager=FontManager();manager.build([str(directory)],support_dirs=False);fonts.TrueTypeFont._glyph_caches.clear()
        result=evaluate(doc,manager)
    (OUT/'reference.json').write_text(json.dumps(dict(files=FILES,**result),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    expected=result['scene'];corrections=result['grid_corrections']
    print(f'{len(expected)} scene entities / {sum(e["kind"]=="ATTRIB" for e in expected)} attached attributes / {len(corrections)} explicit MINSERT corrections')


if __name__=='__main__':main()
