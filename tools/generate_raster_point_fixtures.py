"""Invoke real original point modes on synthetic layers/datasets; no app or network."""
from pathlib import Path
from types import CodeType,FunctionType,MethodType,SimpleNamespace
from dataclasses import asdict
import importlib,importlib.util,json,math,os,random
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_RASTER_POINT_FIXTURES',ROOT/'rust/tests/fixtures/raster-points'))
SEED=10139
COUNT=96
def main():
    spec=importlib.util.spec_from_file_location('raster_point_legacy',ROOT/'legacy_bytecode.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    cls=next(c for c in h.read_validated_code('app',ROOT/'legacy_bytecode.py').co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    ref=reference();exp=importlib.import_module(ref.__package__+'.cadastral_export');models=importlib.import_module(ref.__package__+'.models')
    patch=importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch');patch.install_dxf_template_pipeline_v9_patch()
    exporter=exp.CadastralDxfExporter(wfs_client=None);rules=ref.build_object_layer_rules()
    scope={'math':math,'CadastralDxfExporter':exp.CadastralDxfExporter,'KickTheMapObjectDataset':ref.KickTheMapObjectDataset}
    methods={c.co_name:FunctionType(c,scope) for c in cls.co_consts if isinstance(c,CodeType) and c.co_name in [
        '_auto_cross_section_start_candidate_for_layer','_cross_section_start_candidates_for_layer','_pick_cross_section_start_candidate',
        '_set_reference_point_metadata','_set_template_cross_section_start_metadata','_template_cross_section_start_metadata_point',
        '_rotate_point_around_center','_selection_tolerance_meters','clear_all_reference_points']}
    rng=random.Random(SEED);cases=[]
    for n in range(COUNT):
        cx=155000.+rng.uniform(-3000,3000);cy=463000.+rng.uniform(-3000,3000)
        angle=[0.,37.5,-90.,1e-10,137.,360.][n%6]
        transform=models.GeoTransform(.5,0.,cx-5.,0.,-.5,cy+5.)
        metadata={'notes':'Behoud Ø '+str(n),'unknown':{'nested':[1,2,'x']},'rotation_degrees':angle}
        layer=SimpleNamespace(metadata=metadata,name='Proefsleuf Ø.tif',bounds=models.Bounds(cx-5,cy-5,cx+5,cy+5),transform=transform,image=SimpleNamespace(width=20,height=20))
        names=['punt','water','POINT','LS','１２','  ','¹','onbekend','PUNT']
        points=tuple(ref.KickTheMapObjectPoint('object '+str(i),names[(i+n)%len(names)],cx+rng.uniform(-5,5),cy+rng.uniform(-4,4),None if (i+n)%11==0 else 9.+i*.1,'','','') for i in range(2+n%7))
        if n%19==0:points=points[:1]
        elif n%17==0:
            points=tuple(ref.KickTheMapObjectPoint('coincident '+str(i),'PUNT',cx,cy,10.+i,'','','') for i in range(3))
        roads=[[(cx-10,cy-2),(cx+10,cy-2)]] if n%3 else []
        terrain=[[(cx+3,cy-10),(cx+3,cy+10)]] if n%4 else []
        if n%5==0 and points:
            metadata['template_cross_section_start_point']={'x':points[-1].x+(.49 if n%10==0 else .51),'y':points[-1].y}
        def dataset():
            forced=app._template_cross_section_start_metadata_point(layer)
            return ref.KickTheMapObjectDataset(n+1,'PS'+str(n),Path('points.json'),points,(),forced)
        app=SimpleNamespace(cadastral_exporter=exporter,road_centerline_client=SimpleNamespace(fetch_paths=lambda bounds:roads),bgt_terrain_boundary_client=SimpleNamespace(fetch_paths=lambda bounds:terrain),meters_per_pixel=[.02,.1,.5][n%3],min_meters_per_pixel=.001)
        for name,fn in methods.items():setattr(app,name,MethodType(fn,app) if fn.__code__.co_varnames[0]=='self' else fn)
        app._is_virtual_trench_layer=lambda layer:False
        app._rotation_degrees_for_layer=lambda layer:float(layer.metadata.get('rotation_degrees',0.))
        app._tiff_layer_center_world=lambda layer:layer.transform.pixel_to_world(layer.image.width*.5,layer.image.height*.5)
        app._load_maaiveld_dataset_for_layer=lambda layer:dataset()
        app._resolved_cross_section_layer_rules=lambda:rules
        app._build_maaiveld_profile_for_layer=lambda layer:exporter._build_template_cross_section_profile(dataset(),rules,roads,terrain,.1,False)
        before=json.loads(json.dumps(metadata));auto=app._auto_cross_section_start_candidate_for_layer(layer,dataset(),rules,road_centerline_client=None,terrain_boundary_client=None)
        rows=app._cross_section_start_candidates_for_layer(layer)
        picks=[]
        for i in range(4):
            xy=([rows[i%len(rows)]['x']+.1*i,rows[i%len(rows)]['y']] if rows else [cx,cy])
            pick=app._pick_cross_section_start_candidate(layer,*xy)
            picks.append(dict(world=xy,expected=None if pick is None else rows.index(pick)))
        display=[cx+2.3,cy-1.7];app._set_reference_point_metadata(layer,*display);after_ref=json.loads(json.dumps(metadata))
        start=[cx-1.1,cy+2.2];app._set_template_cross_section_start_metadata(layer,*start);after_start=json.loads(json.dumps(metadata))
        raw=exporter._template_reference_metadata_point(layer);shown=exporter._template_reference_display_point(layer,None)
        features=[dict(object_name=p.object_name,source_name=p.source_name,geometry={'Point':dict(x=p.x,y=p.y,z=p.z)},attributes=['','','']) for p in points]
        cases.append(dict(metadata=before,transform=asdict(transform),dataset=dict(job_id=n+1,title='PS'+str(n),source_path='points.json',features=features),roads=roads,terrain=terrain,
            automatic=None if auto is None else [auto.x,auto.y],rows=[dict(raw=[r['raw_x'],r['raw_y']],display=[r['x'],r['y']],selected=r['selected']) for r in rows],
            picks=picks,meters_per_pixel=app.meters_per_pixel,selection_tolerance=app._selection_tolerance_meters(),reference_click=display,after_reference=after_ref,start_click=start,after_start=after_start,reference_display=list(shown)))
    layers=[SimpleNamespace(metadata={}),SimpleNamespace(metadata={'template_reference_point':None,'keep':'x'}),SimpleNamespace(metadata={'template_reference_point':{'x':1,'y':2},'template_cross_section_start_point':{'x':4,'y':5}})]
    status=[];refresh=[];app.tiff_layers=layers;app.set_status=status.append;app._refresh_map_edit_markers=lambda:refresh.append(True);app.request_render=lambda **kw:refresh.append(kw)
    app.clear_all_reference_points();first=dict(metadata=[l.metadata.copy() for l in layers],status=status[:],refresh=refresh[:]);app.clear_all_reference_points()
    result=dict(cases=cases,clear=dict(first=first,last_status=status[-1]))
    OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original metadata/rotation/automatic/candidate contracts; {sum(len(c["picks"]) for c in cases)} original hit selections; clear-all original side effects')
    return result
if __name__=='__main__':main()
