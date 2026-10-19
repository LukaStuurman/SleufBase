"""Original prepared-slot assembly, complete active profiles and durable PNGs.

External fetching/rendering is upstream of this contract. Calls the original
drawer and insertion helpers; no Rust output is used to generate expectations.
"""
from pathlib import Path
from types import SimpleNamespace
from datetime import datetime
import importlib,io,json,os
import ezdxf
from PIL import Image
from shapely.geometry import Polygon
from generate_object_export_fixtures import reference
from generate_template_setup_fixtures import snapshot as setup_snapshot
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_EXPORT_FIXTURES',ROOT/'rust/tests/fixtures/template-export'))

def snapshot(doc,exporter):
    result=setup_snapshot(doc,exporter)
    indices={e.dxf.handle:i for i,e in enumerate(doc.modelspace())}
    for e in result['model']:
        if e['kind']=='LEADER':
            e['attrs']['annotation_handle']=indices[e['attrs']['annotation_handle']]
        if e['kind']=='IMAGE':
            # New owned bundles have unpredictable absolute directories. Compare
            # their indexed basenames here and exact pixels/bytes separately.
            filename=e['attrs']['image_def_handle']
            if Path(filename).name.startswith('slot'):e['attrs']['image_def_handle']=Path(filename).name
    return result

def original_dataset(ref,data,forced_start):
    features=[]
    for f in data['features']:
        g=f['geometry'];args=(f['object_name'],f['source_name'])
        if 'Point' in g:
            p=g['Point'];feature=ref.KickTheMapObjectPoint(*args,p['x'],p['y'],p['z'],*f['attributes'])
        else:feature=ref.KickTheMapObjectPolyline(*args,tuple(ref.KickTheMapPolylineVertex(v['x'],v['y'],v['z']) for v in g['Polyline']),*f['attributes'])
        features.append(feature)
    return ref.KickTheMapObjectDataset(data['job_id'],data['title'],Path(data['source_path']),tuple(f for f in features if isinstance(f,ref.KickTheMapObjectPoint)),tuple(f for f in features if isinstance(f,ref.KickTheMapObjectPolyline)),forced_start)

