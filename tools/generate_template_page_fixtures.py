"""Original sheet extension and deep layout-copy references, offline."""
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
from datetime import datetime
import importlib
import io
import json
import os
import ezdxf
from ezdxf.lldxf.types import DXFTag
from generate_object_export_fixtures import reference

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_PAGE_FIXTURES',ROOT/'rust/tests/fixtures/template-pages'))

def value(v):
    if isinstance(v,bytes):return v.hex()
    if hasattr(v,'x'):return list(v)
    if isinstance(v,(list,tuple)):return [value(x) for x in v]
    return v

def dictionary(d):
    result={}
    for key,e in d.items():
        if e.dxftype() in ('DICTIONARY','ACDBDICTIONARYWDFLT'):result[key]=dictionary(e)
        elif e.dxftype()=='XRECORD':result[key]=[[t.code,value(t.value)] for t in e.tags]
        else:result[key]={k:value(v) for k,v in e.dxf.all_existing_dxf_attribs().items() if k not in ('handle','owner')}
    return result

def attributes(e,skip):
    # ezdxf omits explicit default tags on write. Native editing preserves them;
    # compare their semantic values instead of serialization presence.
    result={}
    for k,v in e.dxf.all_existing_dxf_attribs().items():
        if k in skip:continue
        definition=e.DXFATTRIBS.get(k)
        default=definition.default if definition is not None else None
        if value(v)==value(default):continue
        result[k]=value(v)
    return result

def entity(e,indices,doc):
    attrs=attributes(e,('handle','owner'))
    if 'clipping_boundary_handle' in attrs:attrs['clipping_boundary_handle']=indices.get(attrs['clipping_boundary_handle'],'shared:'+attrs['clipping_boundary_handle'])
    if 'image_def_handle' in attrs:attrs['image_def_handle']=doc.entitydb[attrs['image_def_handle']].dxf.filename
    if 'image_def_reactor_handle' in attrs:attrs['image_def_reactor_handle']='independent reactor'
    result=dict(kind=e.dxftype(),attrs=attrs)
    if e.has_extension_dict:result['dictionary']=dictionary(e.get_extension_dict().dictionary)
    if e.xdata:result['xdata']={k:[[t.code,value(t.value)] for t in tags] for k,tags in e.xdata.data.items()}
    if e.appdata:result['appdata']={k:[[t.code,value(t.value)] for t in tags] for k,tags in e.appdata.data.items()}
    if e.reactors:result['reactors']=list(e.reactors)
    if e.dxftype()=='INSERT':result['attributes']=[entity(a,indices,doc) for a in e.attribs]
    if e.dxftype()=='POLYLINE':result['vertices']=[entity(a,indices,doc) for a in e.vertices]
    if e.dxftype()=='LWPOLYLINE':result['vertices']=[list(p) for p in e.get_points()]
    if e.dxftype()=='MTEXT' and e.has_columns:
        result['columns']={k:value(getattr(e.columns,k)) for k in ('column_type','count','auto_height','reversed_column_flow','width','gutter_width','heights','total_width','total_height')}
    return result

def snapshot(doc,exporter):
    model=list(doc.modelspace());model_indices={e.dxf.handle:i for i,e in enumerate(model)}
    layouts=[]
    for layout in doc.layouts:
        if layout.name=='Model':continue
        entities=list(layout);indices={e.dxf.handle:i for i,e in enumerate(entities)}
        attrs=attributes(layout.dxf_layout,('handle','owner','block_record_handle','viewport_handle'))
        layouts.append(dict(name=layout.name,attrs=attrs,entities=[entity(e,indices,doc) for e in entities]))
    slots=[asdict(s) for s in exporter._detect_template_slots(doc)]
    for slot in slots:
        for name in ('label_handle','address_handle','comments_handle'):
            h=slot[name];slot[name]=None if h is None else model_indices[h]
    return dict(slots=slots,model=[entity(e,model_indices,doc) for e in model],layouts=layouts)

