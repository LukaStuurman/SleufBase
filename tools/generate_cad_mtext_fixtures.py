"""Original real DXF MTEXT source-to-contour oracle; no SleufBase initialization."""
from pathlib import Path
from unittest.mock import patch
import json,os
from generate_mtext_render_fixtures import fonts,ezdxf,manager_for,Oracle,context,original,fixed_face,ASSETS
from generate_mtext_simple_fixtures import evaluate as simple_evaluate
from ezdxf.addons.drawing.frontend import is_complex_mtext
from ezdxf.tools.text import estimate_mtext_extents
from ezdxf.math import Matrix44

OUT=Path(os.environ.get('SLEUFBASE_CAD_MTEXT_FIXTURES',ASSETS/'cad-mtext'))
FONTS=ASSETS/'mtext-render'
SHIFT=0.

def segments(path):
    result=[];at=0;current=None
    for kind in path['kinds']:
        count=max(1,kind);points=path['points'][at:at+count];at+=count
        if kind:result.append(dict(kind=kind,start=current,points=points))
        current=points[-1]
    assert at==len(path['points']);return result

def ink(paths,matrix=Matrix44()):
    result=[]
    for path in paths:
        current=None;at=0
        for kind in path['kinds']:
            n=max(1,kind);points=[list(matrix.transform(p)) for p in path['points'][at:at+n]];at+=n
            if kind==0:
                if current and current['commands']:result.append(current)
                current=dict(start=points[0],commands=[])
            else:current['commands'].append(dict(kind=kind,points=points))
        if current and current['commands']:result.append(current)
    return result

def original_paths(e):
    style=e.doc.styles.get(e.dxf.style);face=fonts.font_manager.get_font_face(style.dxf.font)
    r=dict(content=e.text,face=face._asdict(),height=e.dxf.char_height,width=e.dxf.width,spacing=e.dxf.line_spacing_factor,
        align=e.dxf.attachment_point,insert=list(e.dxf.insert),rotation=e.dxf.rotation,normal=list(e.dxf.extrusion),
        direction=list(e.dxf.text_direction) if e.dxf.hasattr('text_direction') else None)
    complex_=is_complex_mtext(e) or r'\~' in e.text
    if not complex_:
        found=simple_evaluate(r,FONTS);assert 'output' in found,found
        return [line['contours'] for line in found['output']['lines']],False,None
    initial=original(None);initial.font_face=face;initial.cap_height=e.dxf.char_height if e.dxf.char_height>1e-9 else 2.5
    initial.width_factor=style.dxf.width;initial.oblique=style.dxf.oblique
    initial.paragraph=initial.paragraph._replace(align={0:1,1:3,2:2}[(e.dxf.attachment_point-1)%3])
    req=dict(initial=context(initial));oracle=Oracle(req,True)
    layout=oracle.layout_engine(e);layout.place(align=e.dxf.attachment_point);layout.render(e.ucs().matrix)
    paths=[]
    for d in oracle._backend.draws:
        if d['kind']=='text':paths.append(d['contours'])
        else:paths.append(dict(kinds=[0,1],points=[d['start'],d['end']]))
    correction=not is_complex_mtext(e)
    legacy=[line['contours'] for line in simple_evaluate(r,FONTS)['output']['lines']] if correction else None
    return paths,correction,legacy

def main():
    ezdxf.options.write_fixed_meta_data_for_testing=True;OUT.mkdir(parents=True,exist_ok=True)
    m=manager_for(FONTS);raw=fonts.find_font_file_name
    with patch.object(fonts,'font_manager',m),patch.object(fonts,'find_font_file_name',lambda f:raw(fixed_face(f))):
        doc=ezdxf.new('R2018');names=['Regular.ttf','Italic.ttf','own.shp','own.shx','own.lff','curves.shx','curves.lff']
        styles=[]
        for k in range(3):
            for n,name in enumerate(names):
                label=f'S-{n}-{k}';doc.styles.new(label,dxfattribs=dict(font=name,width=[1.,.8,1.][k],oblique=[0,0,13][k]));styles.append(label)
        texts=['Axp MØ',r'A\Px\P\Pp',' A  x^Ip ',r'\Lunder\l \S1/2;',r'\fFixture Styled|b1|i0;Axp',r'A\~x P',r'\H.6x;A \Q17;x','',' ']
        def add(layout,n):
            a=dict(style=styles[n%len(styles)],char_height=[.25,1.,2.5][n%3],width=[0.,11.,31.][n%3],
                line_spacing_factor=[.7,1.,1.37][n%3],attachment_point=n%9+1,
                insert=(168000.125+n*.37+SHIFT,401000.375-n*.23-SHIFT,2.),extrusion=(0,0,-1 if n%5==0 else 1))
            if n%4:a['rotation']=[0.,27.,-90.,0.][n%4]
            if n%7==0:a['text_direction']=(0,-1,0)
            if layout is not doc.modelspace():a['insert']=(3+n*.1+SHIFT,-2+n*.2-SHIFT,1.)
            layout.add_mtext(texts[n%len(texts)],dxfattribs=a)
        for n in range(126):add(doc.modelspace(),n)
        block=doc.blocks.new('MText',base_point=(2,-3,1))
        for n in [2,3,4]:add(block,n+126)
        inner=doc.blocks.new('Nested');inner.add_blockref('MText',(10,20,3),dxfattribs=dict(xscale=-2,yscale=.3,zscale=1.2,rotation=-17))
        doc.modelspace().add_blockref('Nested',(168000,401000,5),dxfattribs=dict(xscale=.7,yscale=1.7,zscale=.6,rotation=37,row_count=2,column_count=2,row_spacing=-7,column_spacing=5))
        doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
        doc.saveas(OUT/'mtext-ascii.dxf',fmt='asc');doc.saveas(OUT/'mtext-binary.dxf',fmt='bin')
        # Evaluate the actual reloaded file. Saving may replace a rotation by a
        # WCS direction; the frontend's simple get_rotation observes that change.
        doc=ezdxf.readfile(OUT/'mtext-ascii.dxf');expected=[]
        for layout in [doc.modelspace(),doc.blocks.get('MText')]:
            for e in layout.query('MTEXT'):
                paths,correction,legacy=original_paths(e)
                expected.append(dict(handle=e.dxf.handle,content=e.text,style=e.dxf.style,raw_paths=paths,
                    segments=[s for p in paths for s in segments(p)],nbsp_correction=correction))
                if correction:expected[-1]['legacy_segments']=[s for p in legacy for s in segments(p)]
        by_handle={c['handle']:c for c in expected};scene=[]
        def walk(layout,matrix=Matrix44()):
            for e in layout:
                if e.dxftype()=='INSERT':
                    for copy in e.multi_insert():walk(copy.block(),copy.matrix44()@matrix)
                elif e.dxftype()=='MTEXT':
                    segs=by_handle[e.dxf.handle]['segments']
                    scene.append(dict(handle=e.dxf.handle,first=list(matrix.transform(segs[0]['start'])) if segs else None,
                        last=list(matrix.transform(segs[-1]['points'][-1])) if segs else None,paths=ink(by_handle[e.dxf.handle]['raw_paths'],matrix)))
        walk(doc.modelspace())
    for e in expected:del e['raw_paths']
    (OUT/'reference.json').write_text(json.dumps(dict(cases=expected,scene=scene),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(expected)} original source MTEXTs / {sum(len(c["segments"]) for c in expected)} vector segments / {sum(c["nbsp_correction"] for c in expected)} explicit nonbreaking-space routing corrections in ASCII/binary drawings with nested grids')

if __name__=='__main__':main()
