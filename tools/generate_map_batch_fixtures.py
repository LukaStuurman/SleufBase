"""Independent original standalone PNG/opacity/filename contracts; no app startup."""
from dataclasses import asdict
from pathlib import Path
from unittest.mock import patch
import importlib,json,math,os,random
from PIL import Image,ImageFont,ImageDraw
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_MAP_BATCH_FIXTURES',ROOT/'rust/tests/fixtures/map-batch'))
FONT=ROOT/'rust/tests/fixtures/map-export/Hack-Regular.ttf'
SEED=9157
ANGLES=[0.,17.,90.,137.,-37.,0.]

def main():
    OUT.mkdir(parents=True,exist_ok=True)
    ref=reference();exp=importlib.import_module(ref.__package__+'.exporting');models=importlib.import_module(ref.__package__+'.models')
    renderer=importlib.import_module(ref.__package__+'.renderer').MapRenderer()
    cases=[];rng=random.Random(SEED)
    class White:
        def fetch_map(self,bounds,size):return Image.new('RGBA',size,(255,255,255,255))
    for n,angle in enumerate(ANGLES):
        salt=rng.randrange(245)
        image=Image.new('RGBA',(53,37));image.putdata([((x*17+y*3+salt)%245,(x*7+y*11+salt)%245,(x*23+y*19+salt)%245,[0,31,127,201,255][(x+y)%5]) if (x+y)%11 else (250,249,245,255) for y in range(37) for x in range(53)])
        path=OUT/f'input-{n}.tiff';image.save(path,compression='raw')
        r=math.radians(angle);tx=models.GeoTransform(a=.15*math.cos(r),b=.15*math.sin(r),c=155000.+n*.37,d=.15*math.sin(r),e=-.15*math.cos(r),f=463000.+n*.23)
        corners=[tx.pixel_to_world(x,y) for x,y in [(0,0),(image.width,0),(image.width,image.height),(0,image.height)]]
        bounds=models.Bounds(min(p[0] for p in corners),min(p[1] for p in corners),max(p[0] for p in corners),max(p[1] for p in corners))
        layer=models.GeoTiffLayer(path=path,image=image,transform=tx,bounds=bounds,epsg=28992)
        dpi=[96,120,144,96,120,96][n];scale=[225,150,300,225,450,125][n]
        exporter=exp.MapExporter(White(),renderer,dpi=dpi,scale=scale)
        out=OUT/f'original-{n}';out.mkdir(exist_ok=True)
        with patch.object(exp,'_load_font',lambda size,bold=False:ImageFont.truetype(str(FONT),size)):
            png=exporter.export_single(out,layer,[])
        with Image.open(png) as page:
            samples=[]
            for _ in range(400):
                x=82+rng.randrange(exporter.map_width_px//5,exporter.map_width_px*3//5)
                y=82+rng.randrange(exporter.map_height_px*3//10,exporter.map_height_px*3//5)
                samples.append([x,y,list(page.getpixel((x,y)))])
            cases.append(dict(path=path.name,transform=asdict(tx),bounds=asdict(bounds),dpi=dpi,scale=scale,
                expected=dict(name=png.name,mode=page.mode,size=list(page.size),dpi=list(page.info['dpi']),samples=samples)))
        png.unlink();out.rmdir()
    # Execute the real original duplicate-name publication on synthetic pages.
    duplicate=OUT/'duplicate-original';duplicate.mkdir(exist_ok=True)
    exporter=exp.MapExporter(None,renderer)
    exporter.build_page_image=lambda layer,*a,**kw:Image.new('RGBA',(2,2),layer.color)
    from types import SimpleNamespace
    layers=[SimpleNamespace(path=Path('first/scan.tif'),color=(255,0,0,255)),SimpleNamespace(path=Path('second/scan.tiff'),color=(0,0,255,255))]
    results=exporter.export_all(duplicate,layers,[])
    with Image.open(results[-1]) as saved:pixel=list(saved.getpixel((0,0)))
    bug=dict(names=[p.name for p in results],remaining_files=len(list(duplicate.iterdir())),last_pixel=pixel)
    for p in duplicate.iterdir():p.unlink()
    duplicate.rmdir()
    result=dict(cases=cases,original_duplicate_overwrite=bug)
    strokes=[]
    for width in range(1,6):
        for points in [[3,8,13,8],[13,8,3,8],[8,3,8,13],[8,13,8,3]]:
            canvas=Image.new('L',(20,20));ImageDraw.Draw(canvas).line(points,fill=255,width=width)
            strokes.append(dict(width=width,points=[points[:2],points[2:]],pixels=[[x,y] for y in range(20) for x in range(20) if canvas.getpixel((x,y))]))
    (OUT/'axis-strokes.json').write_text(json.dumps(strokes,separators=(',',':'))+'\n',encoding='utf-8')
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original standalone RGB/DPI pages / {sum(len(c["expected"]["samples"]) for c in cases)} original pixels; duplicate overwrite preserved as explicit old bug')
    return result
if __name__=='__main__':main()
