"""Original isolated point selection, axis-copy geometry and AHN queue handler.

No Tk app/settings initialized. Only non-geometric refresh/dialog services and
copy-spec surface initialization are faked; coordinate transforms run unchanged.
"""
from pathlib import Path
from types import CodeType,FunctionType,MethodType,SimpleNamespace
import copy,importlib,importlib.util,json,math,os,queue,random
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_VIRTUAL_TOOL_FIXTURES',ROOT/'rust/tests/fixtures/virtual-tools'))
def main():
    spec=importlib.util.spec_from_file_location('virtual_tool_legacy',ROOT/'legacy_bytecode.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    cls=next(c for c in h.read_validated_code('app',ROOT/'legacy_bytecode.py').co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    ref=reference();vt=importlib.import_module(ref.__package__+'.virtual_trench')
    names=['_coerce_float','_line_local_coordinates','_line_point_from_local_coordinates','_virtual_trench_copy_target_end','_build_virtual_trench_payload_from_template','_find_virtual_trench_point_near','_virtual_trench_point_depth','_virtual_trench_point_uses_auto_surface','_sync_virtual_trench_object_surfaces_to_start','_poll_background_queue']
    env=dict(math=math,queue=queue,virtual_trench_points=vt.virtual_trench_points,virtual_trench_endpoints=vt.virtual_trench_endpoints,ordered_virtual_trench_points=vt.ordered_virtual_trench_points)
    functions={c.co_name:FunctionType(c,env) for c in cls.co_consts if isinstance(c,CodeType) and c.co_name in names}
    def app():
        a=SimpleNamespace(_is_virtual_trench_layer=lambda l:True,_selection_tolerance_meters=lambda:.3,meters_per_pixel=.05,VIRTUAL_TRENCH_WIDTH_METERS=.6)
        for name,fn in functions.items():setattr(a,name,MethodType(fn,a) if fn.__code__.co_varnames[0]=='self' else fn)
        # Keep original surface fields intact to isolate original XY transforms.
        a._virtual_trench_point_from_copy_spec=lambda spec,x,y:(dict(spec,x=x,y=y),False)
        a.external_paths_queue=queue.Queue();a.background_queue=queue.Queue();a.virtual_trench_surface_queue=queue.Queue();a.winfo_exists=lambda:False
        a._resolve_virtual_trench_point=lambda layer,pid,fallback:next(p for p in vt.virtual_trench_points(layer) if p['point_id']==pid)
        a._refresh_virtual_trench_layer_visuals=lambda *args,**kw:None
        return a
    def vertex(p):return dict(x=p['x'],y=p['y'],z=p.get('z'))
    def native(points):
        start=next(p for p in points if p['role']=='start');end=next(p for p in points if p['role']=='end')
        return dict(label='Bron',width_meters=.6,start=vertex(start),end=vertex(end),start_metadata=start,end_metadata=end,objects=[dict(role=p['role'],object_name=p['object_name'],source_name=p.get('source_name',''),position=vertex(p),attributes=[p.get('attribute_1',''),p.get('attribute_2',''),p.get('attribute_3','')],metadata=p) for p in points if p['role'] not in ['start','end']])
    def layer(points):return SimpleNamespace(metadata={vt.VIRTUAL_TRENCH_METADATA_KEY:dict(width_meters=.6,points=points)})
    rng=random.Random(716820);cases=[]
    for n in range(48):
        x,y=rng.uniform(100000,220000),rng.uniform(350000,550000);angle=rng.uniform(-math.pi,math.pi);length=rng.uniform(1.,18.)
        end=(x+math.cos(angle)*length,y+math.sin(angle)*length);surface=rng.uniform(-5,25)
        points=[dict(point_id=role,role=role,object_name=role,x=p[0],y=p[1],z=surface,surface_z=surface,surface_mode='auto',surface_status='resolved',unknown={'retain':n}) for role,p in [('start',(x,y)),('end',end)]]
        for i in range(5):
            along=length*(i+1)/6;offset=rng.uniform(-.8,.8)
            points.append(dict(point_id=f'p{i}',role='object',object_name=f'Punt {i}',source_name='Kabel Ø',x=x+math.cos(angle)*along-math.sin(angle)*offset,y=y+math.sin(angle)*along+math.cos(angle)*offset,z=surface-rng.uniform(-.2,2),surface_z=surface,surface_mode='start',surface_status='start'))
        if n==1:
            points[0]['surface_mode']='\x1c AuTo \x1f'
            points[2]['surface_mode']='  START\t'
            points[3]['surface_mode']='\x1cStart '
        a=app();source=native(copy.deepcopy(points));l=layer(points)
        target_start=(x+rng.uniform(-10,10),y+rng.uniform(-10,10));direction=(target_start[0]+rng.uniform(-10,10),target_start[1]+rng.uniform(-10,10));target_end=a._virtual_trench_copy_target_end(dict(source_length=length),target_start,direction)
        if n%2:target_end=(end[0]+rng.uniform(-10,10),end[1]+rng.uniform(-10,10))
        transformed=a._build_virtual_trench_payload_from_template(dict(source_start=(x,y),source_end=end,points=points,width_meters=.6),target_start,target_end)['points']
        picks=[[p['x']+rng.uniform(-.3,.3),p['y']+rng.uniform(-.3,.3)] for p in points]+[[x+50,y+50]]
        selected=[a._find_virtual_trench_point_near(l,*p) for p in picks]
        refs=[None if p is None else p['point_id'] for p in selected]
        value=None if n%7==0 else rng.uniform(-5,35)
        a.virtual_trench_surface_queue.put((l,'start',points[0],value));a._poll_background_queue()
        cases.append(dict(source=source,target_start=target_start,target_end=target_end,direction=direction,copy_end=a._virtual_trench_copy_target_end(dict(source_length=length),target_start,direction),expected_geometry=[vertex(p) for p in transformed],picks=picks,expected_picks=refs,height=value,expected_surface=native(copy.deepcopy(points))))
    OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(dict(cases=cases),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf8')
    print(f'{len(cases)} original axis/copy transforms, {len(cases)*8} original nearest-point decisions, {len(cases)} original AHN queue updates')
if __name__=='__main__':main()
