"""Production page numbers and overview viewport fits, without app startup."""
from pathlib import Path
from types import SimpleNamespace
import importlib
import json
import tempfile
import ezdxf
from PIL import Image
from generate_object_export_fixtures import reference
from generate_template_geometry_fixtures import signature as geometry_signature
from generate_template_image_fixtures import signature as image_signature
from datetime import datetime

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'rust/tests/fixtures/template-layout'
FIELDS=['center','width','height','view_center_point','view_direction_vector',
        'view_target_point','view_twist_angle','view_height','status','id','flags']

def snapshot(doc):
    return {'viewports':{e.dxf.handle:{n:list(v) if hasattr(v,'x') else v
        for n in FIELDS if (v:=e.dxf.get(n)) is not None}
        for e in doc.entitydb.values() if e.dxftype()=='VIEWPORT'},
        'attributes':{e.dxf.handle:e.dxf.text for e in doc.entitydb.values() if e.dxftype()=='ATTRIB'}}

def main():
    assert ezdxf.__version__=='1.4.4'
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export')
    Bounds=importlib.import_module(ref.__package__+'.models').Bounds
    exporter=module.CadastralDxfExporter(wfs_client=None)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    OUT.mkdir(parents=True,exist_ok=True)
    doc=ezdxf.new('R2018');doc.layers.add('KADER')
    block=doc.blocks.new('Title')
    for t in ['BLAD','BLADEN','SCHAAL','DATUM1','PROJECTNAAM']:block.add_attdef(t,(0,0),text='definition')
    for i in range(1,6):
        layout=doc.layouts.new(f'bLaD{i}')
        if i!=4:
            insert=layout.add_blockref('Title',(0,0));insert.add_auto_attribs({t:'old '+t for t in ['BLAD','BLADEN','SCHAAL','DATUM1','PROJECTNAAM']})
            other=layout.add_blockref('Title',(100,0));other.add_auto_attribs({'BLAD':'second stays','SCHAAL':'second stays'})
        if i==5:continue
        for center,size,layer in [((100,200),(170,110),'KADER'),((100,20),(80,40),'KADER'),((200,20),(90,60),'KADER'),((0,-100),(90,80),'0')]:
            viewport=layout.add_viewport(center,size,view_center_point=(7,8),view_height=90,status=2,dxfattribs={'layer':layer})
            viewport.dxf.view_direction_vector=(1,2,3);viewport.dxf.view_twist_angle=25
            viewport.dxf.view_target_point=(20,30,40)
            viewport.set_xdata('ACAD',[(1000,'preserve viewport metadata')])
    # Default optional view tags exercise native insertion and repeat editing.
    plain=doc.layouts.get('bLaD3').add_viewport((50,-20),(60,40),view_center_point=(0,0),view_height=40,dxfattribs={'layer':'KADER'})
    for n in ['view_target_point','view_direction_vector','view_twist_angle']:plain.dxf.discard(n)
    for name in ['Straße','ος','ı','K','K']:
        lay=doc.layouts.new(name);insert=lay.add_blockref('Title',(0,0));insert.add_auto_attribs({'DATUM1':'old date','PROJECTNAAM':'old project'})
    source=OUT/'sheets.dxf';doc.saveas(source)
    def bounds(x,y,w,h):return dict(min_x=x,min_y=y,max_x=x+w,max_y=y+h)
    def step(slots,gap=18):return dict(slots=slots,label_gap=gap)
    small=bounds(120000,450000,4,8);large=bounds(120000,450000,1600,1400)
    steps=[step([small]),step([small,None,large]+[None]*6),step([None]*16+[small,large]),
           step([small]*40),step([None]*40),step([small]*8+[large],-20),step([],0)]
    # Exact denominator thresholds, overflow-to-last-scale, and zero-size inputs.
    for width in [0,16,16.00001,36,76,156,396,796,1796,1e6]:
        steps.append(step([bounds(100,200,width,0)],-20))
    cases=[dict(name='synthetic',path=source,steps=steps),dict(name='shipped',path=ROOT/'assets/cadastral_template.dxf',steps=[step([small]*9),step([None]*8+[large]*8+[small]*16),step([None]*32)])]
    for case in cases:
        doc=ezdxf.readfile(case['path'])
        for s in case['steps']:
            names=exporter._used_template_layout_names(len(s['slots']))
            layers=[None if b is None else SimpleNamespace(bounds=Bounds(**b)) for b in s['slots']]
            exporter._update_template_layout_page_numbers(doc,names)
            exporter._configure_template_wireframe_viewports(doc,layers,names,s['label_gap'])
            s['expected']=snapshot(doc)
            pages=[]
            for i,name in enumerate(names,1):
                scale=None
                try:layout=doc.layouts.get(name)
                except KeyError:layout=None
                if layout is not None:
                    v=exporter._find_template_wireframe_viewport(layout);t=exporter._find_template_layout_insert(layout)
                    if v is not None and t is not None and any(layers[(i-1)*8:i*8]):
                        scale=round(v.dxf.view_height*1000/v.dxf.height)
                pages.append(dict(layout=name,number=i,scale=scale))
            s['pages']=pages
        case['path']=case['path'].relative_to(ROOT).as_posix()
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    doc=ezdxf.readfile(source)
    aliases=['STRASSE','οσ','I','K','K'];expected={}
    metadata=importlib.import_module(ref.__package__+'.template_metadata')
    metadata.update_title_blocks(doc,aliases,project='Unicode layouts',export_date=datetime(2026,10,19))
    exporter._ensure_template_image_layer(doc)
    with tempfile.TemporaryDirectory() as directory:
        image=Path(directory)/'unicode.png';Image.new('RGBA',(2,2),(12,34,56,255)).save(image)
        for i,name in enumerate(aliases):
            layout=doc.layouts.get(name)
            circle=layout.add_circle((i,2),1)
            exporter._add_box_image(doc,layout,image,Bounds(i,2,i+2,4),f'unicode_{i}',inset=0,preserve_aspect=False)
            expected[layout.name]=dict(circle=geometry_signature(circle),image=image_signature(doc,list(layout.query('IMAGE'))[-1]))
    (OUT/'unicode-reference.json').write_text(json.dumps(dict(aliases=aliases,expected=expected,attributes=snapshot(doc)['attributes']),ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'page sequences;',sum(len(c['steps']) for c in cases),'states')

if __name__=='__main__':main()
