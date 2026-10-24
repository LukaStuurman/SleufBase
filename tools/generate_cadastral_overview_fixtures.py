"""Original standalone cadastral export envelope and offline option oracles.

Imports helpers only; never starts the application, settings or accounts.
"""
from pathlib import Path
from dataclasses import asdict
import importlib,json,os,tempfile
import ezdxf
from PIL import Image,ImageDraw
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_CADASTRAL_OVERVIEW_FIXTURES',ROOT/'rust/tests/fixtures/cadastral-overview'))

def main():
    ref=reference();pkg=ref.__package__;mod=importlib.import_module(pkg+'.cadastral_export');models=importlib.import_module(pkg+'.models')
    exp=mod.CadastralDxfExporter(None);OUT.mkdir(parents=True,exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    envelope=ezdxf.new('R2018',setup=True);envelope.units=6
    exp._setup_layers(envelope,(0,0,0),(0,0,0));exp._setup_text_styles(envelope)
    envelope.classes.add_required_classes(envelope.dxfversion)
    envelope.classes.classes=type(envelope.classes.classes)(sorted(envelope.classes.classes.items()))
    base=OUT/'base.dxf';envelope.saveas(base)
    base_target=ROOT/'rust/src/cadastral_overview_base.txt';base_bytes=base.read_bytes().replace(b'\r\n',b'\n')
    if not base_target.exists() or base_target.read_bytes()!=base_bytes:base_target.write_bytes(base_bytes)
    cases=[]
    transforms=[dict(a=.7,b=.12,c=155000.,d=.08,e=-.6,f=463020.),dict(a=.8,b=0.,c=155014.,d=0.,e=-.7,f=463014.),dict(a=.5,b=0.,c=155002.,d=0.,e=-.5,f=463024.)]
    specs=[]
    for i,tx in enumerate(transforms):
        im=Image.new('RGBA',(40,32));draw=ImageDraw.Draw(im)
        if i==0:
            draw.polygon([(7,5),(30,5),(30,12),(25,12),(25,26),(7,26)],fill=(195,207,211,255));draw.rectangle((11,10,18,23),fill=(45,45,45,255));draw.rectangle((1,1,2,2),fill=(9,19,29,255))
        elif i==1:draw.rectangle((5,3,31,27),fill=(140,140,140,255));draw.rectangle((13,8,20,22),fill=(20,20,20,255))
        # Third source deliberately has no visible alpha, exercising fallback.
        path=OUT/(['fase_2_ps_00021_66587.tiff','PS-21_same.tiff','zondernummer.tiff'][i]);im.save(path,format='TIFF')
        transform=models.GeoTransform(**tx);corners=[transform.pixel_to_world(x,y) for x,y in [(0,0),(im.width,0),(im.width,im.height),(0,im.height)]]
        bounds=models.Bounds(min(p[0] for p in corners),min(p[1] for p in corners),max(p[0] for p in corners),max(p[1] for p in corners))
        metadata={'template_proefsleuf_label':' ps007b '} if i==1 else {}
        specs.append(dict(path=str(path.resolve()),transform=tx,bounds=asdict(bounds),metadata=metadata))
    linework=[dict(layer_name='KAD_GRENS',paths=[[(154980.,463030.),(155080.,463030.)],[(155001.,462990.),(155001.,463040.)]]),dict(layer_name='BGT_WEGDEEL',paths=[[(154990.,462995.),(155040.,462995.)]]),dict(layer_name='BGT_VECTOR_OMTREK',paths=[[(154995.,463040.),(155040.,463040.)]])]
    labels=[dict(layer_name='KAD_STRAATNAAM',text='Straat Ø',position=[155004.,463028.],rotation=37.5),dict(layer_name='KAD_HUISNUMMER',text='12A',position=[155008.,463018.],rotation=-23.)]
    for mode in ['polygon','centerline','none','centerline-pca']:
        for images in [False,True]:
            name=mode+('-tiffs' if images else '-plain');resolved_mode='centerline' if mode=='centerline-pca' else mode;groups=[] if mode=='centerline-pca' else linework
            settings=dict(trench_mode=resolved_mode,include_tiff_images=images,label_gap=1.25,centerline_color=[17,141,207],label_color=[210,31,84])
            request=dict(layers=[dict(s,path=Path(s['path']).name) for s in specs],linework=groups,labels=labels,settings=settings)
            layers=[models.GeoTiffLayer(Path(s['path']),Image.open(s['path']),models.GeoTransform(**s['transform']),models.Bounds(**s['bounds']),28992,metadata=s['metadata']) for s in specs]
            combined=exp._combined_bounds(layers);bounds=combined.padded(exp._overview_padding(combined));output=OUT/(name+'.dxf')
            from types import SimpleNamespace
            with tempfile.TemporaryDirectory(prefix='SleufBase-original-overview-') as temp:
                fresh=Path(temp)/(name+'.dxf')
                exp._write_dxf(fresh,layers,[SimpleNamespace(**g) for g in groups],[SimpleNamespace(**g) for g in labels],bounds,resolved_mode,images,settings['label_gap'],tuple(settings['centerline_color']),tuple(settings['label_color']))
                written=ezdxf.readfile(fresh)
            # Keep the committed oracle portable; copies are checked against
            # fixture sources independently, never against a developer's path.
            for definition in written.objects.query('IMAGEDEF'):definition.dxf.filename=Path(definition.dxf.filename).name
            written.classes.add_required_classes(written.dxfversion)
            written.classes.classes=type(written.classes.classes)(sorted(written.classes.classes.items()))
            written.saveas(output)
            for layer in layers:layer.image.close()
            cases.append(dict(name=name,request=request))
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('8 original standalone cadastral overview option combinations, affine TIFFs, PCA/projection masks, labels and colors')
if __name__=='__main__':main()
