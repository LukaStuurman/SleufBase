"""Original absolute TIFF transforms, ring picking and real pointer handlers; no app."""
from pathlib import Path
from types import CodeType,FunctionType,MethodType,SimpleNamespace
from dataclasses import asdict
import importlib,importlib.util,json,math,os,random
from PIL import Image
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_RASTER_ROTATION_FIXTURES',ROOT/'rust/tests/fixtures/raster-rotation'))
SEED=70319
COUNT=36
def main():
    spec=importlib.util.spec_from_file_location('rotation_legacy',ROOT/'legacy_bytecode.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    cls=next(c for c in h.read_validated_code('app',ROOT/'legacy_bytecode.py').co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    ref=reference();models=importlib.import_module(ref.__package__+'.models');gt=importlib.import_module(ref.__package__+'.geotiff')
    names=['_rotation_degrees_for_layer','_rotation_overlay_geometry','_rotation_hit_test','_screen_angle_from_center','_tiff_layer_center_world','_tiff_layer_footprint','_on_canvas_press','_on_canvas_drag','_on_canvas_release']
    functions={c.co_name:FunctionType(c,dict(math=math,ViewportTransform=models.ViewportTransform)) for c in cls.co_consts if isinstance(c,CodeType) and c.co_name in names}
    def snapshot(layer,n):return dict(path=str(layer.path),width=layer.image.width,height=layer.image.height,transform=asdict(layer.transform),bounds=asdict(layer.bounds),epsg=28992,kickthemap_job_id=n+1,kickthemap_job_title=f'PS{n}',kickthemap_job_sidecar=None,metadata=json.loads(json.dumps(layer.metadata)))
    rng=random.Random(SEED);cases=[]
    for n in range(COUNT):
        w=8+n%17;hh=6+n%13;image=Image.new('RGBA',(w,hh),(80,130,170,190))
        tx=models.GeoTransform(rng.uniform(.04,.6),rng.uniform(-.04,.04),155000.+rng.uniform(-500,500),rng.uniform(-.04,.04),-rng.uniform(.05,.8),463000.+rng.uniform(-500,500))
        center=tx.pixel_to_world(w*.5,hh*.5);meta=dict(keep={'unknown':[n,'Ø']},template_reference_point=dict(x=center[0]+1,y=center[1]-2),template_cross_section_start_point=dict(x=center[0],y=center[1]))
        layer=models.GeoTiffLayer(Path(f'rotation-{n}.tiff'),image,tx,gt._calculate_bounds(tx,w,hh),28992,metadata=meta)
        initial=[0.,37.5,-90.,180.][n%4]
        if n%4:layer=gt.set_geotiff_layer_rotation(layer,initial)
        source=snapshot(layer,n);width=900+n%7;height=600+n%5;mpp=[.005,.2,1.5][n%3]
        bounds=models.Bounds(center[0]-width*mpp*.5+2,center[1]-height*mpp*.5-7,center[0]+width*mpp*.5+2,center[1]+height*mpp*.5-7)
        app=SimpleNamespace(tiff_layers=[layer],rotation_mode_tiff_index=0,_current_view_bounds=lambda:bounds,_canvas_size=lambda:(width,height),_is_virtual_trench_layer=lambda _:False,ROTATION_RING_PADDING_PX=28.,ROTATION_RING_HIT_TOLERANCE_PX=18.,ROTATION_HANDLE_RADIUS_PX=10.)
        for name,fn in functions.items():setattr(app,name,MethodType(fn,app) if fn.__code__.co_varnames[0]=='self' else fn)
        steps=[];angles=[initial+90,initial-90,rng.uniform(-1440,1440),540.,-180.,1e-10,0.]
        for angle in angles:
            layer=gt.set_geotiff_layer_rotation(layer,angle);assert layer.image is image;app.tiff_layers[0]=layer
            geometry=app._rotation_overlay_geometry();cx,cy=geometry['center'];r=geometry['radius'];hx,hy=geometry['handle']
            # Tk supplies integer mouse coordinates; test complete hit decisions
            # at real pixel positions rather than epsilon-offset floating rings.
            picks=[[round(x),round(y)] for x,y in [[cx+r,cy],[cx+r+17,cy],[cx+r+19,cy],[hx+15,hy],[hx+19,hy],[cx,cy],[cx,cy-r+10]]]
            hit=[app._rotation_hit_test(*p) is not None for p in picks]
            overlay={k:geometry[k] for k in ['center','radius','handle','footprint','rotation']}
            steps.append(dict(angle=angle,expected=snapshot(layer,n),overlay=overlay,picks=picks,hits=hit,angle_center=[cx,cy],screen_angles=[app._screen_angle_from_center(cx,cy,*p) for p in picks]))
        # Drive the actual original press/drag/release branches on the synthetic layer.
        layer=gt.set_geotiff_layer_rotation(layer,initial);app.tiff_layers[0]=layer
        app.virtual_trench_mode=None;app.rotation_drag_active=False;app.canvas=SimpleNamespace(focus_set=lambda:None);app.drag_origin=None;app.drag_center=None;app.click_origin=None;app.set_status=lambda _:None
        app._canvas_event_world_position=lambda event:(0.,0.)
        def update(index,angle,**kw):
            app.tiff_layers[index]=gt.set_geotiff_layer_rotation(app.tiff_layers[index],angle);return app.tiff_layers[index]
        app._set_tiff_rotation=update;app._cancel_tiff_rotation_mode=lambda *_:None
        g=app._rotation_overlay_geometry();cx,cy=g['center'];r=g['radius'];press=g['handle'];app._on_canvas_press(SimpleNamespace(x=press[0],y=press[1]));assert app.rotation_drag_active
        drag=[]
        for delta in [23.,181.,-179.,359.]:
            a=math.radians(initial+90+delta);p=[cx+math.cos(a)*r,cy-math.sin(a)*r];app._on_canvas_drag(SimpleNamespace(x=p[0],y=p[1]));drag.append(dict(pointer=p,expected=app._rotation_degrees_for_layer(app.tiff_layers[0])))
        app._on_canvas_release(SimpleNamespace(x=p[0],y=p[1]));assert not app.rotation_drag_active
        cases.append(dict(input=source,viewport=dict(bounds=asdict(bounds),width_px=width,height_px=height),steps=steps,drag=dict(initial=initial,center=[cx,cy],press=list(press),moves=drag,final=app._rotation_degrees_for_layer(app.tiff_layers[0]))))
    precise=dict(center=[0.,0.],radius=42.,handle=[0.,-42.],footprint=[],rotation=0.)
    app._rotation_overlay_geometry=lambda:precise.copy()
    precise_hits=[dict(pointer=p,expected=app._rotation_hit_test(*p) is not None) for p in [[60.,0.],[60.000001,0.],[24.,0.],[23.999999,0.],[0.,-60.],[0.,-60.000001],[0.,0.]]]
    result=dict(cases=cases,precise_hit_overlay=precise,precise_hits=precise_hits);OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original rotation chains / {len(cases)*7} absolute transforms / {len(cases)*49} ring hits and screen angles / {len(cases)*4} actual pointer drags, without app/settings/accounts')
    return result
if __name__=='__main__':main()
