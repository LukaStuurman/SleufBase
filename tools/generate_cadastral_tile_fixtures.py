"""Actual original WMTS planning/composition without a client/app/settings init."""
from dataclasses import asdict
from pathlib import Path
import hashlib,importlib,json,os,random
import numpy as np
from PIL import Image
from pyproj import Transformer
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_CADASTRAL_TILE_FIXTURES',ROOT/'rust/tests/fixtures/cadastral-tiles'))
SEED=83073
COUNT=28

def original():
    ref=reference();pdok=importlib.import_module(ref.__package__+'.pdok');models=importlib.import_module(ref.__package__+'.models')
    # __init__ creates a persistent cache: bypass it. All executed helpers are
    # the actual source functions; only synthetic input/tiles are supplied.
    client=pdok.PdokKadastralekaartWmtsTileClient.__new__(pdok.PdokKadastralekaartWmtsTileClient)
    client.min_zoom=0;client.max_zoom=22;client.transformer=Transformer.from_crs('EPSG:28992','EPSG:3857',always_xy=True)
    return ref,models,client

def tile(zoom,x,y,salt):
    yy,xx=np.indices((256,256),dtype=np.int64)
    a=np.array([0,31,127,201,255],dtype=np.uint8)[(xx+yy+x+y)%5]
    rgba=np.stack(((xx*17+yy*3+x*7+y*11+salt)%245,(xx*7+yy*11+x*13+y*3+salt)%245,(xx*23+yy*19+x*3+y*5+salt)%245,a),axis=-1).astype(np.uint8)
    return Image.fromarray(rgba)

def render(client,bounds,size,salt):
    plan=client._prepare_tile_request(bounds,size)
    stitched=Image.new('RGBA',plan['stitched_size'],(243,243,243,255))
    for x,y in plan['tile_coords']:
        with tile(plan['zoom'],x,y,salt) as image:
            stitched.alpha_composite(image,((x-plan['min_tile_x'])*256,(y-plan['min_tile_y'])*256))
    image=client._render_tile_request(plan,stitched);stitched.close()
    return plan,image

def plan_record(plan):
    record={k:(asdict(v) if k=='mercator_bounds' else v) for k,v in plan.items()}
    half=20037508.342789244;span=plan['tile_span'];r=plan['resolution'];b=plan['mercator_bounds']
    ox=plan['min_tile_x']*span-half;oy=half-plan['min_tile_y']*span
    record['crop']=[round((b.min_x-ox)/r),round((oy-b.max_y)/r),round((b.max_x-ox)/r),round((oy-b.min_y)/r)]
    return record

def main():
    OUT.mkdir(parents=True,exist_ok=True);ref,models,client=original();rng=random.Random(SEED);cases=[]
    for n in range(COUNT):
        x=rng.uniform(10000,280000);y=rng.uniform(290000,610000)
        width=rng.uniform(10,2500);height=width*rng.uniform(.15,2.5)
        size=(rng.randrange(12,114),rng.randrange(8,98))
        if n==0:x,y,width,height,size=155000.,463000.,.4,.2,(80,40)
        if n==1:x,y,width,height,size=155000.,463000.,.001,.001,(1,1)
        bounds=models.Bounds(x,y,x+width,y+height);salt=rng.randrange(245)
        request=dict(bounds=asdict(bounds),size=size,salt=salt)
        try:
            plan,image=render(client,bounds,size,salt)
        except ValueError as error:
            cases.append(dict(request=request,error=str(error)));continue
        expected=f'expected-{n}.png';image.save(OUT/expected);image.close()
        urls=[client.build_tile_url(plan['zoom'],xx,yy) for xx,yy in plan['tile_coords']]
        cases.append(dict(request=request,plan=plan_record(plan),urls=urls,image=expected))
    result=dict(sources_lf_sha256={p:hashlib.sha256((ROOT/p).read_bytes().replace(b'\r\n',b'\n')).hexdigest() for p in ['pdok.py','web_tiles.py']},cases=cases)
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original Kadaster WMTS requests, {sum("image" in c for c in cases)} complete original crop/alpha/bilinear images')
    return result
if __name__=='__main__':main()
