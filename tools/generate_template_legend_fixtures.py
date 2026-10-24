"""Original legend regeneration, including affine ATTDEF metrics and real sheets."""
from pathlib import Path
import os,importlib,io,json
import ezdxf
from ezdxf.entities import factory
from datetime import datetime
from generate_object_export_fixtures import reference
from generate_template_page_fixtures import snapshot
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_LEGEND_FIXTURES',ROOT/'rust/tests/fixtures/template-legends'))
def legend_snapshot(doc,exporter):
    result=snapshot(doc,exporter)
    def close(a,b):
        if isinstance(a,(int,float)) and isinstance(b,(int,float)):return abs(a-b)<=1e-12
        if isinstance(a,(list,tuple)) and isinstance(b,(list,tuple)):return len(a)==len(b) and all(close(x,y) for x,y in zip(a,b))
        return a==b
    def canonical(node):
        if isinstance(node,list):
            for v in node:canonical(v)
        elif isinstance(node,dict):
            if 'attrs' in node:
                kind=node.get('kind','LAYOUT');schema=factory.cls(kind).DXFATTRIBS
                for key in list(node['attrs']):
                    if key in ('rotation','oblique'):node['attrs'][key]=(node['attrs'][key]+180.)%360.-180.
                    definition=schema.get(key)
                    if definition is not None and close(node['attrs'][key],definition.default):del node['attrs'][key]
            for v in node.values():canonical(v)
    canonical(result);return result

def main():
    assert ezdxf.__version__=='1.4.4'
    ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    metadata=importlib.import_module(ref.__package__+'.template_metadata')
    ezdxf.options.write_fixed_meta_data_for_testing=True;OUT.mkdir(parents=True,exist_ok=True)
    cases=[]
    def add(name,path,names,required=None):
        doc=ezdxf.readfile(path)
        if required is not None:exporter._extend_template_for_slot_count(doc,required)
        exporter._ensure_template_layout_legends(doc,names)
        metadata.update_title_blocks(doc,names,project='Legenda Ø 漢',export_date=datetime(2026,10,19))
        stream=io.StringIO();doc.write(stream);stream.seek(0);doc=ezdxf.read(stream)
        cases.append(dict(name=name,path=('assets/cadastral_template.dxf' if name=='shipped' else 'rust/tests/fixtures/template-legends/'+path.name),names=names,required=required,expected=legend_snapshot(doc,exporter)))
    add('shipped',ROOT/'assets/cadastral_template.dxf',[f'Blad{i}' for i in range(1,7)],48)
    # Use a real slot-bearing source so this can subsequently exercise the
    # composed native preparation stage, too.
    source=ROOT/'rust/tests/fixtures/template-pages/one-page.dxf'
    for i,(scales,rotation) in enumerate([((1,1,1),0),((2,2,3),35),((2,.5,3),-145),((-1,2,.5),90),((1,-2,3),-35),((-2,-.5,-3),179),((.001,.007,2),270)]):
        doc=ezdxf.readfile(source);doc.layers.new('X-XX-AL-LEGENDA');layout=doc.layouts.get('bLaD1')
        for e in list(layout):layout.delete_entity(e)
        block=doc.blocks.new('Legenda Ø',base_point=(5,7,2));block.add_circle((5,7),1)
        normals=[(0,0,1),(0,0,-1),(.3,.4,1),(1e-4,.5,1)]
        tags=['DATUM1','PROJECTNAAM']+[f'TAG_{j}' for j in range(14)]
        for j,tag in enumerate(tags):
            block.add_attdef(tag,(6+j*.7,8-j*.3,3+j*.1),text='default '+tag,dxfattribs=dict(height=.5+j*.1,rotation=[-145,-90,0,35,90,180,270][j%7],oblique=[0,15,-25][j%3],width=.7+j*.02,extrusion=normals[j%4],flags=j%4,halign=j%3,valign=j%4,align_point=(8+j*.4,9-j*.2,2),lock_position=j%2,field_length=j))
        ins=layout.add_blockref(block.name,(20,30,3),dxfattribs=dict(layer='X-XX-AL-LEGENDA',xscale=scales[0],yscale=scales[1],zscale=scales[2],rotation=rotation,color=5))
        # APPDATA may contain subclass-like tags; they are not text geometry.
        next(iter(block.query('ATTDEF'))).set_app_data('SHADOW',[(100,'AcDbText'),(10,(1e6,2e6,3e6))])
        ins.add_auto_attribs({tag:'actual '+tag for tag in tags[:-2]})
        # A moved source attribute must be regenerated from ATTDEF, not copied
        # from its dragged geometry. Values still follow exact-case tag lookup.
        ins.attribs[3].dxf.insert=(500,600,700)
        ins.add_attrib('TAG_0','last duplicate value',(1,2))
        layout.add_line((1,2),(4,6));layout.add_text('Legend suffix Ø',dxfattribs={'insert':(5,6),'height':1})
        definition=doc.add_image_def('legend.png',(3,2),name='LEGEND_IMAGE');layout.add_image(definition,(7,8),(4,3))
        for n in ['Blad2','Blad3','Blad4']:doc.layouts.new(n)
        # Any existing INSERT suppresses legend restoration on that layout.
        doc.layouts.get('Blad4').add_blockref(block.name,(7,9))
        doc.layouts.set_active_layout('Blad2')
        doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
        p=OUT/f'affine-{i}.dxf';doc.saveas(p);p.write_bytes(p.read_bytes().replace(b'\r\n',b'\n'))
        add(f'affine-{i}',p,['BLAD1','Blad2','BLAD2','Blad3','Blad4','missing'])
    doc=ezdxf.readfile(OUT/'affine-2.dxf');doc.dxfversion='AC1021'
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    p=OUT/'legacy-2007.dxf';doc.saveas(p);p.write_bytes(p.read_bytes().replace(b'\r\n',b'\n'))
    add('legacy-2007',p,['Blad1','Blad2','Blad3'])
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'original legend/affine attribute scenarios')
if __name__=='__main__':main()
