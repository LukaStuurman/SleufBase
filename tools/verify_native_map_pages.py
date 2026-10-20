"""Exact raster and original page-plan audit; real native template CLI check.

Font hinting/contours are deliberately not called pixel-identical to FreeType.
HarfBuzz glyph/position oracles have a separate Rust regression test.
"""
from pathlib import Path
from copy import deepcopy
import importlib,json,math,shutil,subprocess,sys,tempfile
import ezdxf
from PIL import Image
import numpy as np
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
FIXTURE=ROOT/'rust/tests/fixtures/map-export'
def equal_pixels(a,b,name):
    assert a.size==b.size,(name,a.size,b.size)
    actual=np.array(a.convert('RGBA'));expected=np.array(b.convert('RGBA'))
    assert np.array_equal(actual,expected),(name,int(np.count_nonzero(np.any(actual!=expected,axis=2))))
    return a.width*a.height
def main():
    out=Path(sys.argv[1]);cases=json.loads((FIXTURE/'reference.json').read_text(encoding='utf-8'));native=json.loads((out/'audit.json').read_text(encoding='utf-8'))
    pixels=0
    for c in cases['resizes']+cases['paints']:
        with Image.open(out/c['expected']) as a,Image.open(FIXTURE/c['expected']) as b:pixels+=equal_pixels(a,b,c['name'])
    assert len(native['pages'])==len(cases['pages'])
    for c,n in zip(cases['pages'],native['pages']):
        assert n==dict(name=c['name'],layout=c['layout'],ops=c['ops']),c['name']
    assert len(native['texts'])==len(cases['texts'])
    for c,n in zip(cases['texts'],native['texts']):
        assert all(n[k]==c[k] for k in ['text','size','bold'])
        b=n['bbox'];assert all(math.isfinite(x) for x in b) and b[2]>b[0] and b[3]>b[1],n
    if len(sys.argv)>2:verify_cli(Path(sys.argv[2]).resolve())
    print(f'{len(cases["resizes"])} resizes + {len(cases["paints"])} paints: {pixels:,} exact RGBA pixels; {len(cases["pages"])} exact page plans; {len(cases["texts"])} native font bounds checked')
def verify_cli(binary):
    with tempfile.TemporaryDirectory(prefix='sleufbase-map-cli-') as folder:
        temp=Path(folder);r=deepcopy(next(c['request'] for c in json.loads((ROOT/'rust/tests/fixtures/template-export/reference.json').read_text(encoding='utf-8')) if c['name']=='native-tiff'))
        r['slots']=r['slots'][:1];s=r['slots'][0];s['bounds']=dict(min_x=0.,min_y=0.,max_x=11.,max_y=6.)
        shutil.copyfile(ROOT/r['template'],temp/'template.dxf');r['template']='template.dxf'
        shutil.copyfile(ROOT/s['tiff_source']['path'],temp/'input.tiff');s['tiff_source']['path']='input.tiff';shutil.copyfile(FIXTURE/'Hack-Regular.ttf',temp/'font.ttf')
        s.pop('map_png');s['map_source']=dict(background=dict(kind='solid',color=[123,156,189,255]),fonts=dict(regular='font.ttf',bold='font.ttf'))
        r['settings'].update(project='Kaartproject Ø',export_date=[2026,10,20]);manifest=temp/'request.json';manifest.write_text(json.dumps(r,ensure_ascii=False),encoding='utf-8');output=temp/'export.dxf'
        call=lambda:subprocess.run([str(binary),'--template-dxf',str(output),str(manifest)],cwd=temp,capture_output=True,text=True,encoding='utf-8')
        done=call();assert done.returncode==0,(done.stdout,done.stderr)
        doc=ezdxf.readfile(output);audit=doc.audit();assert not audit.has_errors,audit.errors
        title_values=[(a.dxf.tag,a.dxf.text) for e in doc.layouts.get('Blad1') if e.dxftype()=='INSERT' for a in e.attribs]
        assert ('DATUM1','20-10-2026') in title_values,title_values
        assert any(tag in ['GEM-PROJECT','PROJECTOMSCHRIJVING','PROJECTNAAM'] and value=='Kaartproject Ø' for tag,value in title_values),title_values
        images=[doc.entitydb[e.dxf.image_def_handle].dxf.filename for e in doc.modelspace().query('IMAGE')]
        maps=[Path(p) for p in images if Path(p).name=='slot00001_kaart.png'];assert len(maps)==1
        with Image.open(maps[0]) as native:
            native=native.convert('RGBA');assert native.size==(3508,2480)
            ref=reference();exp=importlib.import_module(ref.__package__+'.exporting');models=importlib.import_module(ref.__package__+'.models');renderer=importlib.import_module(ref.__package__+'.renderer').MapRenderer()
            exporter=exp.MapExporter(None,renderer);bounds=models.Bounds(**s['bounds']);view=exporter._determine_map_bounds(bounds.padded(1.1));transform=models.ViewportTransform(view,exporter.map_width_px,exporter.map_height_px)
            with Image.open(temp/'input.tiff') as raw:
                raw=raw.convert('RGBA');geo=models.GeoTransform(**s['tiff_source']['transform']);corners=[geo.pixel_to_world(x,y) for x,y in [(0,0),(raw.width,0),(raw.width,raw.height),(0,raw.height)]]
                layer=models.GeoTiffLayer(Path('input.tiff'),raw,geo,models.Bounds(min(x for x,y in corners),min(y for x,y in corners),max(x for x,y in corners),max(y for x,y in corners)),28992)
                prepared=exp._prepare_export_tiff_layer(layer,forced_opacity=1.);canvas=Image.new('RGBA',(exporter.map_width_px,exporter.map_height_px),(123,156,189,255));renderer._paint_tiff(canvas,transform,prepared)
                # This interior rectangle contains the full TIFF colors/alpha,
                # and no page annotations, glyphs or CAD line edges.
                a=transform.world_to_screen(1.,5.);b=transform.world_to_screen(6.,1.);roi=tuple(map(int,[a[0]+2,a[1]+2,b[0]-2,b[1]-2]));page_roi=(roi[0]+82,roi[1]+82,roi[2]+82,roi[3]+82)
                count=equal_pixels(native.crop(page_roi),canvas.crop(roi),'CLI full TIFF map interior');assert count>40_000
                assert len(set(canvas.crop(roi).getdata()))>20
        old=output.read_bytes();again=call();assert again.returncode!=0 and output.read_bytes()==old
        print(f'CLI template with native map/profile/date/project: DXF audit passed; {count:,} map-interior pixels match original')
if __name__=='__main__':main()
