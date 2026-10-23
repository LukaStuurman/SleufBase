"""Fresh original plans, complete pixels and actual standalone PNG exports."""
from pathlib import Path
from dataclasses import asdict
from tempfile import TemporaryDirectory
from unittest.mock import patch
import importlib,json,os,random,subprocess,sys
from PIL import Image,ImageFont
import generate_cadastral_tile_fixtures as oracle
ROOT=Path(__file__).resolve().parents[1]

def compare(cases,answers,root):
    pixels=0;bugs=0
    for case,actual in zip(cases,answers,strict=True):
        expected_plan=json.loads(json.dumps(case['plan']))
        crop=expected_plan['crop']
        if crop[0]==crop[2] or crop[1]==crop[3]:
            assert 'error' in actual,actual;bugs+=1;continue
        assert 'error' not in actual,actual
        for key in ['zoom','resolution','tile_span','min_tile_x','max_tile_x','min_tile_y','max_tile_y','stitched_size','target_size','crop']:
            assert actual['plan'][key]==expected_plan[key],(key,actual['plan'][key],expected_plan[key])
        for key,value in case['plan']['mercator_bounds'].items():assert abs(actual['plan']['mercator_bounds'][key]-value)<2e-6
        assert [u.split('/Kadastralekaart/')[1] for u in actual['urls']]==[u.split('/Kadastralekaart/')[1] for u in case['urls']]
        with Image.open(root/case['image']) as image:assert bytes(actual['pixels'])==image.convert('RGBA').tobytes(),case['image'];pixels+=image.width*image.height
    return pixels,bugs

