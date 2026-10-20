"""Original geometry bounds and production selector location, offline."""
from pathlib import Path
from unittest.mock import patch
import importlib,json,os
import ezdxf
from ezdxf import bbox
from ezdxf.fonts import fonts
from fontTools.ttLib import TTFont
from generate_object_export_fixtures import reference

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_EXTENTS_FIXTURES',ROOT/'rust/tests/fixtures/template-extents'))

def main():
    assert ezdxf.__version__=='1.4.4';ezdxf.options.write_fixed_meta_data_for_testing=True
    OUT.mkdir(parents=True,exist_ok=True)
    ref=reference();pipeline=importlib.import_module(ref.__package__+'.dxf_template_pipeline_patch')
    doc=ezdxf.new('R2018');doc.styles.get('Standard').dxf.font=fonts.MONOSPACE
    doc.styles.new('FIXTURE',dxfattribs={'font':'SleufBaseFixture.ttf'})
    ms=doc.modelspace();groups=[]
    def add(name,entities):groups.append(dict(name=name,handles=[int(e.dxf.handle,16) for e in entities]))
    add('lines-3d',[ms.add_line((1,2,3),(9,-2,7)),ms.add_point((-4,10,12))])
    for i,normal in enumerate([(0,0,1),(0,0,-1),(1,2,3)]):
        for j,(start,end) in enumerate([(0,360),(0,0),(30,120),(355,5),(-90,50)]):
            add(f'arc-{i}-{j}',[ms.add_arc((1,2,3),2.5,start,end,dxfattribs={'extrusion':normal})])
        add(f'circle-{i}',[ms.add_circle((1,2,3),2.5,dxfattribs={'extrusion':normal})])
        add(f'polyline-{i}',[ms.add_lwpolyline([(1,2),(3,5),(-1,7)],dxfattribs={'extrusion':normal,'elevation':4})])
    add('polyline3d',[ms.add_polyline3d([(1,2,3),(4,6,-8),(-3,7,11)])])
    add('polyline2d',[ms.add_polyline2d([(1,2),(4,6),(-3,7)],dxfattribs={'elevation':(0,0,5),'extrusion':(1,2,3)})])
    add('leader',[ms.add_leader([(0,0,1),(1,5,3),(7,6,2)],dxfattribs={'has_arrowhead':0,'annotation_type':2,'has_hookline':0})])
    add('solid',[ms.add_solid([(0,0,1),(4,0,1),(0,4,1),(4,4,1)],dxfattribs={'extrusion':(1,2,3)})])
    add('face',[ms.add_3dface([(0,0,1),(4,0,2),(4,4,3),(0,4,4)])])
    block=doc.blocks.new('PROFILE',base_point=(3,-2,1))
    block.add_lwpolyline([(3,-2),(6,-2),(6,4)])
    block.add_circle((4,2,1),.75)
    block.add_text('Axp Ø',dxfattribs={'insert':(3,1,1),'height':.5,'style':'FIXTURE'})
    block.add_attdef('VALUE',(4,4,1),height=.5,dxfattribs={'halign':1,'valign':3,'align_point':(4,4,1)})
    # Uninstantiated ATTDEF is omitted by recursive_decompose/virtual INSERT.
    block.add_attdef('SKIPPED',(1000,1000),height=50)
    for i in range(12):
        scale=[.1,1.,2.5][i%3];scale=-scale if i%5==0 else scale
        e=ms.add_blockref('PROFILE',(10,20,3),dxfattribs={'xscale':scale,'yscale':scale,'zscale':abs(scale),'rotation':[0,23,90,-47][i%4],'extrusion':[(0,0,1),(0,0,-1),(1,2,3)][i%3]})
        e.add_auto_attribs({'VALUE':'Water Ø 110'})
        add(f'block-{i}',[e])
    image_def=doc.add_image_def(filename='fixture.png',size_in_pixel=(20,12))
    image=ms.add_image(image_def,insert=(100,200,3),size_in_units=(15,7),rotation=23)
    add('image',[image])
    clipped=ms.add_image(image_def,insert=(-20,30,1),size_in_units=(10,6),rotation=-17)
    clipped.set_boundary_path([(1,2),(15,1),(16,9),(3,8)])
    add('clipped-image',[clipped])
    add('images-only',[image,clipped])
    text=ms.add_text('Long profile label Axp MØ '*3,dxfattribs={'insert':(2,4,1),'height':2,'style':'FIXTURE'})
    add('profile-before-image',[image,text])
    add('empty-text-before-image',[image,ms.add_text('',dxfattribs={'insert':(0,0),'height':2})])
    font_path=ROOT/'rust/tests/fixtures/dxf-text-metrics/SleufBaseFixture.ttf'
    with patch.object(fonts.font_manager,'get_ttf_font',side_effect=lambda *a,**k:TTFont(font_path)):
        for group in groups:
            entities=[doc.entitydb[f'{h:X}'] for h in group['handles']]
            bounds=bbox.extents(entities,fast=True)
            group['bounds']=dict(min=list(bounds.extmin),max=list(bounds.extmax)) if bounds.has_data else None
            wrapper=doc.blocks.new('WRAPPER');content=doc.blocks.new('WRAPPER_NORMAAL_CONTENT')
            for e in entities:content.add_entity(e.copy())
            wrapper.add_blockref(content.name,(0,0))
            group['selector']=pipeline._selector_locations_from_document(doc,[wrapper.name]).get(wrapper.name)
            doc.blocks.delete_block(wrapper.name,safe=False);doc.blocks.delete_block(content.name,safe=False)
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/'input.dxf')
    (OUT/'reference.json').write_text(json.dumps(groups,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'{len(groups)} original geometry bounds and production selector locations')

if __name__=='__main__':main()
