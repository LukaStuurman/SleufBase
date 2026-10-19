"""Original per-tile BGT decoding/RD and grid references; no live requests."""
from pathlib import Path
import importlib
import json
import random
from importlib.metadata import version
import mapbox_vector_tile
from mapbox_vector_tile.Mapbox import vector_tile_pb2 as pb
from shapely.geometry import Polygon,MultiPolygon,LineString,MultiLineString,Point,MultiPoint,box
from generate_object_export_fixtures import reference
from generate_bgt_surface_fixtures import native
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'rust/tests/fixtures/bgt-tiles'

def main():
    assert version('mapbox-vector-tile')=='2.2.0'
    ref=reference();module=importlib.import_module(ref.__package__+'.bgt_vector_tiles');Bounds=importlib.import_module(ref.__package__+'.models').Bounds
    client=module.BgtVectorTileClient();OUT.mkdir(parents=True,exist_ok=True)
    cases=[]
    def case(name,data,row,col):
        path=OUT/(name+'.pbf');path.write_bytes(data)
        decoded=mapbox_vector_tile.decode(data);client._decoded_tile=lambda *args:decoded
        paths=client._tile_features(row,col);surfaces=client._tile_surface_features(row,col)
        cases.append(dict(name=name,path=path.relative_to(ROOT).as_posix(),id=dict(row=row,col=col),features=[dict(layer_name=n,feature_id=i,geometry=native(g)) for n,i,g in paths],surfaces=[dict(layer_name=f.layer_name,feature_id=f.feature_id,physical_appearance=f.physical_appearance,geometry=native(f.geometry)) for f in surfaces]))
    real=OUT/'pdok-12-2420-2075.pbf';assert real.exists(),'Recorded PDOK fixture required; generation does not use network'
    case('pdok-12-2420-2075',real.read_bytes(),2420,2075)
    shapes=[Point(0,0),MultiPoint([(1,2),(3,4)]),LineString([(-100,-100),(50,50),(5000,5000)]),MultiLineString([[(0,0),(10,0)],[(0,10),(10,10)]]),Polygon(box(20,30,300,400).exterior.coords,[box(40,60,80,90).exterior.coords]),MultiPolygon([box(500,500,600,600),box(700,700,800,800)]),Polygon([(0,0),(100,0),(200,0)])]
    for i,extent in enumerate([4096,256,8192,1,0]):
        features=[dict(geometry=g,properties=dict(lokaal_id='' if j%3 else f'L:{j}',external_fid=0 if j%2 else f'E:{j}',fysiek_voorkomen=['Asfalt','',False,0,' Klinkers ','Straße',3.5][j]),id=0 if j%2 else 12345678901234567890) for j,g in enumerate(shapes)]
        data=mapbox_vector_tile.encode([dict(name=' WEGDEEL ',features=features),dict(name=' BUURT ',features=features),dict(name='Straße',features=features),dict(name='STRASSE',features=features)],default_options=dict(extents=extent or 4096,check_winding_order=True))
        if extent==0:
            raw=pb.tile();raw.ParseFromString(data)
            for layer in raw.layers:layer.extent=0
            data=raw.SerializeToString(deterministic=True)
        case('shapes-'+str(extent),data,0 if i%2 else 4095,4095 if i%2 else 0)
    # Exercise all seven protobuf values and their explicit Python precedence.
    tile=pb.tile();layer=tile.layers.add();layer.name='types';layer.extent=4096;layer.version=2;layer.keys.extend(['lokaal_id','external_fid','fysiek_voorkomen'])
    values=[('string_value','Ø 漢😀'),('float_value',.1),('double_value',1.2345678901234567),('int_value',-9223372036854775808),('uint_value',18446744073709551615),('sint_value',-99),('bool_value',True),('bool_value',False),('double_value',float('nan')),('double_value',float('inf')),('double_value',-float('inf'))]
    for i,(field,value) in enumerate(values):
        v=layer.values.add();setattr(v,field,value)
        f=layer.features.add();f.type=2;f.geometry.extend([9,0,0,10,20,20]);f.tags.extend([0,i,2,i])
    value=layer.values.add();value.bool_value=False;value.double_value=123.5
    f=layer.features.add();f.type=2;f.geometry.extend([9,0,0,10,20,20]);f.tags.extend([0,len(values)])
    case('value-types',tile.SerializeToString(deterministic=True),2000,2000)
    # Unpacked uint32 fields and unknown protobuf extensions remain compatible.
    def varint(n):
        out=bytearray()
        while n>127:out.append((n&127)|128);n>>=7
        out.append(n);return bytes(out)
    def atom(n,v):return varint(n<<3)+varint(v)
    def blob(n,v):return varint((n<<3)|2)+varint(len(v))+v
    raw_layer=blob(1,layer.name.encode('utf-8'))
    for f in layer.features:
        raw_feature=atom(1,f.id)+b''.join(atom(2,v) for v in f.tags)+atom(3,f.type)+b''.join(atom(4,v) for v in f.geometry)
        raw_layer+=blob(2,raw_feature)
    raw_layer+=b''.join(blob(3,k.encode('utf-8')) for k in layer.keys)+b''.join(blob(4,v.SerializeToString(deterministic=True)) for v in layer.values)
    extensions=atom(100,18446744073709551615)+varint((101<<3)|3)+atom(102,3)+varint((101<<3)|4)
    case('unpacked-extensions',extensions+blob(3,raw_layer),2048,2048)
    duplicate=pb.tile();a=duplicate.layers.add();a.CopyFrom(layer);a.name='A'
    b=duplicate.layers.add();b.CopyFrom(layer);b.name='B'
    a=duplicate.layers.add();a.CopyFrom(layer);a.name='A';a.extent=8192
    case('duplicate-layers',duplicate.SerializeToString(deterministic=True),2000,2000)
    # Final ring without ClosePath; Shapely closes after decoding.
    data=mapbox_vector_tile.encode(dict(name='wegdeel',features=[dict(geometry=box(10,10,100,100),properties=dict(fysiek_voorkomen='gras'))]))
    tile=pb.tile();tile.ParseFromString(data);tile.layers[0].features[0].geometry.pop() # final ring without ClosePath; Shapely closes after decoding
    case('unclosed-ring',tile.SerializeToString(deterministic=True),2400,2200)
    rng=random.Random(945376)
    for i in range(16):
        features=[]
        for j in range(12):
            x=rng.randrange(-256,4000);y=rng.randrange(-256,4000);w=rng.randrange(2,500);h=rng.randrange(2,500)
            g=box(x,y,x+w,y+h) if j%2 else Polygon(box(x,y,x+w,y+h).exterior.coords,[box(x+w*.25,y+h*.25,x+w*.75,y+h*.75).exterior.coords])
            features.append(dict(geometry=g,properties={'fysiek_voorkomen':rng.choice(['Asfalt','Gras',' Klinkers ','']), 'lokaal_id':f'{i}:{j}'},id=j))
        data=mapbox_vector_tile.encode(dict(name='begroeidterreindeel',features=features))
        case('random-'+str(i),data,rng.randrange(4096),rng.randrange(4096))
    grids=[]
    for i in range(90):
        col=rng.randrange(4096);row=rng.randrange(4096);x=client.ORIGIN_X+col*client.TILE_SPAN_METERS;y=client.ORIGIN_Y-row*client.TILE_SPAN_METERS
        for width,height in [(0,0),(1e-8,1e-8),(client.TILE_SPAN_METERS,client.TILE_SPAN_METERS),(500,300)]:
            b=Bounds(x,y-height,x+width,y)
            grids.append(dict(bounds=dict(min_x=b.min_x,min_y=b.min_y,max_x=b.max_x,max_y=b.max_y),expected=[dict(row=r,col=c) for r,c in client._tiles_for_bounds(b)]))
    result=dict(cases=cases,grids=grids,provenance=dict(file=real.name,url='https://api.pdok.nl/lv/bgt/ogc/v1/tiles/NetherlandsRDNewQuad/12/2420/2075?f=mvt',recorded='2026-10-19',description='Public BGT geometry near Eindhoven; fixed offline snapshot'))
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'original tile cases;',sum(len(c['features']) for c in cases),'RD features;',sum(len(c['surfaces']) for c in cases),'surface parts;',len(grids),'grid queries')

if __name__=='__main__':main()
