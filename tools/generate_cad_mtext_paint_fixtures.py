"""Original MTEXT callbacks + SleufBase's actual palette, without app startup.

Only the isolated font-selection corrections from the existing oracle and the
already accepted true-black/layer-true-color/BYBLOCK fixes are applied. The
original complex renderer decides every word, decoration and fraction color.
"""
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import ast, json, os
from generate_mtext_render_fixtures import (
    ASSETS, ROOT, Oracle, Backend, Properties, fonts, ezdxf, manager_for,
    fixed_face, original, context,
)
from generate_cad_mtext_fixtures import ink
from ezdxf import colors
from ezdxf.math import Matrix44

OUT=Path(os.environ.get('SLEUFBASE_CAD_MTEXT_PAINT_FIXTURES',ASSETS/'cad-mtext-paint'))
FONTS=ASSETS/'mtext-render'
SHIFT=0.

# Execute this single pure function, never the package or its settings imports.
_tree=ast.parse((ROOT/'dxf.py').read_text(encoding='utf-8'))
_fn=next(n for n in _tree.body if isinstance(n,ast.FunctionDef) and n.name=='_resolve_entity_color')
_scope={'colors':colors}
exec(compile(ast.Module(body=[_fn],type_ignores=[]),'dxf.py::_resolve_entity_color','exec'),_scope)
_palette=_scope['_resolve_entity_color']

class Dxf(SimpleNamespace):
    def hasattr(self,name):return hasattr(self,name)

def rgb(aci,true_color,layer,doc,parent=None):
    if true_color is not None:return [(true_color>>16)&255,(true_color>>8)&255,true_color&255]
    if aci==0 and parent is not None:return parent
    if aci in (0,256) and doc.layers.has_entry(layer):
        true_color=doc.layers.get(layer).dxf.get('true_color')
        if true_color is not None:return rgb(aci,true_color,layer,doc)
    return list(_palette(SimpleNamespace(dxf=Dxf(color=aci,layer=layer)),doc))

def hex_rgb(value):
    return '#'+''.join(f'{v:02x}' for v in value)

class ColorBackend(Backend):
    def draw_text(self,*args):
        before=len(self.draws);super().draw_text(*args)
        if len(self.draws)>before:self.draws[-1]['rgb']=list(colors.RGB.from_hex(args[2].color[:7]))
    def draw_line(self,*args):
        super().draw_line(*args)
        self.draws[-1]['rgb']=list(colors.RGB.from_hex(args[2].color[:7]))

def render(e,layer,parent,base,matrix):
    style=e.doc.styles.get(e.dxf.style)
    initial=original(None)
    initial.font_face=fonts.font_manager.get_font_face(style.dxf.font)
    initial.cap_height=e.dxf.char_height
    initial.width_factor=style.dxf.width;initial.oblique=style.dxf.oblique
    initial.aci=abs(e.dxf.color)
    value=e.dxf.get('true_color')
    initial.rgb=tuple(rgb(1,value,layer,e.doc)) if value is not None else None
    initial.paragraph=initial.paragraph._replace(align={0:1,1:3,2:2}[(e.dxf.attachment_point-1)%3])
    oracle=Oracle(dict(initial=context(initial)),True)
    oracle._backend=ColorBackend(True)
    oracle._properties=Properties();oracle._properties._fixture_id=0
    oracle._properties.color=hex_rgb(base)
    oracle.resolve_aci_color=lambda aci:hex_rgb(rgb(aci,None,layer,e.doc,parent))
    layout=oracle.layout_engine(e);layout.place(align=e.dxf.attachment_point)
    layout.render(e.ucs().matrix)
    paths=[]
    for d in oracle._backend.draws:
        path=d['contours'] if d['kind']=='text' else dict(kinds=[0,1],points=[d['start'],d['end']])
        paths.extend(dict(**p,rgb=d['rgb']) for p in ink([path],matrix))
    return paths

