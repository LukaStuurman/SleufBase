"""Original comment/edit/clear and workspace-reset methods, without app startup."""
from pathlib import Path
from types import CodeType,FunctionType,MethodType,SimpleNamespace
from dataclasses import asdict
import importlib,importlib.util,json,math,os,random
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_MAP_ACTION_FIXTURES',ROOT/'rust/tests/fixtures/map-actions'))
SEED=61217
COUNT=54

def main():
    spec=importlib.util.spec_from_file_location('map_action_legacy',ROOT/'legacy_bytecode.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    cls=next(c for c in h.read_validated_code('app',ROOT/'legacy_bytecode.py').co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    instructions=list(__import__('dis').get_instructions(cls));key=next(instructions[i-1].argval for i,x in enumerate(instructions) if x.opname=='STORE_NAME' and x.argval=='TIFF_COMMENT_METADATA_KEY')
    ref=reference();models=importlib.import_module(ref.__package__+'.models');exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    names=['_tiff_comment_text','_set_tiff_comment_text','_template_map_comments','_tiff_layer_center_world','clear_comments','_edit_selected_tiff_comment','_clear_selected_tiff_comment','clear_all','_reset_workspace_state']
    methods={c.co_name:FunctionType(c,dict(math=math,MapComment=models.MapComment)) for c in cls.co_consts if isinstance(c,CodeType) and c.co_name in names}
    def bind(app):
        for name,fn in methods.items():setattr(app,name,MethodType(fn,app) if fn.__code__.co_varnames[0]=='self' else fn)
    rng=random.Random(SEED);cases=[]
    values=['Grondwater\n  grondwater\nStraße\nSTRASSE','\u001c Test Ø \u2003','  ',None,False,True,0,1.25,[],[1,'Ø',None],{'note':'café'}]
    for n in range(COUNT):
        layers=[];rasters=[]
        for j in range(3):
            cx=155000.+rng.uniform(-100,100);cy=463000.+rng.uniform(-100,100)
            tx=models.GeoTransform(1.,0.,cx-5,0.,-1.,cy+3);angle=[0.,37.5,-90.][j]
            # Rotate the actual affine basis around the image centre.
            a=math.radians(angle);cs=math.cos(a);sn=math.sin(a)
            tx=models.GeoTransform(cs,sn,cx-5*cs-3*sn,sn,-cs,cy-5*sn+3*cs)
            corners=[tx.pixel_to_world(x,y) for x,y in [(0,0),(10,0),(10,6),(0,6)]]
            bounds=models.Bounds(min(p[0] for p in corners),min(p[1] for p in corners),max(p[0] for p in corners),max(p[1] for p in corners))
            metadata={key:values[(n+j)%len(values)],'keep':{'x':j,'unknown':'Ø'},'rotation_degrees':angle}
            layers.append(SimpleNamespace(metadata=metadata,transform=tx,bounds=bounds,image=SimpleNamespace(width=10,height=6),path=Path(f'Proefsleuf {j} Ø.tif')))
            rasters.append(dict(metadata=json.loads(json.dumps(metadata)),transform=asdict(tx),bounds=asdict(bounds),width=10,height=6))
        comments=[models.MapComment(x=layers[i%3].bounds.min_x+rng.uniform(-2,10),y=layers[i%3].bounds.min_y+rng.uniform(-2,6),text=t) for i,t in enumerate(['Water\nwater','  ','Straße','STRASSE','Lijn\r\nTweede\u2028Derde\tKolom'])][:n%6]
        status=[];calls=[];app=SimpleNamespace(TIFF_COMMENT_METADATA_KEY=key,tiff_layers=layers,map_comments=comments[:],set_status=status.append,_refresh_tiff_list=lambda:calls.append('refresh'),request_render=lambda **kw:calls.append(kw))
        bind(app);texts=[app._tiff_comment_text(l) for l in layers]
        combined=[asdict(c) for c in app._template_map_comments()]
        formatted=[exporter._template_comments_text(l,app._template_map_comments()) for l in layers]
        edit=[None,'  Nieuwe regel\n  Tweede regel  ','\u001c\u2003','Zelfde\nzelfde'][n%4]
        app._selected_tiff_layer=lambda:layers[n%3];app._prompt_multiline_input=lambda **kw:edit
        app._edit_selected_tiff_comment();after_edit=[l.metadata.copy() for l in layers];edit_status=status[:]
        app._clear_selected_tiff_comment();after_selected_clear=[l.metadata.copy() for l in layers];selected_status=status[-1:] if len(status)>len(edit_status) else []
        app.clear_comments();after_clear=[l.metadata.copy() for l in layers];clear_status=status[-1];assert not app.map_comments
        app.clear_comments();assert status[-1]=='Geen opmerkingen om te wissen.'
        cases.append(dict(rasters=rasters,comments=[asdict(c) for c in comments],texts=texts,combined=combined,formatted=formatted,edit=edit,selected=n%3,after_edit=after_edit,edit_status=edit_status,after_selected_clear=after_selected_clear,selected_status=selected_status,after_clear=after_clear,clear_status=clear_status))
    calls=[]
    app=SimpleNamespace(tiff_layers=['tiff'],dxf_overlays=['cad'],map_comments=['note'],template_export_slot_order=['slot'],maaiveld_markers=['marker'],maaiveld_marker_payloads=['payload'],rotation_drag_active=True,drag_origin=[1,2],drag_center=[3,4],click_origin=[5,6],maaiveld_mode_active=True,selected_tiff_index=2,selected_dxf_index=3,display_image='image',display_bounds='bounds',display_size=[10,20],project_path='project.klicproject',settings={'keep':True},session='unchanged')
    bind(app)
    callbacks=['_cancel_virtual_trench_interaction','_cancel_tiff_rotation_mode','_clear_reference_point_mode_state','_clear_start_point_mode_state','_clear_feature_selection','_clear_background_cache','_refresh_tiff_list','_refresh_dxf_list','_set_info_text','_update_streetsmart_selection_state','_refresh_map_edit_markers','_refresh_map_edit_button_states','request_render','set_status']
    for name in callbacks:setattr(app,name,lambda *args,_name=name,**kw:calls.append(dict(name=_name,args=args,kwargs=kw)))
    app._default_info_text=lambda:'default info';app.clear_all()
    fields=['tiff_layers','dxf_overlays','map_comments','template_export_slot_order','maaiveld_markers','maaiveld_marker_payloads','rotation_drag_active','drag_origin','drag_center','click_origin','maaiveld_mode_active','selected_tiff_index','selected_dxf_index','display_image','display_bounds','display_size','project_path','settings','session']
    result=dict(key=key,cases=cases,clear_all=dict(state={k:getattr(app,k) for k in fields},calls=calls))
    OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original comment/edit/clear batches, {len(cases)*3} affine TIFF notes and complete workspace-reset side effects, without app/settings/accounts')
    return result
if __name__=='__main__':main()