def main():
    assert ezdxf.__version__=='1.4.4';ezdxf.options.write_fixed_meta_data_for_testing=True
    OUT.mkdir(parents=True,exist_ok=True);ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export')
    importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch').install_dxf_template_pipeline_v9_patch()
    exporter=module.CadastralDxfExporter(wfs_client=None);metadata=importlib.import_module(ref.__package__+'.template_metadata')
    source_profiles=json.loads((ROOT/'rust/tests/fixtures/profiles/reference.json').read_text(encoding='utf-8'))
    selected=[next(c for c in source_profiles['cases'] if c['name']==name) for name in ['basic-normal','polyline-heights-normal','all-matched-normal']]
    assert all(c['rules'] is None for c in selected)
    datasets=[dict(job_id=i+1,title=c['name'],source_path='fixture.json',features=c['features']) for i,c in enumerate(selected)]
    for name,size,index in [('tiff.png',(12,8),1),('map.png',(8,12),2)]:
        image=Image.new('RGBA',size);image.putdata([((x*29+index)%256,(y*51+index)%256,(x+y*3)%256,255 if (x+y)%3 else 81) for y in range(size[1]) for x in range(size[0])]);image.save(OUT/name)
    def slot(index,profile=None):
        return dict(bounds=dict(min_x=120000+index*10,min_y=450000,max_x=120030+index*10,max_y=450020),label='PS2',title=f'Proefsleuf {index+1} Ø 漢',address='Adres:\\PTeststraat '+str(index) if index%2 else None,comments='Opmerkingen:\\PTest Ø 漢',tiff_png='rust/tests/fixtures/template-export/tiff.png',map_png='rust/tests/fixtures/template-export/map.png',profile=profile)
    custom=dict(template_maaiveld_segments=dict(start=dict(text=' Asfalt ',color='#aBcDeF'),middle=dict(text='invullen',color='bad'),end=dict(text='Gras 漢😀',color='#123456')))
    profile_drawings=json.loads((ROOT/'rust/tests/fixtures/profile-drawing/reference.json').read_text(encoding='utf-8'))
    surface_case=next(c for c in profile_drawings['cases'] if c['profile_case']=='basic-normal' and c['manual_scale']==250)
    requests=[
        ('mixed','assets/cadastral_template.dxf',[slot(0,dict(dataset=0,show_reference=False,avoid_collisions=True)),None,slot(2,dict(dataset=1,reverse=True,manual_scale=250,metadata=custom,diameter_prefix=' DN ',clip_markers=True)),slot(3)]),
        ('extended','assets/cadastral_template.dxf',[slot(0)]+[None]*38+[slot(39,dict(dataset=2,manual_scale=20,avoid_collisions=False,auto_fill_bgt=False,show_reference=False))]),
        ('image-only','rust/tests/fixtures/template-pages/one-page.dxf',[None,slot(1)]+[None]*6+[slot(8)]),
        ('surface-reference','assets/cadastral_template.dxf',[
            slot(0,dict(dataset=0,metadata=surface_case['metadata'],manual_scale=250)),
            slot(1,dict(dataset=0,metadata=surface_case['metadata'],use_custom_maaiveld=False,reverse=True,marker_diameter=.001)),
            slot(2,dict(dataset=0,metadata=surface_case['metadata'],auto_fill_bgt=False,manual_scale=5001)),
        ]),
    ]
    cases=[]
    for name,source,slots in requests:
        request=dict(template=source,slots=slots,datasets=datasets,rules=source_profiles['rules'],roads=selected[0]['roads'],terrain_boundaries=selected[0]['terrain'],surfaces=[],layers=[],geometry=[],settings=dict(project=' Exportproject Ø 漢 ',drawn_by=' User Ø ',export_date=[2026,10,19],label_gap=18.))
        if name=='surface-reference':
            request['surfaces']=surface_case['surfaces']
            request['layers']=[dict(name='Prepared overview',color=3,true_color=None,lineweight=None)]
            request['geometry']=[dict(layer='Prepared overview',layout=None,shape=dict(Circle=dict(center=[120000.,450000.],radius=2.)))]
        doc=ezdxf.readfile(ROOT/source);before=len(exporter._detect_template_slots(doc));exporter._extend_template_for_slot_count(doc,len(slots));targets=exporter._detect_template_slots(doc)
        names=exporter._used_template_layout_names(len(slots));exporter._ensure_template_image_layer(doc);exporter._setup_text_styles(doc);exporter._ensure_template_layout_legends(doc,names)
        doc.units=6;doc.set_raster_variables(frame=0,quality=1,units='m');exporter._update_template_scale_list(doc)
        metadata.update_title_blocks(doc,names,project=request['settings']['project'],drawn_by=request['settings']['drawn_by'],export_date=datetime(2026,10,19))
        exporter._update_template_layout_page_numbers(doc,names);exporter._configure_template_wireframe_viewports(doc,[None if s is None else SimpleNamespace(bounds=module.Bounds(**s['bounds'])) for s in slots],names,18.)
        if request['geometry']:
            doc.layers.new('Prepared overview',dxfattribs={'color':3});doc.modelspace().add_circle((120000.,450000.),2.,dxfattribs={'layer':'Prepared overview'})
        drawn=0
        for i,(s,target) in enumerate(zip(slots,targets)):
            if s is None:exporter._clear_template_slot(doc,target);continue
            exporter._set_template_text(doc,target.label_handle,s['title'])
            if target.address_handle:exporter._set_template_mtext(doc,target.address_handle,s['address'] or 'Adres:')
            if target.comments_handle:exporter._set_template_mtext(doc,target.comments_handle,s['comments'])
            for role,box,inset,aspect in [('tiff',target.tiff_box,.04,True),('kaart',target.map_box,0.,False)]:
                # The actual pixels have been prepared upstream; original image
                # insertion uses only the verified dimensions and final path.
                path=OUT/('tiff.png' if role=='tiff' else 'map.png')
                exporter._add_box_image(doc,doc.modelspace(),path,box,s['label']+'_'+role,inset=inset,preserve_aspect=aspect)
                list(doc.modelspace().query('IMAGE'))[-1].image_def.dxf.filename=f'slot{i+1:05}_{role}.png'
            p=s['profile']
            if p is None:continue
            source_case=selected[p['dataset']];p.setdefault('forced_start',source_case['forced_start']);p.setdefault('marker_diameter',source_case['marker_scale'])
            data=original_dataset(ref,datasets[p['dataset']],p['forced_start']);rules=tuple(ref.ObjectLayerRule(tuple(r['keywords']),r['target_layer'],r['color_aci'],r['label']) for r in request['rules'])
            profile=exporter._build_template_cross_section_profile(data,rules,request['roads'],request['terrain_boundaries'],max(.005,p['marker_diameter']),p.get('reverse',False))
            if profile is not None:
                layer=SimpleNamespace(metadata=p.get('metadata',{}),bounds=module.Bounds(**s['bounds']))
                surfaces=[module.BgtSurfaceFeature('wegdeel',f['physical_appearance'],f['physical_appearance'],Polygon(f['geometry']['Polygon'][0],f['geometry']['Polygon'][1:])) for f in request['surfaces']] if p.get('auto_fill_bgt',True) else []
                exporter._add_template_cross_section(doc,doc.modelspace(),target,layer,s['label'],profile,p.get('show_reference',True),p.get('use_custom_maaiveld',True),surfaces,p.get('avoid_collisions',True),p.get('clip_markers',False),max(.005,p['marker_diameter']),p.get('manual_scale'),p.get('diameter_prefix','Ø').strip() or 'Ø');drawn+=1
        stream=io.StringIO();doc.write(stream);stream.seek(0);doc=ezdxf.read(stream)
        cases.append(dict(name=name,request=request,filled=sum(s is not None for s in slots),profiles=drawn,added_pages=max(0,(len(targets)-before)//8),expected=snapshot(doc,exporter)))
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'prepared export assemblies;',sum(c['profiles'] for c in cases),'complete active profiles')
if __name__=='__main__':main()
