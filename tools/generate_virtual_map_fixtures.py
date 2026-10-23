"""Original virtual-layer map methods on synthetic layers; never start the app."""
from pathlib import Path
from types import CodeType,FunctionType,MethodType,SimpleNamespace
from dataclasses import asdict
import copy,importlib,importlib.util,json,math,os,random,re
from PIL import Image,ImageDraw
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_VIRTUAL_MAP_FIXTURES',ROOT/'rust/tests/fixtures/virtual-map'))
SEED=97051
COUNT=32

def ellipse_fixtures():
    """Independent Pillow pixels, including the actual 22 px reference ring."""
    cases=[];rng=random.Random(481192)
    boxes=[[20.3,12.7,42.3,34.7],[0,0,0,0],[1,1,1,12],[1,1,12,1],[-8,-7,15,17],[48,32,80,70]]
    boxes += [[rng.uniform(-15,40),rng.uniform(-15,25),0,0] for _ in range(26)]
    for box in boxes[6:]:box[2]=box[0]+rng.randrange(0,75);box[3]=box[1]+rng.randrange(0,65)
    for bounds in boxes:
        for fill,width in [(True,None),(False,1),(False,4),(True,4),(True,0),(False,80)]:
            image=Image.new('RGBA',(64,48),(9,17,33,255));draw=ImageDraw.Draw(image)
            draw.ellipse(bounds,fill=(231,211,191,240) if fill else None,outline=(0,0,0,255) if width is not None else None,width=width or 0)
            cases.append(dict(bounds=bounds,fill=fill,width=width,pixels=''.join({(9,17,33,255):'.',(231,211,191,240):'f',(0,0,0,255):'o'}[p] for p in image.getdata())));image.close()
    (OUT/'ellipses.json').write_text(json.dumps(dict(cases=cases),separators=(',',':'))+'\n',encoding='utf-8')

