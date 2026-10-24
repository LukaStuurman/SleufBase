"""Original GEOS-based dominant BGT appearances and three profile probes."""
from pathlib import Path
from types import SimpleNamespace
import importlib
import json
import random
import math
import shapely
from shapely.geometry import box,Polygon,MultiPolygon,LineString,MultiLineString,GeometryCollection,Point,MultiPoint,mapping
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'rust/tests/fixtures/bgt-surface'

def native(g):
    if g.is_empty:return {'Collection':[]}
    if g.geom_type=='GeometryCollection':return {'Collection':[native(x) for x in g.geoms]}
    return {g.geom_type:mapping(g)['coordinates']}

def main():
    assert shapely.__version__=='2.0.7'
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export')
    exporter=module.CadastralDxfExporter(wfs_client=None);Feature=module.BgtSurfaceFeature
    cases=[]
    def add(name,start,end,features,axis_length=None):
        axis_length=math.dist(start,end) if axis_length is None else axis_length
        features=[Feature('wegdeel',str(i),text,g) for i,(text,g) in enumerate(features)]
        profile=SimpleNamespace(start_point=SimpleNamespace(x=start[0],y=start[1]),end_point=SimpleNamespace(x=end[0],y=end[1]),axis_length=axis_length)
        texts=exporter._template_bgt_surface_texts(profile,features)
        direct=exporter._dominant_bgt_physical_appearance(LineString([start,end]),features)
        cases.append(dict(name=name,start=start,end=end,axis_length=axis_length,features=[dict(physical_appearance=f.physical_appearance,geometry=native(f.geometry)) for f in features],expected=direct,texts=texts))
    square=box(0,0,10,10);hole=Polygon(square.exterior.coords,[box(2,2,8,8).exterior.coords])
    for y in [-1,0,1,2,5,8,10,11]:
        add('hole-'+str(y),[-1,y],[11,y],[('Asfalt',hole),('Gras',box(3,3,7,7))])
    add('duplicate-collection',[-1,5],[11,5],[('A',GeometryCollection([square,square,LineString([(0,5),(10,5)])])),('B',box(0,-1,11,11))])
    add('separate-features-add',[-1,5],[11,5],[('A',square),('a',square),('B',box(0,-1,11,11))])
    add('equal-keys-first-spelling',[-1,5],[11,5],[('  Straße  ',square),('STRASSE',square)])
    add('tie-key-order',[-1,5],[11,5],[('Z',square),('A',square)])
    add('multiline-overlap',[-1,5],[11,5],[('A',MultiLineString([[(0,5),(5,5)],[(2,5),(7,5)]])),('B',box(0,0,6,10))])
    add('point-touch',[-1,-1],[0,0],[('A',square),('B',Point(0,0))])
    add('empty',[-1,5],[11,5],[('A',GeometryCollection()),(' ',square)])
    add('zero-axis',[1,1],[1,1],[('A',square)])
    add('tiny-axis',[1,1],[1+1e-10,1],[('A',square)])
    for w in [1e-10,1e-8,1.0001e-8,1e-7]:add('threshold-'+str(w),[0,0],[1,0],[('A',box(0,-1,w,1))])
    rng=random.Random(627314)
    for i in range(520):
        offset=[120000,450000] if i%2 else [0,0]
        start=[offset[0]+rng.uniform(-10,10),offset[1]+rng.uniform(-10,10)]
        end=[offset[0]+rng.uniform(-10,10),offset[1]+rng.uniform(-10,10)]
        features=[]
        for j in range(rng.randint(1,8)):
            x=offset[0]+rng.uniform(-12,8);y=offset[1]+rng.uniform(-12,8);w=rng.uniform(.1,16);h=rng.uniform(.1,16)
            if j%4==0:g=Polygon(box(x,y,x+w,y+h).exterior.coords,[box(x+w*.2,y+h*.2,x+w*.8,y+h*.8).exterior.coords])
            elif j%4==1:g=MultiPoint([(x+rng.random()*w,y+rng.random()*h) for _ in range(8)]).convex_hull
            elif j%4==2:g=MultiPolygon([box(x,y,x+w*.4,y+h),box(x+w*.6,y,x+w,y+h)])
            else:g=box(x,y,x+w,y+h)
            features.append((rng.choice(['Asfalt','asfalt','Gras','Straße','STRASSE','  Klinkers  ','']),g))
        add('random-'+str(i),start,end,features)
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'original BGT geometry/probe cases')

if __name__=='__main__':main()