def standalone(driver,root):
    ref,models,client=oracle.original();exp=importlib.import_module(ref.__package__+'.exporting');renderer=importlib.import_module(ref.__package__+'.renderer').MapRenderer()
    data=json.loads((ROOT/'rust/tests/fixtures/map-batch/reference.json').read_text(encoding='utf-8'));font=ROOT/'rust/tests/fixtures/map-export/Hack-Regular.ttf';samples=0
    for n,c in enumerate(data['cases'][:3]):
        source=ROOT/'rust/tests/fixtures/map-batch'/c['path'];cache_root=root/f'appdata-{n}';cache=cache_root/'SleufBase/Rust/cadastral-tiles';cache.mkdir(parents=True)
        salt=83+n*11
        class Provider:
            def fetch_map(self,bounds,size):
                plan,image=oracle.render(client,bounds,size,salt)
                for x,y in plan['tile_coords']:
                    with oracle.tile(plan['zoom'],x,y,salt) as tile:tile.save(cache/f'{plan["zoom"]}_{x}_{y}.png')
                return image
        tx=models.GeoTransform(**c['transform']);bounds=models.Bounds(**c['bounds'])
        with Image.open(source) as image:
            layer=models.GeoTiffLayer(source,image.copy(),tx,bounds,28992)
        exporter=exp.MapExporter(Provider(),renderer,dpi=c['dpi'],scale=c['scale'])
        expected_dir=root/f'original-{n}';expected_dir.mkdir()
        with patch.object(exp,'_load_font',lambda size,bold=False:ImageFont.truetype(str(font),size)):
            path=exporter.export_single(expected_dir,layer,[])
        layer.image.close()
        request=dict(rasters=[dict(path=str(source),width=53,height=37,transform=c['transform'],bounds=c['bounds'],epsg=28992,kickthemap_job_id=None,kickthemap_job_title='',kickthemap_job_sidecar=None)],settings=dict(dpi=c['dpi'],scale=c['scale'],background='cadastral'),fonts=dict(regular=str(font),bold=str(font)))
        manifest=root/f'map-{n}.json';manifest.write_text(json.dumps(request),encoding='utf-8');out=root/f'native-{n}'
        env=dict(os.environ,LOCALAPPDATA=str(cache_root))
        subprocess.run([driver,str(manifest),str(out)],check=True,timeout=120,capture_output=True,env=env)
        with Image.open(path) as expected,Image.open(out/path.name) as actual:
            assert expected.mode==actual.mode=='RGB' and expected.size==actual.size
            assert all(abs(a-b)<1e-9 for a,b in zip(expected.info['dpi'],actual.info['dpi']))
            rng=random.Random(8913+n)
            for _ in range(600):
                x=82+rng.randrange(exporter.map_width_px//5,exporter.map_width_px*3//5)
                y=82+rng.randrange(exporter.map_height_px*3//10,exporter.map_height_px*3//5)
                assert expected.getpixel((x,y))==actual.getpixel((x,y)),(n,x,y,expected.getpixel((x,y)),actual.getpixel((x,y)))
                samples+=1
    # Finish one real page, then fail a later uncached location through a closed
    # local proxy. No external provider is contacted by this failure scenario.
    first=json.loads((root/'map-0.json').read_text(encoding='utf-8'))
    second=json.loads((root/'map-1.json').read_text(encoding='utf-8'))['rasters'][0]
    second['transform']['c']+=10000.;second['bounds']['min_x']+=10000.;second['bounds']['max_x']+=10000.
    first['rasters'].append(second);manifest=root/'late-network-failure.json';manifest.write_text(json.dumps(first),encoding='utf-8')
    out=root/'previous-output';out.mkdir();previous={out/'input-0_kaart.png':b'previous first PNG',out/'input-1_kaart.png':b'previous second PNG'}
    for path,content in previous.items():path.write_bytes(content)
    env=dict(os.environ,LOCALAPPDATA=str(root/'appdata-0'),HTTPS_PROXY='http://127.0.0.1:9',HTTP_PROXY='http://127.0.0.1:9',ALL_PROXY='http://127.0.0.1:9',NO_PROXY='')
    result=subprocess.run([driver,str(manifest),str(out)],timeout=120,capture_output=True,env=env)
    assert result.returncode!=0,'uncached failed Kadaster tile must fail the complete batch'
    assert all(path.read_bytes()==content for path,content in previous.items())
    assert set(out.iterdir())==set(previous),'temporary page/cache data leaked into output'
    print(f'3 actual Kadaster standalone RGB/DPI exports / {samples} original composited background/TIFF pixels and late-network-failure output preservation passed using isolated caches')

def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-cadastral-runtime-') as temp:
        root=Path(temp);oracle.OUT=root/'original';oracle.SEED=937711;oracle.COUNT=43
        cases=oracle.main()['cases'];request=root/'requests.json';request.write_text(json.dumps([c['request'] for c in cases]),encoding='utf-8');out=root/'answers.json'
        subprocess.run([driver,str(request),str(out)],check=True,timeout=120,capture_output=True)
        pixels,bugs=compare(cases,json.loads(out.read_text(encoding='utf-8')),oracle.OUT)
        print(f'{len(cases)} fresh original Kadaster WMTS plans / {pixels} complete RGBA pixels / {bugs} preserved empty-crop bugs rejected passed')
        if len(sys.argv)>2 and not sys.argv[2].startswith('--'):standalone(str(Path(sys.argv[2]).resolve()),root)
        if '--live' in sys.argv:live(driver,root)

def live(driver,root):
    # Explicit optional public-provider probe. Default/CI execution is offline.
    ref,models,client=oracle.original();bounds=models.Bounds(154940.,462955.,155060.,463045.);size=(256,192)
    cache=root/'live-cache';request=[dict(bounds=asdict(bounds),size=size,salt=0,cache=str(cache))]
    manifest=root/'live-request.json';manifest.write_text(json.dumps(request),encoding='utf-8');out=root/'live-answer.json'
    subprocess.run([driver,str(manifest),str(out)],check=True,timeout=120,capture_output=True)
    actual=json.loads(out.read_text(encoding='utf-8'))[0];assert 'error' not in actual,actual
    plan=client._prepare_tile_request(bounds,size);stitched=Image.new('RGBA',plan['stitched_size'],(243,243,243,255));snapshot={}
    for x,y in plan['tile_coords']:
        path=cache/f'{plan["zoom"]}_{x}_{y}.png';snapshot[path]=(path.read_bytes(),path.stat().st_mtime_ns)
        with Image.open(path) as tile:stitched.alpha_composite(tile.convert('RGBA'),((x-plan['min_tile_x'])*256,(y-plan['min_tile_y'])*256))
    expected=client._render_tile_request(plan,stitched);stitched.close()
    assert expected.tobytes()==bytes(actual['pixels'])
    expected.close();again=root/'live-cache-answer.json'
    env=dict(os.environ,HTTPS_PROXY='http://127.0.0.1:9',HTTP_PROXY='http://127.0.0.1:9',ALL_PROXY='http://127.0.0.1:9',NO_PROXY='')
    subprocess.run([driver,str(manifest),str(again)],check=True,timeout=120,capture_output=True,env=env)
    assert json.loads(again.read_text(encoding='utf-8'))[0]==actual
    assert all((path.read_bytes(),path.stat().st_mtime_ns)==content for path,content in snapshot.items())
    print(f'Live PDOK Kadaster: {len(snapshot)} real tiles / {size[0]*size[1]} pixels match original; isolated fresh-cache replay without network passed')
if __name__=='__main__':main()