def main():
    assert ezdxf.__version__=='1.4.4'
    ezdxf.options.write_fixed_meta_data_for_testing=True
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export');exporter=module.CadastralDxfExporter(wfs_client=None)
    OUT.mkdir(parents=True,exist_ok=True)
    def source(name,pages=1,existing=False,missing=False):
        doc=ezdxf.new('R2018');doc.layers.add('KADER');doc.layers.add('Hulplijn kader');doc.appids.add('PAGE_TEST')
        m=doc.modelspace()
        def box(x,y,w,h,layer):m.add_lwpolyline([(x,y),(x+w,y),(x+w,y+h),(x,y+h)],close=True,dxfattribs={'layer':layer})
        for page in range(pages):
            for base_x in (0.,30.):
                for base_y in (0.,-3.,-6.,-9.):
                    x=base_x+page*2;y=base_y-page*40
                    box(x,y-.6,29,2.75,'KADER');box(x+24,y,2.1,1.6,'Hulplijn kader');box(x+26.5,y-.1,2.5,1.8,'Hulplijn kader')
                    m.add_text('Proefsleuf',dxfattribs={'insert':(x+26.3,y+1.8),'height':.1,'layer':'KADER'})
                    m.add_mtext('Adres\\PØ 漢',dxfattribs={'insert':(x+24.1,y+.7),'char_height':.1,'layer':'KADER'})
                    m.add_mtext('Opmerkingen',dxfattribs={'insert':(x+26.3,y-.3),'char_height':.1})
        extra=m.add_text('OCS witness',dxfattribs={'insert':(3,-2,4),'align_point':(5,-3,2),'height':.2,'extrusion':(.3,.4,1.)})
        extra.set_xdata('PAGE_TEST',[(1000,'preserve data'),(1011,(1,2,3)),(1012,(4,5,6))])
        extra.set_app_data('PAGE_APP',[(10,(1000000.,2000000.,0.))])
        extension=extra.new_extension_dict()
        extension.add_xrecord('PAGE_DATA').reset([DXFTag(1,'copy-owned-data'),DXFTag(90,17)])
        nested=extension.dictionary.add_new_dict('NESTED',hard_owned=True)
        nested.add_xrecord('DATA').reset([DXFTag(1,'independent nested data')])
        weak=extension.dictionary.add_new_dict('WEAK',hard_owned=False)
        weak.add_xrecord('DATA').reset([DXFTag(1,'retain data in copied soft-owned dictionary')])
        m.add_mtext('Rotated Ø',dxfattribs={'insert':(7,-3),'char_height':.2,'rotation':35,'extrusion':(.3,.4,1.)})
        witness=m.add_lwpolyline([(1,-4,.1,.2,.3),(3,-4,.2,.1,-.2),(5,-3,0,0,0)],dxfattribs={'elevation':3,'extrusion':(.3,.4,1.)})
        witness.set_app_data('PAGE_APP',[(10,(1000000.,2000000.,0.))])
        m.add_line((0,-3,2),(25,-3,5));m.add_circle((2,-3),1) # Circle is deliberately outside the original anchor types.
        for i in range(1,pages+1+int(existing)):
            if missing:continue
            layout=doc.layouts.new(f'bLaD{i}')
            layout.dxf.paper_width=297;layout.dxf.paper_height=210;layout.dxf.limmin=(2,3);layout.dxf.limmax=(20,30)
            layout.dxf.plot_origin_x_offset=12;layout.dxf.plot_origin_y_offset=9
            clip=layout.add_lwpolyline([(0,0),(10,0),(10,10),(0,10)],close=True)
            for y,height in [(20.,40.),(200.,110.)]:
                v=layout.add_viewport((100,y),(170,height),view_center_point=(7+i,8+i),view_height=90,status=2,dxfattribs={'layer':'KADER'})
                v.dxf.clipping_boundary_handle=clip.dxf.handle;v.dxf.flags|=65536
            b=doc.blocks.new('Title'+str(i));b.add_attdef('DATUM1',(1,2),text='definition');b.add_attdef('PROJECTNAAM',(2,3),text='definition')
            ins=layout.add_blockref(b.name,(20,30));ins.add_auto_attribs({'DATUM1':'old date','PROJECTNAAM':'old project'})
            ins.new_extension_dict().add_xrecord('TITLE').reset([DXFTag(1,'independent title data')])
            line=layout.add_line((0,0),(7,8));line.new_extension_dict().add_xrecord('LINE').reset([DXFTag(90,42)])
            layout.add_polyline2d([(1,2),(3,4),(5,6)])
            definition=doc.add_image_def('fixture.png',(2,2),name='PAGE_IMAGE'+str(i))
            image=layout.add_image(definition,(2,3),(4,5));image.new_extension_dict().add_xrecord('IMAGE_DATA').reset([DXFTag(1,'owned image metadata')])
        doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
        path=OUT/(name+'.dxf');doc.saveas(path);path.write_bytes(path.read_bytes().replace(b'\r\n',b'\n'));return path
    sources=[('one-page',source('one-page'),[8,9,17,33]),('two-pages',source('two-pages',pages=2),[24,40]),('existing-layout',source('existing-layout',existing=True),[9]),('missing-layout',source('missing-layout',missing=True),[9]),('shipped',ROOT/'assets/cadastral_template.dxf',[32,40,48,57])]
    cases=[]
    for name,path,requests in sources:
        doc=ezdxf.readfile(path);steps=[]
        for required in requests:
            before=len(exporter._detect_template_slots(doc))
            try:
                exporter._extend_template_for_slot_count(doc,required)
                assert len(exporter._detect_template_slots(doc))>=required
                names=exporter._used_template_layout_names(required)
                metadata=importlib.import_module(ref.__package__+'.template_metadata')
                metadata.update_title_blocks(doc,names,project='Uitbreiding Ø 漢',export_date=datetime(2026,10,19))
                Bounds=importlib.import_module(ref.__package__+'.models').Bounds
                layers=[SimpleNamespace(bounds=Bounds(120000+i*10,450000-i,120020+i*10,450020-i)) for i in range(required)]
                exporter._update_template_layout_page_numbers(doc,names)
                exporter._configure_template_wireframe_viewports(doc,layers,names,18.)
                # Readback normalizes the same persisted structures checked for Rust.
                stream=io.StringIO();doc.write(stream);stream.seek(0);doc=ezdxf.read(stream)
                if name=='one-page' and required==9:
                    proof=io.StringIO();doc.write(proof);proof.seek(0)
                    audit=ezdxf.read(proof).audit()
                    assert any(error.code==202 and 'DICTIONARY' in error.message for error in audit.fixes),'Original helper must reproduce invalid copied dictionary ownership'
                steps.append(dict(required=required,error=False,added_pages=(len(exporter._detect_template_slots(doc))-before)//8,expected=snapshot(doc,exporter)))
            except module.CadastralExportError:
                steps.append(dict(required=required,error=True));break
        cases.append(dict(name=name,path='assets/cadastral_template.dxf' if name=='shipped' else 'rust/tests/fixtures/template-pages/'+path.name,steps=steps))
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'sheet sequences;',sum(len(c['steps']) for c in cases),'extension states')

if __name__=='__main__':main()
