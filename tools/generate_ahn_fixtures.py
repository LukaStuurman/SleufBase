"""Independent original AHN/Pillow oracle. Never instantiate the desktop app."""
import importlib.util,io,json,os,random
from pathlib import Path
from PIL import Image,TiffImagePlugin
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_AHN_FIXTURES',ROOT/'rust/tests/fixtures/ahn'))
def main():
    spec=importlib.util.spec_from_file_location('original_ahn',ROOT/'ahn.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    client=module.PdokAhnClient();rng=random.Random(819214);OUT.mkdir(parents=True,exist_ok=True)
    cases=[]
    for n in range(96):
        w,h=rng.randrange(1,14),rng.randrange(1,14)
        nodata=[None,'3.4028234663852886e38','-9999','nan','nonsense','0','9999'][n%7]
        mode='I' if n%3==0 else 'F'
        values=[rng.randrange(-5000,5000) if mode=='I' else rng.uniform(-20,150) for _ in range(w*h)]
        no=float(nodata) if nodata not in [None,'nonsense'] else client.DEFAULT_NODATA
        if n%5==0:values=[no if mode=='F' else -9999]*len(values)
        elif mode=='F':
            for i in range(0,len(values),3):values[i]=no
        image=Image.new(mode,(w,h));image.putdata(values);tags=TiffImagePlugin.ImageFileDirectory_v2()
        if nodata is not None:tags[42113]=nodata
        path=OUT/f'sample-{n:03}.tif';image.save(path,tiffinfo=tags)
        with Image.open(path) as decoded:value=client._nearest_valid_value(decoded)
        cases.append(dict(path=path.name,expected=round(value,3) if value is not None else None))
        image.close()
    # Direct binary-float ties where multiply-then-round gives the wrong answer.
    rounding=[dict(value=v,expected=round(v,3)) for v in [1.2345,1.2355,2.6755,-1.2345,0.0005,-0.0005,22.0005,100.1235]]
    cache=[dict(x=x,y=y,key=list(client._cache_key(x,y)[:2])) for x,y in [(1.005,2.675),(-0.0,0.0),(168000.005,401000.005),(0.125,1.225)]]
    (OUT/'reference.json').write_text(json.dumps(dict(cases=cases,rounding=rounding,cache=cache),separators=(',',':'))+'\n',encoding='utf8')
    print(f'{len(cases)} original AHN TIFF/nodata/nearest/rounding samples')
if __name__=='__main__':main()
