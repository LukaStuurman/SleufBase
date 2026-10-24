"""Fresh original virtual map actions plus independently read written exports."""
from pathlib import Path
from tempfile import TemporaryDirectory
from datetime import datetime,timedelta
import copy,importlib,json,subprocess,sys
import ezdxf
from PIL import Image,ImageDraw
import generate_virtual_map_fixtures as oracle
from verify_native_profile_leaders import full_signature
from verify_native_template_images import compare
ROOT=Path(__file__).resolve().parents[1]

def close(actual,expected,path='root'):
    if isinstance(expected,bool) or expected is None or isinstance(expected,str):assert actual==expected,(path,actual,expected)
    elif isinstance(expected,(int,float)):assert abs(actual-expected)<2e-8,(path,actual,expected)
    elif isinstance(expected,list):
        assert len(actual)==len(expected),path
        for i,(a,b) in enumerate(zip(actual,expected)):close(a,b,f'{path}[{i}]')
    elif isinstance(expected,dict):
        assert actual.keys()==expected.keys(),(path,actual.keys(),expected.keys())
        for key in expected:close(actual[key],expected[key],path+'.'+key)

def exports(directory):
    from verify_native_discipline_exports import signature
    from verify_native_template_reports import original_signature
    ref=oracle.reference();models=importlib.import_module(ref.__package__+'.models');vt=importlib.import_module(ref.__package__+'.virtual_trench');module=importlib.import_module(ref.__package__+'.cadastral_export');exp=importlib.import_module(ref.__package__+'.exporting')
    importlib.import_module(ref.__package__+'.marxact_boundary_patch').install_marxact_boundary_patch()
    importlib.import_module(ref.__package__+'.marxact_local_cross_section_patch').install_marxact_local_cross_section_patch()
    importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch').install_dxf_template_pipeline_v9_patch()
    exporter=module.CadastralDxfExporter(None);audit=json.loads((directory/'audit.json').read_text(encoding='utf-8'));entities=0
    start=datetime.fromtimestamp((directory/'started.txt').stat().st_mtime).date();end=datetime.fromtimestamp((directory/'audit.json').stat().st_mtime).date();dates={(start+timedelta(days=i)).strftime('%d-%m-%Y') for i in range((end-start).days+1)}
    for case in audit:
        t=case['trench'];points=[dict(t['start'],role='start',object_name='Beginpunt',source_name='PUNT'),dict(t['end'],role='end',object_name='Eindpunt',source_name='PUNT')]
        for o in t['objects']:points.append(dict(o['position'],role=o['role'],object_name=o['object_name'],source_name=o['source_name'],attribute_1=o['attributes'][0],attribute_2=o['attributes'][1],attribute_3=o['attributes'][2]))
        payload=dict(points=points,width_meters=t['width_meters'])
        if t['boundary_3d']:payload['boundary_3d']=[[p['x'],p['y'],p['z']] for p in t['boundary_3d']]
        metadata=copy.deepcopy(t['metadata']);metadata[vt.VIRTUAL_TRENCH_METADATA_KEY]=payload
        layer=models.GeoTiffLayer(Path(t['label']+'.tif'),Image.new('RGBA',(1,1)),models.GeoTransform(1,0,0,0,-1,1),models.Bounds(0,0,1,1),28992,metadata=metadata);vt.refresh_virtual_trench_layer(layer)
        # Independently invert the original reference click, then compare the
        # actual original dataset/profile/entity writer with the published DXF.
        world=case['reference_world'];angle=float(metadata.get('rotation_degrees',0));cx,cy=layer.transform.pixel_to_world(layer.image.width/2,layer.image.height/2)
        raw=exporter._rotate_template_map_point(world[0],world[1],cx,cy,-angle) if abs(angle)>=1e-9 else tuple(world)
        close(t['metadata']['template_reference_point'],dict(x=raw[0],y=raw[1]))
        layer.metadata['template_reference_point']=dict(x=raw[0],y=raw[1])
        doc=ezdxf.readfile(directory/case['path']);assert not doc.audit().has_errors
        values=[(a.dxf.tag,a.dxf.text) for e in doc.layouts.get('Blad1').query('INSERT') for a in e.attribs]
        assert ('TEKENAAR1','Technicus') in values;assert any(tag=='DATUM1' and text in dates for tag,text in values)
        assert any(tag in ['GEM-PROJECT','PROJECTOMSCHRIJVING','PROJECTNAAM'] and text=='Virtuele kaart Ø' for tag,text in values)
        assert any('Bronnotitie' in str(e.text) for e in doc.modelspace().query('MTEXT'))
        if case['draw']:
            dataset=vt.build_virtual_trench_dataset(layer);rules=tuple(ref.ObjectLayerRule(tuple(r['keywords']),r['target_layer'],r['color_aci'],r['label']) for r in case['rules'])
            profile=exporter._build_template_cross_section_profile(dataset,rules,[],[],.02,False)
            expected=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf');baseline={e.dxf.handle for e in expected.modelspace()};exporter._setup_text_styles(expected);target=exporter._detect_template_slots(expected)[0]
            exporter._add_template_cross_section(expected,expected.modelspace(),target,layer,case['label'],profile,True,True,[],True,False,.02,None,'Ø')
            wanted=[full_signature(e) for e in expected.modelspace() if e.dxf.handle not in baseline];layers={e['layer'] for e in wanted};actual=[full_signature(e) for e in doc.modelspace() if e.dxf.handle not in baseline and e.dxf.layer in layers]
            compare(json.loads(json.dumps(actual)),json.loads(json.dumps(wanted)),case['name']+' complete written profile');entities+=len(wanted)
        images=[Path(doc.entitydb[e.dxf.image_def_handle].dxf.filename) for e in doc.modelspace().query('IMAGE') if 'slot00001' in doc.entitydb[e.dxf.image_def_handle].dxf.filename]
        assert len(images)==2
        map_path=next(p for p in images if p.name.endswith('_kaart.png'))
        renderer=exp.MapExporter(None,None,dpi=96,scale=225);bounds=renderer._determine_map_bounds(layer.bounds);transform=models.ViewportTransform(bounds,renderer.map_width_px,renderer.map_height_px)
        sx,sy=transform.world_to_screen(*world);sx+=82;sy+=82
        # The original 22px/300-DPI ring has a constant physical size, including
        # image-only export. Native fixes the old fixed-pixel low-DPI layout.
        # Original Pillow supplies the expected outline at the corrected size.
        scale=renderer.page_width_px/3508;rx=11*renderer.map_width_px/round(renderer.map_width_px/scale);ry=11*renderer.map_height_px/round(renderer.map_height_px/scale)
        mask=Image.new('L',(renderer.page_width_px,renderer.page_height_px));draw=ImageDraw.Draw(mask);draw.ellipse((sx-rx,sy-ry,sx+rx,sy+ry),outline=255,width=max(1,round(4*scale)))
        with Image.open(map_path) as image:
            assert image.size==mask.size
            for y in range(max(0,int(sy)-12),min(image.height,int(sy)+13)):
                for x in range(max(0,int(sx)-12),min(image.width,int(sx)+13)):
                    if mask.getpixel((x,y)):assert image.getpixel((x,y))[:3]==(0,0,0),(case['name'],'reference ring',x,y,image.getpixel((x,y)))
        workbook=(directory/case['path']).with_suffix('.xlsx');assert workbook.is_file()
        wanted,_=original_signature(case['rules'],[case['dataset']],[('PS12',0)]);assert signature(workbook)==wanted
        layer.image.close()
    print(f'{len(audit)} real virtual/MarXact/image-only DXFs / {entities} complete original profile entities / six PNGs / three XLSX and actual reference rings passed')

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-virtual-map-runtime-') as temp:
        root=Path(temp);oracle.OUT=root/'original';oracle.SEED=861433;oracle.COUNT=47;cases=oracle.main()
        keys=['source','reference_world','anchor','target','picks','mpp','selection_tolerance','creation','names'];request=root/'input.json';request.write_text(json.dumps([{key:c[key] for key in keys} for c in cases]),encoding='utf-8');out=root/'output.json'
        subprocess.run([driver,str(request),str(out)],check=True,timeout=120,capture_output=True)
        for c,a in zip(cases,json.loads(out.read_text(encoding='utf-8')),strict=True):
            close(a['expected_picks'],c['expected_picks']);close(a['reference_raw'],c['reference_raw']);close(a['reference_display'],c['reference_display'])
            assert a['next_name']==c['next_name']
            expected=copy.deepcopy(c['expected']);expected['boundary_3d']=c['corrected_boundary'];close(a['expected'],expected);assert a['close_error'];close(a['created'],c['creation']['created'][0])
        print(f'{len(cases)} fresh original moves/references/creation chains / {len(cases)*9} nearest hits match optimized driver, including corrected measured outlines')
        if len(sys.argv)>2:exports(Path(sys.argv[2]))
if __name__=='__main__':main()
