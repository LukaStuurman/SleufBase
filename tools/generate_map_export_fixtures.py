"""Independent original MapExporter geometry and Pillow raster oracles.

No application/settings initialization or native output. Font metrics are frozen
from an explicitly licensed test face, separate from outline rasterization.
"""
from dataclasses import asdict
from pathlib import Path
from unittest.mock import patch
import importlib,json,os,math
import uharfbuzz as hb
from PIL import Image,ImageDraw,ImageFont,__version__ as pillow_version
import numpy as np
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_MAP_FIXTURES',ROOT/'rust/tests/fixtures/map-export'))
FONT=ROOT/'rust/tests/fixtures/map-export/Hack-Regular.ttf'
def color(c):return list(c)+([255] if len(c)==3 else []) if c is not None else None
def xy(points):
    if points and isinstance(points[0],(int,float)):return [list(points[i:i+2]) for i in range(0,len(points),2)]
    return [list(p) for p in points]
def main():
    assert pillow_version=='11.3.0';assert hb.__version__=='0.51.0'
    OUT.mkdir(parents=True,exist_ok=True);ref=reference();package=ref.__package__
    exp=importlib.import_module(package+'.exporting');models=importlib.import_module(package+'.models');renderer=importlib.import_module(package+'.renderer').MapRenderer()
    result=dict(resizes=[],paints=[],pages=[],texts=[],shaping=[])
    with Image.open(ROOT/'rust/tests/fixtures/map-export/pdok-getmap-jpeg.jpg') as pdok:pdok.convert('RGBA').save(OUT/'pdok-expected.png')
    for i,(w,h) in enumerate([(1,1),(1,17),(23,1),(7,11),(31,19),(113,79)]):
        image=Image.new('RGBA',(w,h));image.putdata([((x*41+y*7)%256,(x*13+y*31)%256,(x*3+y*17)%256,[0,1,15,16,81,127,200,254,255][(x+y)%9]) for y in range(h) for x in range(w)])
        name=f'input-{i}.png';image.save(OUT/name)
        for crop in [[0,0,w,h]]+([[1,1,w-1,h-1]] if w>3 and h>3 else []):
            for size in [(1,1),(4,9),(37,23),(crop[2]-crop[0],crop[3]-crop[1]),(crop[2]-crop[0],13),(17,crop[3]-crop[1])]:
                case=f'resize-{len(result["resizes"]):03}';expected=f'{case}.png';image.crop(crop).resize(size,Image.Resampling.BILINEAR).save(OUT/expected)
                result['resizes'].append(dict(name=case,input=name,crop=crop,size=size,expected=expected))
    image=Image.open(OUT/'input-5.png');pixels=np.array(image);pixels[2:17,4:23]=[250,249,245,255];image=Image.fromarray(pixels);image.save(OUT/'paint-input.png')
    for i,(angle,flip,origin,view,size) in enumerate([
        (0,False,(0.,0.),(-4.,-2.,22.,18.),(83,61)),(0,False,(0.,0.),(2.,1.,13.,9.),(61,49)),
        (0,True,(0.,0.),(-2.,-2.,23.,20.),(31,27)),(17,False,(0.,0.),(-4.,-2.,22.,18.),(83,61)),
        (90,False,(0.,0.),(-20.,-20.,25.,25.),(81,73)),(137,False,(0.,0.),(-20.,-20.,25.,25.),(81,73)),
        (29,False,(168000.,450000.),(167996.,449998.,168022.,450018.),(83,61)),
        (0,False,(0.,0.),(80.,80.,90.,90.),(35,29)),
    ]):
        t=models.GeoTransform(-.17 if flip else .17,0.,origin[0],0.,-.21,origin[1]+16.)
        if angle:
            radians=np.deg2rad(angle);cs=float(np.cos(radians));sn=float(np.sin(radians))
            t=models.GeoTransform(cs*t.a,-sn*t.e,t.c,sn*t.a,cs*t.e,t.f)
        corners=[t.pixel_to_world(x,y) for x,y in [(0,0),(image.width,0),(image.width,image.height),(0,image.height)]]
        bounds=models.Bounds(min(x for x,y in corners),min(y for x,y in corners),max(x for x,y in corners),max(y for x,y in corners))
        layer=models.GeoTiffLayer(Path('fixture.tiff'),image,t,bounds,28992,opacity=1.)
        prepared=exp._prepare_export_tiff_layer(layer,forced_opacity=1.)
        canvas=Image.new('RGBA',size,(37,61,83,191));viewbounds=models.Bounds(*view)
        renderer._paint_tiff(canvas,models.ViewportTransform(viewbounds,*size),prepared)
        case=f'paint-{i:03}';expected=f'{case}.png';canvas.save(OUT/expected)
        result['paints'].append(dict(name=case,input='paint-input.png',transform=asdict(t),layout=dict(page_size=size,map_size=size,origin=[0.,0.],bounds=asdict(viewbounds),meters_per_pixel=viewbounds.width/size[0]),background=[37,61,83,191],expected=expected))
    fontinfo={};fontcache={};real_draw=ImageDraw.Draw
    def font(size,bold=False):
        key=(size,bold)
        if key not in fontcache:fontcache[key]=ImageFont.truetype(str(FONT),size)
        f=fontcache[key];fontinfo[id(f)]=key;return f
    class Recorder:
        def __init__(self,ops,measures):self.ops=ops;self.measures=measures
        def line(self,points,fill,width=1,**kwargs):self.ops.append(dict(Line=dict(points=xy(points),fill=color(fill),width=width)))
        def rectangle(self,bounds,fill=None,outline=None,width=1,**kwargs):self.ops.append(dict(Rect=dict(bounds=list(bounds),fill=color(fill),outline=color(outline),width=width,radius=0.)))
        def rounded_rectangle(self,bounds,radius,fill=None,outline=None,width=1,**kwargs):self.ops.append(dict(Rect=dict(bounds=list(bounds),fill=color(fill),outline=color(outline),width=width,radius=radius)))
        def ellipse(self,bounds,fill,outline,width,**kwargs):self.ops.append(dict(Ellipse=dict(bounds=list(bounds),fill=color(fill),outline=color(outline),width=width)))
        def polygon(self,points,fill,**kwargs):self.ops.append(dict(Polygon=dict(points=xy(points),fill=color(fill))))
        def text(self,position,text,font,fill,anchor=None,stroke_width=0,stroke_fill=None,**kwargs):
            size,bold=fontinfo[id(font)];self.ops.append(dict(Text=dict(position=list(position),text=text,size=size,bold=bold,anchor=anchor or 'la',fill=color(fill),stroke=[color(stroke_fill),stroke_width] if stroke_width else None,angle=0.,frame_rotation=False)))
        def textbbox(self,position,text,font,**kwargs):
            bbox=real_draw(Image.new('L',(1,1))).textbbox(position,text,font=font,**kwargs);size,bold=fontinfo[id(font)]
            self.measures.append(dict(text=text,size=size,bold=bold,bbox=list(bbox)));return bbox
    cases=[('rd',300,225,(168000.,450000.,168020.,450012.)),('thin',300,225,(3.,8.,3.1,80.)),('wide',300,225,(-100.,-40.,50.,-39.)),('dpi96',96,225,(100.,200.,130.,220.)),('dpi600',600,225,(1.,2.,31.,22.)),('scale',300,750,(168000.,450000.,168030.,450050.))]
    for base,dpi,scale,rawbounds in cases:
        for hasref in [False,True]:
            exporter=exp.MapExporter(None,renderer,dpi=dpi,scale=scale);bounds=models.Bounds(*rawbounds)
            padded=bounds.padded(max(1.,min(4.,max(bounds.width,bounds.height)*.1)));view=exporter._determine_map_bounds(padded)
            annotation=models.ProfileReferenceAnnotation(bounds.center_x,bounds.center_y) if hasref else None
            ops=[];measures=[];recorder=Recorder(ops,measures)
            def rotated(page,position,text,font,angle,fill=(0,0,0)):
                size,bold=fontinfo[id(font)];ops.append(dict(Text=dict(position=list(position),text=text,size=size,bold=bold,anchor='la',fill=color(fill),stroke=None,angle=angle,frame_rotation=True)))
            mapimage=Image.new('RGBA',(exporter.map_width_px,exporter.map_height_px),(190,210,180,255))
            with patch.object(exp,'_load_font',font),patch.object(exp.ImageDraw,'Draw',lambda *a,**k:recorder),patch.object(exp,'_draw_rotated_text',rotated):
                exporter._compose_page(mapimage,view,'Bron: PDOK ©',annotation)
            name=base+('-reference' if hasref else '')
            result['pages'].append(dict(name=name,bounds=asdict(bounds),settings=dict(dpi=dpi,scale=scale),reference=[annotation.x,annotation.y] if annotation else None,attribution='Bron: PDOK ©',layout=dict(page_size=[exporter.page_width_px,exporter.page_height_px],map_size=[exporter.map_width_px,exporter.map_height_px],origin=[82.,82.],bounds=asdict(view),meters_per_pixel=view.width/exporter.map_width_px),measures=measures,ops=ops))
            if base=='rd':
                with patch.object(exp,'_load_font',font):exporter._compose_page(mapimage,view,'Bron: PDOK ©',annotation).save(OUT/(name+'-original.png'))
            mapimage.close()
    for text in ['Legenda','Referentiepunt','Bron: PDOK ©','168000','Café A\u0301g Ø','مرحبا 123','אבג ABC 123','नमस्ते']:
        for size in [20,36,72,84]:
            bbox=real_draw(Image.new('L',(1,1))).textbbox((0,0),text,font=font(size))
            result['texts'].append(dict(text=text,size=size,bold=False,bbox=list(bbox)))
    face=hb.Face(FONT.read_bytes());hbfont=hb.Font(face);hbfont.scale=(face.upem,face.upem)
    for text,script,direction in [('Café A\u0301g Ø','Latn','ltr'),('office ffi','Latn','ltr'),('مرحبا','Arab','rtl'),('אבג','Hebr','rtl'),('नमस्ते','Deva','ltr')]:
        for size in [32,64]:
            buffer=hb.Buffer();buffer.add_str(text);buffer.script=script;buffer.direction=direction;hb.shape(hbfont,buffer)
            quant=lambda n:math.floor(n*size/face.upem*64+.5)/64 if n>=0 else math.ceil(n*size/face.upem*64-.5)/64
            pen=0.;glyphs=[]
            for info,pos in zip(buffer.glyph_infos,buffer.glyph_positions):
                glyphs.append([info.codepoint,pen+quant(pos.x_offset),-quant(pos.y_offset)]);pen+=quant(pos.x_advance)
            result['shaping'].append(dict(text=text,size=size,glyphs=glyphs,advance=pen))
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'{len(result["resizes"])} resize, {len(result["paints"])} paint, {len(result["pages"])} page, {len(result["texts"])} text cases')
if __name__=='__main__':main()
