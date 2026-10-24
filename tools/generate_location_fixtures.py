"""Original location/search contracts; no application, GUI or account startup."""
from pathlib import Path
from types import SimpleNamespace,CodeType,FunctionType
import ast,importlib.util,json,math,os,random,re,sys

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_LOCATION_FIXTURES',ROOT/'rust/tests/fixtures/location'))
SEED=8521
COUNT=96

def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);sys.modules[name]=value;spec.loader.exec_module(value)
    return value

def main():
    from pyproj import network
    network.set_network_enabled(False)
    client_type=module('original_location',ROOT/'location_search.py').PdokLocationClient
    tree=ast.parse((ROOT/'app.py').read_text(encoding='utf-8'))
    fn=next(n for n in ast.walk(tree) if isinstance(n,ast.FunctionDef) and n.name=='_parse_inline_coordinate_query')
    env={'re':re};exec(compile(ast.Module(body=[fn],type_ignores=[]),'app.py::_parse_inline_coordinate_query','exec'),env)
    workspace=SimpleNamespace(location_client=client_type())
    parse=lambda text:env[fn.name](workspace,text)
    helper=module('original_bytecode_helper',ROOT/'legacy_bytecode.py')
    root=helper.read_validated_code('app',ROOT/'legacy_bytecode.py')
    cls=next(c for c in root.co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    zoom=FunctionType(next(c for c in cls.co_consts if isinstance(c,CodeType) and c.co_name=='_zoom_for_location_type'),{})
    queries=['','x','Utrecht','RD 155000 463000','155000,463000','155000,25; 463000,75',
        '999 2000','1000 1000','-155000 463000','x=155000 y=463000 z=8','GPS 52.15517440 5.38720621',
        '52.15517440, 5.38720621','5.38720621 52.15517440','WGS84 52.1 5.2','epsg:4326 52.1 5.2',
        'gps lat: 52,1 lon: 5,2','wgs 84 5.2 52.1','GPS 5.2 52.1','0 0','gps 0 0',
        'wgs 90 180','GPS -33.9 151.2','GPS 52.1 5.2 7','RD ١٥٥٠٠٠ ４６３０００','52..1 5.2',
        'GPS 51.125 5.125','RD 155000.125 463000.125','GPS lon 5.2 lat 52.1','GPS lat 5.2 lon 52.1']
    rng=random.Random(SEED)
    for _ in range(COUNT):
        lat=rng.uniform(50.7,53.6);lon=rng.uniform(3.2,7.3)
        queries += [f'GPS {lat:.12f} {lon:.12f}',f'{lon:.12f}; {lat:.12f}']
    # Every decimal alphabet used by Python 3.11, including mixed scripts.
    import unicodedata
    zeros=[chr(n) for n in range(sys.maxunicode+1) if unicodedata.category(chr(n))=='Nd' and unicodedata.decimal(chr(n))==0]
    for zero in zeros:
        queries.append('RD '+''.join(chr(ord(zero)+int(c)) if c.isdecimal() else c for c in '155000.25 463000.75'))
    coordinates=[]
    for query in queries:
        value=parse(query);rd=client_type.parse_rd_input(query);legacy=value
        if rd is None:
            raw=re.sub(r'(?i)\bwgs\s*84\b|\bepsg\s*:?\s*4326\b',' ',query)
            values=[float(m.replace(',','.')) for m in re.findall(r'-?\d+(?:[.,]\d+)?',raw)]
            if len(values)>=2:
                a,b=values[:2];axes=list(re.finditer(r'\b(lat(?:itude)?|lon(?:gitude)?|lng)\b',query.casefold()))
                lat=next((m.start() for m in axes if m.group().startswith('lat')),None)
                lon=next((m.start() for m in axes if not m.group().startswith('lat')),None)
                if lat is not None and lon is not None:
                    latitude,longitude=(a,b) if lat<lon else (b,a)
                    value=parse(f'GPS {latitude:.17g} {longitude:.17g}')
                elif 2.5<=a<=8 and 50<=b<=54.5:value=parse(f'GPS {b:.17g} {a:.17g}')
        result=dict(query=query,rd=rd)
        if value!=legacy:result['legacy_coordinate']=legacy
        if value is None:result['coordinate']=None
        elif not all(math.isfinite(v) for v in value[1:3]):result.update(coordinate=None,legacy_nonfinite=True)
        else:
            label,x,y,kind=value;result['coordinate']=dict(label=label,x=x,y=y,location_type=kind)
        coordinates.append(result)
    payloads=[{},dict(response={}),dict(response=dict(docs=[])),dict(response=dict(docs=[
        None,2,'invalid',{},dict(weergavenaam='Adres Ø',centroide_rd='POINT(155000.125 463000.75)',type='adres'),
        dict(centroide_rd='POINT(-20 0) trailing',type='weg'),dict(centroide_rd='POINT(1.0\t2.5)'),
        dict(centroide_rd='POINT(1 2)',weergavenaam=None,type=3),dict(centroide_rd='LINESTRING(1 2)')]))]
    for _ in range(32):
        payloads.append(dict(response=dict(docs=[dict(weergavenaam=f'Fictieve locatie {i}',type=rng.choice(['adres','weg','woonplaats']),
            centroide_rd=f'POINT({rng.uniform(1000,250000):.12f} {rng.uniform(300000,620000):.12f})') for i in range(rng.randrange(1,15))])))
    searches=[];reverses=[]
    for index,payload in enumerate(payloads):
        client=client_type();calls=[]
        client._request_json=lambda url,params:(calls.append(dict(url=url,params=params)) or payload)
        query=' Utrecht Ø + &? ';rows=[-7,1,6,99][index%4]
        output=[vars(v) for v in client.search(query,rows)]
        searches.append(dict(query=query,rows=rows,payload=payload,output=output,request=calls[-1]))
    for values in [[],[None,{},dict(weergavenaam='Straat Ø',type='adres',afstand='1.25'),dict(afstand='bad'),dict(afstand=True),dict(afstand=None),dict(afstand=[2])]]:
        payload=dict(response=dict(docs=values));client=client_type();calls=[]
        client._request_json=lambda url,params:(calls.append(dict(url=url,params=params)) or payload)
        output=[vars(v) for v in client.reverse_lookup(155000.1254,463000.7506,99)]
        reverses.append(dict(payload=payload,output=output,request=calls[-1]))
    result=dict(coordinates=coordinates,searches=searches,reverses=reverses,
        zooms={kind:(.2 if kind in ('rd','wgs84') else zoom(None,kind)) for kind in ['rd','wgs84','adres','weg','woonplaats','other']})
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,allow_nan=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(coordinates)} original coordinate queries / {len(zeros)} decimal alphabets / {len(searches)} search responses / {len(reverses)} reverse responses / 6 actual map scales; {sum(c.get("legacy_nonfinite",False) for c in coordinates)} explicit nonfinite guards')
    return result

if __name__=='__main__':main()