def main():
    spec=importlib.util.spec_from_file_location('virtual_legacy',ROOT/'legacy_bytecode.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    cls=next(c for c in h.read_validated_code('app',ROOT/'legacy_bytecode.py').co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    ref=reference();models=importlib.import_module(ref.__package__+'.models');vt=importlib.import_module(ref.__package__+'.virtual_trench');exp=importlib.import_module(ref.__package__+'.cadastral_export')
    # These geometry patches are active in __init__.py. The settings-driven
    # live-color hook is deliberately not invoked by this geometry-only oracle.
    importlib.import_module(ref.__package__+'.marxact_boundary_patch').install_marxact_boundary_patch()
    importlib.import_module(ref.__package__+'.marxact_local_cross_section_patch').install_marxact_local_cross_section_patch()
    names=['_coerce_float','_point_segment_distance','_virtual_trench_layer_center_world','_find_virtual_trench_layer_near','_set_reference_point_metadata','_rotation_degrees_for_layer','_rotate_point_around_center','_tiff_layer_center_world','_normalize_rotation_degrees','_rotate_virtual_trench_layer','_translate_virtual_trench_layer','_start_virtual_trench_layer_move','_handle_virtual_trench_canvas_click','_on_canvas_press','_on_canvas_release']
    env=dict(math=math,virtual_trench_points=vt.virtual_trench_points,virtual_trench_endpoints=vt.virtual_trench_endpoints,virtual_trench_width=vt.virtual_trench_width,refresh_virtual_trench_layer=vt.refresh_virtual_trench_layer,CadastralDxfExporter=exp.CadastralDxfExporter)
    functions={c.co_name:FunctionType(c,env) for c in cls.co_consts if isinstance(c,CodeType) and c.co_name in names}
    name_fn=FunctionType(next(c for c in cls.co_consts if isinstance(c,CodeType) and c.co_name=='_next_virtual_trench_name'),dict(re=re))
    def bind(app):
        for name,fn in functions.items():setattr(app,name,MethodType(fn,app) if fn.__code__.co_varnames[0]=='self' else fn)
        return app
    def snapshot(layer):
        pts=vt.virtual_trench_points(layer);start,end=vt.virtual_trench_endpoints(layer);payload=layer.metadata[vt.VIRTUAL_TRENCH_METADATA_KEY]
        def xyz(p):return dict(x=p['x'],y=p['y'],z=p.get('z'))
        metadata={k:copy.deepcopy(v) for k,v in layer.metadata.items() if k not in [vt.VIRTUAL_TRENCH_METADATA_KEY,'marxact_boundary_3d']}
        boundary=[dict(x=p[0],y=p[1],z=p[2]) for p in payload.get('boundary_3d',[])]
        return dict(label=layer.path.stem,width_meters=vt.virtual_trench_width(layer),start=xyz(start),end=xyz(end),objects=[dict(role=p['role'],object_name=p['object_name'],source_name=p['source_name'],position=xyz(p),attributes=[p.get('attribute_1',''),p.get('attribute_2',''),p.get('attribute_3','')]) for p in pts if p is not start and p is not end],dekband_rows=[],source_path='measured.dxf' if boundary else None,boundary_3d=boundary,alignment=None,job_id=0,metadata=metadata)
    rng=random.Random(SEED);cases=[]
    for n in range(COUNT):
        x=155000.+rng.uniform(-900,900);y=463000.+rng.uniform(-900,900);a=rng.uniform(-math.pi,math.pi);length=rng.uniform(.6,24.);width=rng.uniform(.1,1.8)
        points=[dict(point_id='start',role='start',object_name='Beginpunt',source_name='PUNT',x=x,y=y,z=2.2),dict(point_id='end',role='end',object_name='Eindpunt',source_name='PUNT',x=x+math.cos(a)*length,y=y+math.sin(a)*length,z=2.4)]
        for i in range(4):
            c=length*(i+1)/5;offset=rng.uniform(-2,2)
            points.append(dict(point_id=f'point-{i}',role='object',object_name=f'LS {i}',source_name='LS',x=x+math.cos(a)*c-math.sin(a)*offset,y=y+math.sin(a)*c+math.cos(a)*offset,z=1.2+i*.1,attribute_1='Ø',attribute_2='',attribute_3='bron'))
        payload=dict(points=points,width_meters=width,next_point_number=5)
        if n%4==0:payload['boundary_3d']=[[x-1,y-1,2.],[x+length+1,y-1,2.1],[x+length+1,y+2,2.2],[x-1,y+2,2.3]]
        layer=models.GeoTiffLayer(Path(f'PS{n+1}.tif'),Image.new('RGBA',(1,1)),models.GeoTransform(1,0,0,0,-1,1),models.Bounds(0,0,1,1),28992,metadata={vt.VIRTUAL_TRENCH_METADATA_KEY:payload,'keep':{'unknown':[n,'Ø']},'rotation_degrees':0.,'tiff_comment_text':'Bronnotitie'})
        vt.refresh_virtual_trench_layer(layer)
        app=bind(SimpleNamespace(_is_virtual_trench_layer=lambda l:vt.is_virtual_trench_layer(l),rotation_mode_tiff_index=None,rotation_drag_active=False,_canvas_event_world_position=lambda e:(e.x,e.y),canvas=SimpleNamespace(focus_set=lambda:None),_select_layer_in_tiff_list=lambda l:None,_redraw_canvas_overlays=lambda:None,set_status=lambda s:None,_refresh_virtual_trench_layer_state=lambda *a,**k:None,meters_per_pixel=[.005,.1,1.5][n%3],_selection_tolerance_meters=lambda:.3))
        def cancel(*a,**k):app.virtual_trench_mode=None;app.virtual_trench_active_layer=None
        app._cancel_virtual_trench_interaction=cancel
        initial=37.5 if n%8==4 else [0.,37.5,-90.,180.][n%4]
        if initial:app._rotate_virtual_trench_layer(layer,initial)
        app.tiff_layers=[layer,copy.deepcopy(layer)]
        center=app._virtual_trench_layer_center_world(layer);picks=[[center[0]+rng.uniform(-8,8),center[1]+rng.uniform(-8,8)] for _ in range(8)]+[list(center)]
        expected_picks=[None if (l:=app._find_virtual_trench_layer_near(*p)) is None else next(i for i,v in enumerate(app.tiff_layers) if v is l) for p in picks]
        source=snapshot(layer);world=[center[0]+3.25,center[1]-1.75];app._set_reference_point_metadata(layer,*world)
        initial_reference=snapshot(layer)['metadata']['template_reference_point'];display=exp.CadastralDxfExporter(None)._template_reference_display_point(layer,None)
        anchor=[center[0]+.25,center[1]-.15];target=[anchor[0]+rng.uniform(-50,50),anchor[1]+rng.uniform(-50,50)]
        def click(x,y):
            event=SimpleNamespace(x=x,y=y);app._on_canvas_press(event);app._on_canvas_release(event)
        app._start_virtual_trench_layer_move(layer,tuple(anchor));click(*target)
        after=snapshot(layer);dx=target[0]-anchor[0];dy=target[1]-anchor[1]
        corrected=[dict(p,x=p['x']+dx,y=p['y']+dy) for p in after['boundary_3d']]
        creation=[];app.virtual_trench_mode='placing_start';app.virtual_trench_pending_start=None;app.virtual_trench_copy_template=None;app.virtual_trench_pointer_world=None
        app._create_virtual_trench_layer=lambda start,end:(creation.append([list(start),list(end)]) or SimpleNamespace(path=Path('Created.tif')))
        app._open_virtual_trench_editor=lambda l:None
        click(x,y);first_mode=app.virtual_trench_mode
        click(x+.1,y);short_mode=app.virtual_trench_mode
        end=[x+.5,y] if n%5==0 else [x+1.,y+2.];click(*end)
        cases.append(dict(source=source,reference_world=world,reference_raw=initial_reference,reference_display=display,anchor=anchor,target=target,expected=after,corrected_boundary=corrected,picks=picks,mpp=app.meters_per_pixel,selection_tolerance=.3,expected_picks=expected_picks,creation=dict(start=[x,y],too_close=[x+.1,y],end=end,first_mode=first_mode,short_mode=short_mode,created=creation)))
        name_cases=[[],['Virtuele proefsleuf 1','Virtuele proefsleuf 3'],[' VIRTUELE PROEFSLEUF 002 ','anders'],['VİRTUELE PROEFſLEUF ٠٨'],['Virtuele proefsleuf','Virtuele proefsleuf 0'],['Virtuele proefsleuf '+'9'*60],['Virtuele proefsleuf -1','Virtuele proefsleuf 3x'],['Virtuele proefsleuf 3','physical','Virtuele proefsleuf 7']]
        names=list(name_cases[n%len(name_cases)])
        if n%8 in [1,2,7]:names.append(f'Virtuele proefsleuf {random.Random(SEED+n).randrange(500)}')
        cases[-1]['names']=names;cases[-1]['next_name']=name_fn(SimpleNamespace(tiff_layers=[SimpleNamespace(path=Path(name+'.tif')) for name in names]))
        layer.image.close()
    (OUT/'reference.json').parent.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(dict(cases=cases),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    ellipse_fixtures()
    print(f'{len(cases)} original virtual moves/references/creation chains / {len(cases)*9} nearest hit decisions; measured-boundary old bug retained')
    return cases
if __name__=='__main__':main()