def main():
    OUT.mkdir(parents=True,exist_ok=True);ezdxf.options.write_fixed_meta_data_for_testing=True
    doc=ezdxf.new('R2018')
    doc.layers.new('Blue',dxfattribs=dict(color=5))
    doc.layers.new('RGB',dxfattribs=dict(color=2,true_color=0x3264c8))
    doc.layers.new('Black',dxfattribs=dict(color=3,true_color=0))
    doc.layers.new('Hidden',dxfattribs=dict(color=4));doc.layers.get('Hidden').off()
    names=['Regular.ttf','Italic.ttf','own.shp','own.shx','own.lff','curves.shx','curves.lff']
    for n,name in enumerate(names):doc.styles.new(f'P-{n}',dxfattribs=dict(font=name))
    texts=[r'A {\C1;x {\c0;p} M} A \C7;x \c16777215;p \C256;A \C0;x',
        r'\c197121;\LAxp\l \C5;\OAx\o \Kp\k \S1/2; \Sx#p; A',
        r'\C2;A\P{\C3;x\P\C4;p}\P A \c0;x\C6;p',
        r'A {\c16777215;x {\C7;p} A} \C0;x \C256;p',
        r'\C256;A \C0;x {\C130;p} A']
    def add(layout,n):
        props=dict(style=f'P-{n%7}',char_height=.65,width=30.,attachment_point=n%9+1,
            color=[0,256,1,7,25,130][n%6],layer=['0','Blue','RGB','Black','Hidden'][n%5],
            insert=(168000+n*2.25+SHIFT,401000-n*2.5-SHIFT,2.),rotation=n*13%360)
        value=[None,0,0xffffff,0x123456][n%4]
        if value is not None:props['true_color']=value
        if layout is not doc.modelspace():props['insert']=(n*.7+SHIFT,-n*.3-SHIFT,1.)
        layout.add_mtext(texts[n%len(texts)],dxfattribs=props)
    for n in range(28):add(doc.modelspace(),n)
    # All indexed colors, including BYBLOCK/BYLAYER, through real token callbacks.
    doc.modelspace().add_mtext(' '.join(f'\\C{n};A' for n in range(257)),dxfattribs=dict(
        style='P-0',layer='RGB',char_height=.65,width=300.,insert=(168010+SHIFT,400900-SHIFT,0)))
    block=doc.blocks.new('Paint',base_point=(2,-3,1))
    for n in range(7):add(block,n)
    inner=doc.blocks.new('Nested')
    inner.add_blockref('Paint',(10,20,3),dxfattribs=dict(color=0,xscale=-2,yscale=.3,zscale=1.2,rotation=-17))
    for layer,color,value in [('Blue',3,None),('RGB',0,0xabcdef),('Hidden',256,None),('Black',256,None)]:
        props=dict(layer=layer,color=color,xscale=.7,yscale=1.7,zscale=.6,rotation=37,
            row_count=2,column_count=2,row_spacing=-7,column_spacing=5)
        if value is not None:props['true_color']=value
        doc.modelspace().add_blockref('Nested',(168030+SHIFT,401020-SHIFT,5),dxfattribs=props)
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/'paint-ascii.dxf',fmt='asc');doc.saveas(OUT/'paint-binary.dxf',fmt='bin')
    doc=ezdxf.readfile(OUT/'paint-ascii.dxf')
    raw=fonts.find_font_file_name;scene=[]
    def walk(layout,matrix=Matrix44(),parent_layer=None,parent_rgb=None,visible=True):
        for e in layout:
            layer=parent_layer if e.dxf.layer=='0' and parent_layer else e.dxf.layer
            base=rgb(abs(e.dxf.color),e.dxf.get('true_color'),layer,doc,parent_rgb)
            show=visible and not doc.layers.get(layer).is_off() and not doc.layers.get(layer).is_frozen()
            if e.dxftype()=='INSERT':
                for copy in e.multi_insert():walk(copy.block(),copy.matrix44()@matrix,layer,base,show)
            elif e.dxftype()=='MTEXT':
                scene.append(dict(handle=e.dxf.handle,content=e.text,layer=layer,rgb=base,visible=show,
                    paths=render(e,layer,parent_rgb,base,matrix)))
    with patch.object(fonts,'font_manager',manager_for(FONTS)),patch.object(fonts,'find_font_file_name',lambda f:raw(fixed_face(f))):walk(doc.modelspace())
    result=dict(scene=scene)
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(scene)} original colored MTEXT instances / {sum(len(n["paths"]) for n in scene)} painted contours in ASCII/binary, all 257 ACI roles, true RGB, decorations and nested inherited grids')
    return result

if __name__=='__main__':main()
