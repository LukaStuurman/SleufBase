"""Full original TIFF asset preparation: exact RGBA pixels, crop and direction."""
from pathlib import Path
from types import SimpleNamespace
import importlib,json,math,os,shutil,tempfile
from PIL import Image,TiffImagePlugin
import PIL,tifffile,numpy as np
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_RASTER_FIXTURES',ROOT/'rust/tests/fixtures/template-raster'))

def main():
    assert PIL.__version__=='11.3.0' and tifffile.__version__=='2025.5.10';OUT.mkdir(parents=True,exist_ok=True)
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export');models=importlib.import_module(ref.__package__+'.models')
    importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch').install_dxf_template_pipeline_v9_patch()
    exporter=module.CadastralDxfExporter(wfs_client=None);cases=[];sources=[]
    for index,(w,h) in enumerate([(1,1),(1,9),(9,1),(3,5),(16,11),(17,13),(31,19),(2,2),(11,11),(31,32)]):
        pixels=[]
        for y in range(h):
            for x in range(w):
                p=((x*71+y*13)%256,(x*37+y*61)%256,(x*19+y*43)%256,[0,1,15,16,127,254,255][(x+y*3)%7])
                if index in (4,8):
                    p=(255,255,255,255) if x in (0,w-1) or y in (0,h-1) or (x==w//2 and y==h//2) else (30,60,90,255)
                if index==5:p=(255,255,255,0) if x<4 or y<3 or x>w-5 or y>h-4 else (20+x,30+y,80,255)
                if index==6:p=(251,240,246,16) if (x+y)%2 else (10,20,30,255)
                if index==7:p=(x*80,y*70,99,0)
                if index==9:p=(255,255,255,255)
                pixels.append(p)
        image=Image.new('RGBA',(w,h));image.putdata(pixels);name=f'input-{index}.png';image.save(OUT/name);sources.append(name)
    def add(source,transform,axis=None,roads=None,terrain=None,reverse=False):
        index=len(cases);t=models.GeoTransform(**transform);roads=roads or [];terrain=terrain or []
        profile=None if axis is None else SimpleNamespace(start_point=SimpleNamespace(x=axis[0][0],y=axis[0][1]),end_point=SimpleNamespace(x=axis[1][0],y=axis[1][1]))
        def run(image,label):
            rgba=image.convert('RGBA');w,h=rgba.size
            layer=models.GeoTiffLayer(Path(source),image,t,models.Bounds(0,0,w,h),28992)
            vector=exporter._template_tiff_orientation_pixel_vector(layer,roads,terrain,profile);rotation=0.
            if vector is not None and sum(v*v for v in vector)>1e-6:
                target=270. if profile is not None else (90. if reverse else 270.)
                rotation=exporter._normalize_rotation_degrees(target-exporter._template_bearing_from_pixel_vector(*vector))
                rgba=rgba.rotate(rotation,resample=Image.Resampling.BICUBIC,expand=True,fillcolor=(255,255,255,0))
            rotated=list(rgba.size);bbox=rgba.getchannel('A').point(lambda a:255 if a>15 else 0).getbbox()
            crop=[0,0,*rgba.size] if bbox is None else [max(0,bbox[0]-2),max(0,bbox[1]-2),min(rgba.width,bbox[2]+2),min(rgba.height,bbox[3]+2)]
            with tempfile.TemporaryDirectory() as td:
                result=exporter._build_template_tiff_raster(Path(td),layer,label,index,roads,terrain,profile,reverse)
                shutil.copyfile(result,OUT/result.name)
            return result.name,dict(source_size=[w,h],vector=vector,rotation=rotation,rotated_size=rotated,crop=crop)
        expected,details=run(Image.open(OUT/source),f'expected-{index:03}');difference=None
        if source.endswith('.tiff'):
            orientation=int(Image.open(OUT/source).tag_v2.get(274,1))
            if orientation>=5:
                raw=tifffile.imread(OUT/source)
                assert np.array_equal(raw,np.asarray(Image.open(OUT/'input-5.png')))
                corrected=Image.fromarray(raw).transpose({5:Image.Transpose.TRANSPOSE,6:Image.Transpose.ROTATE_270,7:Image.Transpose.TRANSVERSE,8:Image.Transpose.ROTATE_90}[orientation])
                difference=dict(reason='Pillow 11.3.0 RGBA TIFF Orientation 5..8 has wrong post-load dimensions; native follows raw numeric pixels and the orientation tag.',original_expected=expected,original_details=details,oracle='tifffile numeric RGBA plus explicit orientation; original rotation/crop/alpha thereafter')
                expected,details=run(corrected,f'expected-native-{index:03}')
        cases.append(dict(name=f'case-{index:03}',input=source,kind='tiff' if source.endswith('.tiff') else 'png',transform=transform,axis=axis,roads=roads,terrain=terrain,reverse=reverse,expected=expected,details=details,intentional_difference=difference))
    transforms=[dict(a=1.,b=0.,c=0.,d=0.,e=-1.,f=0.),dict(a=.5,b=.15,c=120000.,d=-.2,e=-.7,f=450000.)]
    for i,source in enumerate(sources):
        for j,angle in enumerate([0.,45.,90.,135.,180.,225.,270.,315.,13.37,-89.999999,33.3333,179.999999]):
            transform=transforms[(i+j)%2];t=models.GeoTransform(**transform);bearing=math.radians(270.-angle);dx=-math.sin(bearing)*10.;dy=-math.cos(bearing)*10.
            start=t.pixel_to_world(2.,3.);end=t.pixel_to_world(2.+dx,3.+dy)
            add(source,transform,[start,end],reverse=bool(j%2))
    t=transforms[0]
    for reverse in (False,True):
        add('input-4.png',t,reverse=reverse)
        add('input-4.png',t,roads=[[[-100.,-8.],[100.,-8.]]],reverse=reverse)
        add('input-4.png',t,roads=[[[-100.,-9.5],[100.,-9.5]]],terrain=[[[12.,100.],[12.,-100.]]],reverse=reverse)
        add('input-4.png',t,roads=[[[8.,100.],[8.,-100.]]],terrain=[[[4.,100.],[4.,-100.]]],reverse=reverse)
        add('input-4.png',t,roads=[[[100.,100.],[101.,100.]],[[1.,2.],[1.,2.]]],reverse=reverse)
        add('input-4.png',t,axis=[[1.,2.],[1.,2.]],terrain=[[[4.,100.],[4.,-100.]]],reverse=reverse)
    for orientation in range(1,9):
        image=Image.open(OUT/'input-5.png');tags=TiffImagePlugin.ImageFileDirectory_v2();tags[274]=orientation
        name=f'orientation-{orientation}.tiff';image.save(OUT/name,compression='raw',tiffinfo=tags)
        add(name,transforms[1],axis=[[120001.,449998.],[120010.,449995.]],reverse=orientation%2==0)
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'raster preparations: 136 original and 4 documented numeric-orientation corrections')
if __name__=='__main__':main()
