"""Original AI profile/rotation/provider contracts without initializing the app.

The app's unchanged methods are rebound to a small dependency surface. BRT/BGT
responses, storage and provider instances are fakes; geometry/exporter run real.
No original settings/accounts, GPU, service or live-render patch are loaded.
"""
from pathlib import Path
from types import CodeType,FunctionType,MethodType,SimpleNamespace
from dataclasses import asdict
import ast,copy,importlib,importlib.util,json,math,os
from PIL import Image
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_AI_PROFILE_FIXTURES',ROOT/'rust/tests/fixtures/ai-profile'))
def main():
    ref=reference();ce=importlib.import_module(ref.__package__+'.cadastral_export');vt=importlib.import_module(ref.__package__+'.virtual_trench');models=importlib.import_module(ref.__package__+'.models');ai=importlib.import_module(ref.__package__+'.ai_maaiveld')
    importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch').install_dxf_template_pipeline_v9_patch()
    spec=importlib.util.spec_from_file_location('isolated_ai_app',ROOT/'legacy_bytecode.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    app=next(c for c in h.read_validated_code('app',ROOT/'legacy_bytecode.py').co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    names=['_build_maaiveld_profile_for_layer','_dataset_with_template_dekbanden','_rotate_point_around_center','_tiff_layer_center_world','_map_export_background_provider','_map_export_dxf_overlays','_visible_dxf_overlays']
    functions={c.co_name:FunctionType(c,{'math':math}) for c in app.co_consts if isinstance(c,CodeType) and c.co_name in names}
    functions['_dataset_with_template_dekbanden'].__kwdefaults__={'reverse_profile_direction':False}
    node=next(n for n in ast.walk(ast.parse((ROOT/'app.py').read_text(encoding='utf8'))) if isinstance(n,ast.FunctionDef) and n.name=='_ai_maaiveld_points_for_layer')
    env={'AiMaaiveldPoint':ai.AiMaaiveldPoint};exec(compile(ast.Module(body=[node],type_ignores=[]),str(ROOT/'app.py'),'exec'),env)
    functions[node.name]=env[node.name]
    rules=ref.build_object_layer_rules();cases=[]
    for index,(virtual,rotation,fail_roads) in enumerate([(True,0.,False),(True,37.,True),(False,0.,False),(False,-73.,False)]):
        x=155000.;y=463000.;start=dict(x=x,y=y,z=2.3);end=dict(x=x+10.,y=y,z=5.1)
        objects=[dict(role='object',object_name='LS',source_name='ls',position=dict(x=x+12.,y=y+.4,z=1.2),attributes=['','',''],metadata={})]
        bands=[dict(start_chainage=7.,end_chainage=2.,start_depth=.8,end_depth=.25,source_name='Dekband'),dict(start_chainage=-2.,end_chainage=20.,start_depth=.3,source_name='Beton')]
        metadata=dict(rotation_degrees=rotation,template_dekband_lines=copy.deepcopy(bands))
        points=[dict(role='start',object_name='Beginpunt',source_name='PUNT',**start),dict(role='end',object_name='Eindpunt',source_name='PUNT',**end)]+[dict(role=o['role'],object_name=o['object_name'],source_name=o['source_name'],**o['position']) for o in objects]
        if virtual:
            metadata[vt.VIRTUAL_TRENCH_METADATA_KEY]=dict(points=points,width_meters=.6)
            layer=SimpleNamespace(metadata=metadata,bounds=models.Bounds(x,y,x+10,y+1))
            _,_,_,bounds,w,height=vt._virtual_trench_render_geometry(layer)
            transform=models.GeoTransform(bounds.width/w,0,bounds.min_x,0,-bounds.height/height,bounds.max_y)
        else:bounds=models.Bounds(x-2,y-3,x+14,y+3);w=160;height=60;transform=models.GeoTransform(.1,0,bounds.min_x,0,-.1,bounds.max_y)
        layer=models.GeoTiffLayer(Path('ai-profile.tif'),Image.new('RGBA',(w,height)),transform,bounds,28992,metadata=metadata)
        native_dataset=dict(job_id=81,title='Fixture',source_path='jobFeatures.json',features=[])
        raw_points=[]
        for p in points:
            raw_points.append(ref.KickTheMapObjectPoint(p['object_name'],p['source_name'],p['x'],p['y'],p['z']))
            native_dataset['features'].append(dict(object_name=p['object_name'],source_name=p['source_name'],geometry=dict(Point={k:p[k] for k in ['x','y','z']}),attributes=['','','']))
        dataset=ref.KickTheMapObjectDataset(81,'Fixture',Path('jobFeatures.json'),tuple(raw_points),(),(x,y) if virtual else None)
        roads=[[(x+15,y-10),(x+15,y+10)]];terrain=[[(x-5,y-10),(x-5,y+10)]];fetches=[]
        def fetch(kind,paths):
            def run(b):
                fetches.append(dict(kind=kind,bounds=asdict(b)))
                if kind=='roads' and fail_roads:raise RuntimeError('offline roads')
                return paths
            return SimpleNamespace(fetch_paths=run)
        state=SimpleNamespace(cadastral_exporter=ce.CadastralDxfExporter(wfs_client=None),road_centerline_client=fetch('roads',roads),bgt_terrain_boundary_client=fetch('terrain',terrain),_load_maaiveld_dataset_for_layer=lambda layer:dataset,_resolved_cross_section_layer_rules=lambda:rules,_cross_section_marker_scale=lambda:.1)
        for name,fn in functions.items():setattr(state,name,MethodType(fn,state) if fn.__code__.co_varnames[0]=='self' else fn)
        profile=state._build_maaiveld_profile_for_layer(layer);expected=state._ai_maaiveld_points_for_layer(layer)
        native_meta=copy.deepcopy(metadata);native_meta.pop(vt.VIRTUAL_TRENCH_METADATA_KEY,None)
        native_bands=list(state.cadastral_exporter._template_dekband_rows(layer))
        trench=dict(label='Fixture',job_id=81,width_meters=.6,start=start,end=end,objects=objects,metadata=native_meta,dekband_rows=native_bands)
        raster=dict(path='ai-profile.tif',width=w,height=height,transform=asdict(transform),bounds=asdict(bounds),epsg=28992,kickthemap_job_id=81,kickthemap_job_title='Fixture',kickthemap_job_sidecar=None,metadata=native_meta)
        cases.append(dict(name=f'case-{index}',virtual=virtual,trench=trench,raster=raster,dataset=native_dataset,roads=roads,terrain=terrain,fail_roads=fail_roads,expected_points=[asdict(p) for p in expected],expected_axis=[dict(x=p.x,y=p.y,z=p.z) for p in [profile.start_point,profile.end_point]],fetch_bounds=fetches[0]['bounds']))
        layer.image.close()
    providers=[];cad=[]
    for value in ['osm','cadastral','pdok','unknown']:
        state=SimpleNamespace(settings=SimpleNamespace(map_export_background_source=value),osm_client='OpenStreetMap',cadastral_client='Cadastral',cyclomedia_aerial_client='Removed',pdok_client='PdokAerial')
        providers.append(dict(setting=value,expected=functions['_map_export_background_provider'](state)))
    for include in [False,True]:
        state=SimpleNamespace(settings=SimpleNamespace(include_klic_in_map_exports=include),dxf_overlays=[SimpleNamespace(visible=True),SimpleNamespace(visible=False)])
        state._visible_dxf_overlays=MethodType(functions['_visible_dxf_overlays'],state)
        cad.append(dict(include=include,count=len(functions['_map_export_dxf_overlays'](state))))
    OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(dict(cases=cases,providers=providers,cad=cad),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('4 original AI full profile/rotation cases; fresh BRT/BGT calls, independent failure, bands, 4 providers and CAD policy')
if __name__=='__main__':main()
