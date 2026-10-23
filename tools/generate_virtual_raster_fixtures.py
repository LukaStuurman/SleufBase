"""All pixels of active original virtual/MarXact raster, no app/settings."""
from dataclasses import asdict
from pathlib import Path
import importlib,json,math,os
from PIL import Image
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_VIRTUAL_RASTER_FIXTURES',ROOT/'rust/tests/fixtures/virtual-raster'))
def main():
    ref=reference();vt=importlib.import_module(ref.__package__+'.virtual_trench');models=importlib.import_module(ref.__package__+'.models')
    importlib.import_module(ref.__package__+'.marxact_boundary_patch').install_marxact_boundary_patch()
    importlib.import_module(ref.__package__+'.marxact_local_cross_section_patch').install_marxact_local_cross_section_patch()
    # The live settings hook is never installed/called. Explicit per-point
    # display colors exercise the same active final renderer deterministically.
    OUT.mkdir(parents=True,exist_ok=True);cases=[]
    for i,quality in enumerate([.25,.375,.75,1.,1.5,2.,3.,6.]):
        for measured in [False,True]:
            angle=math.radians([0.,37.5,90.,-73.][i%4]);co=math.cos(angle);si=math.sin(angle)
            def world(x,y):return (155000.+co*x-si*y,463000.+si*x+co*y)
            start=world(0,0);end=world(8,0);boundary=[world(x,y) for x,y in [(-.4,-1.1),(8.4,-1.1),(8.4,1.3),(4.5,1.3),(4.5,.2),(3.5,.2),(3.5,1.3),(-.4,1.3)]] if measured else []
            points=[dict(role='start',object_name='Beginpunt',source_name='PUNT',x=start[0],y=start[1],z=2.),dict(role='end',object_name='Eindpunt',source_name='PUNT',x=end[0],y=end[1],z=2.1)]
            objects=[]
            for j,(chain,offset,color) in enumerate([(6,.3,[200,10,20]),(1,-.6,[7,31,171]),(4,.7,[-5,500,True]),(3,.2,['٥','2_0','3']),(3,-.3,[7,8,9,10]),(8.7,1.8,['invalid',2,3])]):
                x,y=world(chain,offset);meta=dict(display_rgb=color,point_id='object-'+str(j),keep='Ø')
                points.append(dict(role='object',object_name='Kabel '+str(j),source_name='LS',x=x,y=y,z=1.2,**meta))
                objects.append(dict(role='object',object_name='Kabel '+str(j),source_name='LS',position=dict(x=x,y=y,z=1.2),attributes=['','',''],metadata=meta))
            payload=dict(points=points,width_meters=.6)
            if boundary:payload['boundary_3d']=[[x,y,2.] for x,y in boundary]
            layer=models.GeoTiffLayer(Path('PS1.tif'),Image.new('RGBA',(1,1)),models.GeoTransform(1,0,0,0,-1,1),models.Bounds(0,0,1,1),28992,metadata={vt.VIRTUAL_TRENCH_METADATA_KEY:payload})
            bands=[dict(start_chainage=3.,end_chainage=7.,start_depth=.2,end_depth=.8,source_name='Band B',display_rgb=[239,33,44]),
                   dict(start_chainage=-2.,end_chainage=9.,start_depth=.5,end_depth=.1,source_name='Band A',display_rgb=[12,230,64]),
                   dict(start_chainage=3.,end_chainage=6.,start_depth=.1,end_depth=.3,source_name='Band C',display_rgb=[99,31,181])] if i==2 else []
            layer.metadata[vt.TEMPLATE_DEKBAND_METADATA_KEY]=bands
            image,bounds,transform=vt.build_virtual_trench_render(layer,quality_multiplier=quality)
            name=f'{i:02}-'+('measured' if measured else 'manual');image.save(OUT/(name+'.png'))
            source=dict(trench=dict(label='PS1',width_meters=.6,start=dict(x=start[0],y=start[1],z=2.),end=dict(x=end[0],y=end[1],z=2.1),objects=objects,
                boundary_3d=[dict(x=x,y=y,z=2.) for x,y in boundary],dekband_rows=bands,metadata={},job_id=0),quality=quality,reverse=False)
            cases.append(dict(name=name,source=source,size=list(image.size),bounds=asdict(bounds),transform=asdict(transform)));image.close();layer.image.close()
    colors=[None,[],[1,2],[1,2,3],[True,False,8],[-7,300,2.9],['٥','2_0','+003'],['-99999999999999999999999999999999999999999999','99999999999999999999999999999999999999999999',0],['x',2,3],['1.5',2,3],[None,2,3],['2_',2,3],[' 3 ',2,3]]
    (OUT/'reference.json').write_text(json.dumps(dict(cases=cases,colors=[dict(value=v,expected=vt._display_rgb(dict(display_rgb=v))) for v in colors]),ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(len(cases),'active original complete RGBA rasters;',sum(c['size'][0]*c['size'][1] for c in cases),'pixels;',len(colors),'color conversions')
if __name__=='__main__':main()
