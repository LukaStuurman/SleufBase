"""ATTDEF display/selection oracle from the original ezdxf drawing route.

No application initialization, user preferences, accounts or installed fonts.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import os
import shutil

import ezdxf
from ezdxf.addons.drawing.text import _split_into_lines
from ezdxf.entities import MText,Text
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from generate_cad_text_fixtures import paths
from generate_cad_attrib_fixtures import evaluate as attributes,FILES,ASSETS,rgb

OUT=Path(os.environ.get('SLEUFBASE_CAD_ATTDEF_FIXTURES',ASSETS/'cad-attdef'))
VERSIONS=['R12','R2000','R2007','R2018']


def build(version,shift=0):
    doc=ezdxf.new(version)
    doc.styles.get('Standard').dxf.font='SleufBaseFixture.ttf'
    doc.styles.new('Stroke',dxfattribs=dict(font='curves.shx'))
    doc.styles.new('Libre',dxfattribs=dict(font='curves.lff'))
    doc.layers.new('Root',dxfattribs=dict(true_color=0x317abc))
    doc.layers.new('Off',dxfattribs=dict(color=-3))
    model=doc.modelspace()
    alignments=[(h,0) for h in range(6)]+[(h,v) for v in [1,2,3] for h in range(3)]
    for style in ['Standard','Stroke','Libre']:
        for h,v in alignments:
            n=len(model);x=168000+shift+35*n;y=401000-shift
            model.add_attdef(f'A%%cx{n}',insert=(x,y,1),text=f'VALUE_{n}',dxfattribs=dict(
                prompt=f'Prompt {n}: waarde?',layer='Root',style=style,height=[.25,1.,2.5][n%3],width=.8,halign=h,valign=v,
                align_point=(x+13,y+4,-2),rotation=-23,oblique=17,field_length=12345,lock_position=1,
                text_generation_flag=[0,2,4,6][n%4],extrusion=[(0,0,1),(1,2,3),(0,0,-1)][n%3]))
    for name,params in [('FLAG',dict(flags=1)),('ENTITY',dict(invisible=1)),('LAYER',dict(layer='Off')),
                        ('CONSTANT',dict(flags=2|4|8)),('EMPTY',dict())]:
        model.add_attdef('' if name=='EMPTY' else name,insert=(168002+shift,401003-shift),text='VALUE_NOT_TAG',dxfattribs=dict(prompt='bewaar prompt',**params))
    if version=='R2018':
        a=model.add_attdef('MULTI',text='fallback',dxfattribs=dict(prompt='multiline prompt'))
        a.set_mtext(MText.new(dxfattribs=dict(text='first\\Psecond',char_height=2.5,width=20)))
    # Paper definitions do not become root labels.
    doc.layout().add_attdef('PAPER',insert=(1e6,1e6),text='ignored')
    leaf=doc.blocks.new('Leaf',base_point=(2,-3,1))
    leaf.add_line((0,0,0),(1,2,3),dxfattribs=dict(color=0))
    for flag in [0,1,2,2|4|8]:
        leaf.add_attdef(f'BLOCK_{flag}',insert=(1e6,1e6),text='BLOCK_DEFAULT',dxfattribs=dict(prompt='block prompt',flags=flag,height=1e50))
    for n in range(2):
        owner=model.add_blockref('Leaf',(168000+shift+5*n,401000-shift,0),dxfattribs=dict(layer='Root',
            rotation=37,xscale=-2.3,yscale=.3,zscale=1.7,extrusion=(1,2,3),row_count=2,column_count=3,row_spacing=-7,column_spacing=5))
        owner.add_attrib('BLOCK_0','VALUE_VISIBLE',insert=(168000+shift,401000-shift),dxfattribs=dict(height=2.5,color=0))
    nested=doc.blocks.new('Nested')
    nested.add_blockref('Leaf',(30,40,5),dxfattribs=dict(rotation=-17,xscale=.7,yscale=-2,zscale=.4))
    nested.add_attdef('NESTED_DEF',insert=(-1e6,-1e6),text='ignored',dxfattribs=dict(prompt='nested prompt'))
    model.add_blockref('Nested',(168000+shift,401000-shift),dxfattribs=dict(layer='Off',rotation=29,xscale=-1.5,yscale=.6,zscale=1.2))
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    return doc


def source(e):
    d=e.dxf
    return dict(kind=e.dxftype(),handle=d.handle,content=d.text,tag=d.tag,prompt=d.prompt,
                flags=d.flags,field_length=d.field_length,lock_position=bool(d.lock_position),
                multiline=e.has_embedded_mtext_entity)


def evaluate(doc,manager):
    expected=[];definitions=[]
    with patch.object(fonts,'font_manager',manager):
        for e in doc.modelspace():
            if e.dxftype()=='ATTDEF':
                meta=source(e);definitions.append(meta)
                # text2path accepts TEXT/ATTRIB only. Keep every supported
                # TEXT field: the old conversion helper erroneously forwards
                # R2018 attribute_type (not a valid TEXT attribute).
                copy=Text.new(doc=e.doc)
                copy.update_dxf_attribs({k:v for k,v in e.dxfattribs().items() if copy.is_supported_dxf_attrib(k)})
                # Use the actual original frontend's tag selection, then its
                # native-outline geometry oracle (not rectangle bounds).
                copy.dxf.text=_split_into_lines(e,None,lambda text:0)[0]
                ink=[] if e.has_embedded_mtext_entity else paths(copy,fonts.make_font(e.font_name(),1.))
                layer=doc.layers.get(e.dxf.layer)
                shown=not e.is_invisible and not e.dxf.invisible and not layer.is_off() and not layer.is_frozen()
                expected.append(dict(**meta,blockpath='',layer=e.dxf.layer,rgb=rgb(doc,e,e.dxf.layer,None),visible=bool(shown),paths=ink))
            elif e.dxftype()=='INSERT':
                # The original block expansion excludes even constant ATTDEFs.
                assert not any(v.dxftype()=='ATTDEF' for v in e.virtual_entities())
        for block in doc.blocks:
            if block.name.lower().startswith(('*model_space','*paper_space','$model_space','$paper_space')):continue
            definitions.extend(source(e) for e in block if e.dxftype()=='ATTDEF')
        expected.extend(attributes(doc,manager)['scene'])
    return dict(scene=expected,definitions=definitions)


def main():
    assert ezdxf.__version__=='1.4.4'
    ezdxf.options.write_fixed_meta_data_for_testing=True
    OUT.mkdir(parents=True,exist_ok=True);cases=[]
    with TemporaryDirectory(prefix='SleufBase-attdef-reference-') as temp:
        root=Path(temp)
        for file in FILES:shutil.copyfile(ASSETS/file,root/Path(file).name)
        manager=FontManager();manager.build([str(root)],support_dirs=False);fonts.TrueTypeFont._glyph_caches.clear()
        for version in VERSIONS:
            doc=build(version)
            for fmt in ['asc','bin']:
                name=f'{version}-{fmt}.dxf';path=OUT/name;doc.saveas(path,fmt=fmt)
                cases.append(dict(file=name,**evaluate(ezdxf.readfile(path),manager)))
    (OUT/'reference.json').write_text(json.dumps(dict(files=FILES,cases=cases),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} ATTDEF drawings / {sum(len(c["definitions"]) for c in cases)} source definitions / {sum(len(c["scene"]) for c in cases)} scene nodes')


if __name__=='__main__':main()
