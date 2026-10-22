"""Original BRT/BGT geometry, request parameters and paginated batches, offline."""
from pathlib import Path
import importlib,json,os,random
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_PROFILE_CONTEXT_FIXTURES',ROOT/'rust/tests/fixtures/profile-context'))
SEED=11473
COUNT=24
def main():
    ref=reference();models=importlib.import_module(ref.__package__+'.models')
    rng=random.Random(SEED);cases=[]
    for kind,module,cls in [('roads','road_centerline','RoadCenterlineClient'),('terrain','bgt_terrain_boundary','BgtTerrainBoundaryClient')]:
        Client=getattr(importlib.import_module(ref.__package__+'.'+module),cls)
        for n in range(COUNT):
            client=Client();bounds=models.Bounds(155000.+n*.0011,463000.-n*.0017,155130.+n*.0013,463130.+n*.0019);pages=[]
            for p in range(1+n%3):
                vertices=[[155000.+rng.uniform(-50,50),463000.+rng.uniform(-50,50),rng.uniform(-4,4)] for _ in range(2+n%5)]
                vertices[0][0]=str(vertices[0][0]);vertices[1][1]=str(vertices[1][1])
                if kind=='roads':geometry={'type':'LineString' if n%2 else 'MultiLineString','coordinates':vertices if n%2 else [vertices,vertices[::-1],[]]}
                else:geometry={'type':'Polygon' if n%2 else 'MultiPolygon','coordinates':[vertices,vertices[::-1],[]] if n%2 else [[vertices,vertices[::-1]],[vertices]]}
                href=f'{client.BASE_URL}/collections/{client.COLLECTION_ID}/items?offset={p+1}'
                pages.append(dict(features=[None,{'geometry':None},{'geometry':geometry},{'geometry':{'type':'Point','coordinates':[1,2]}}],links=[dict(rel='NEXT',href=href)] if p< n%3 else []))
            calls=[]
            def get(url,params):
                calls.append(dict(url=url,params=params));return pages[len(calls)-1]
            client._get_json=get
            paths=client.fetch_paths(bounds);again=client.fetch_paths(bounds);assert again==paths
            cases.append(dict(kind=kind,bounds=dict(min_x=bounds.min_x,min_y=bounds.min_y,max_x=bounds.max_x,max_y=bounds.max_y),pages=pages,calls=calls,paths=paths))
    result=dict(cases=cases);OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original BRT/BGT complete request/geometry/page/cache contracts / {sum(len(c["calls"]) for c in cases)} original requests, without network')
    return result
if __name__=='__main__':main()
