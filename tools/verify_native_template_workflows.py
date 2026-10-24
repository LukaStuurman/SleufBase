"""Read native worker files through independent ezdxf/Pillow.

The same exporter is exercised by actual egui click/lifecycle tests. This readback
checks published output, title fields, annotations, images and stable variants.
"""
from pathlib import Path
from datetime import datetime,timedelta
import importlib,json,sys,tempfile
from types import SimpleNamespace
from verify_native_discipline_exports import signature
from verify_native_template_reports import original_signature
import ezdxf
from PIL import Image
ROOT=Path(__file__).resolve().parents[1]

def selected_points(doc,images,angle=0.):
    from generate_object_export_fixtures import reference
    from generate_template_export_fixtures import original_dataset
    from verify_native_profile_leaders import full_signature
    from verify_native_template_images import compare
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export')
    importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch').install_dxf_template_pipeline_v9_patch()
    exporter=module.CadastralDxfExporter(wfs_client=None)
    request=next(c['request'] for c in json.loads((ROOT/'rust/tests/fixtures/template-export/reference.json').read_text(encoding='utf-8')) if c['name']=='native-tiff')
    dataset=original_dataset(ref,request['datasets'][0],(10.,0.));rules=tuple(ref.ObjectLayerRule(tuple(r['keywords']),r['target_layer'],r['color_aci'],r['label']) for r in request['rules'])
    profile=exporter._build_template_cross_section_profile(dataset,rules,[],[],.02,False)
    assert profile.start_point.x==10. and profile.end_point.x<0.
    metadata={'template_cross_section_start_point':{'x':10.,'y':0.},'template_reference_point':{'x':3.,'y':2.},'notes':'keep'}
    if angle:metadata['rotation_degrees']=angle
    expected=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf');baseline={e.dxf.handle for e in expected.modelspace()}
    exporter._setup_text_styles(expected)
    target=exporter._detect_template_slots(expected)[0]
    exporter._add_template_cross_section(expected,expected.modelspace(),target,SimpleNamespace(metadata=metadata),'PS12',profile,True,True,[],True,False,.02,None,'Ø')
    expected_entities=[full_signature(e) for e in expected.modelspace() if e.dxf.handle not in baseline]
    layers={e['layer'] for e in expected_entities}
    actual=[full_signature(e) for e in doc.modelspace() if e.dxf.handle not in baseline and e.dxf.layer in layers]
    # Original helpers produce tuples; normalize both semantic signatures to JSON.
    compare(json.loads(json.dumps(actual)),json.loads(json.dumps(expected_entities)),'selected start/reference written profile')
    models=importlib.import_module(ref.__package__+'.models');source=request['slots'][0]['tiff_source'];path=ROOT/source['path']
    with Image.open(path) as image,tempfile.TemporaryDirectory(prefix='SleufBase-selected-points-') as temp:
        tx=models.GeoTransform(**source['transform']);corners=[tx.pixel_to_world(x,y) for x,y in [(0,0),(image.width,0),(image.width,image.height),(0,image.height)]]
        bounds=models.Bounds(min(p[0] for p in corners),min(p[1] for p in corners),max(p[0] for p in corners),max(p[1] for p in corners))
        layer=models.GeoTiffLayer(path,image,tx,bounds,28992);layer.metadata.update(metadata)
        if angle:layer=importlib.import_module(ref.__package__+'.geotiff').set_geotiff_layer_rotation(layer,angle)
        prepared=exporter._build_template_tiff_raster(Path(temp),layer,'PS12',1,[],[],profile,False)
        native=next(p for p in images if p.name.endswith('_tiff.png'))
        with Image.open(prepared) as a,Image.open(native) as b:
            assert a.size==b.size and a.convert('RGBA').tobytes()==b.convert('RGBA').tobytes(),'selected start TIFF direction differs from original'
    print('Selected source points override stale slot metadata: complete written profile and all TIFF pixels match original')
def workbook(path,name):
    original=next(c['request'] for c in json.loads((ROOT/'rust/tests/fixtures/template-export/reference.json').read_text(encoding='utf-8')) if c['name']=='native-tiff')
    rows=[('PS12',None if name=='missing-measurements' else 0)]
    if name=='ordered-variants':rows.extend([None,('PS12B',0)])
    expected,_=original_signature(original['rules'],original['datasets'],rows)
    assert signature(path)==expected,(name,'automatic workbook differs from original')

def original_comments(name):
    from generate_object_export_fixtures import reference
    ref=reference();models=importlib.import_module(ref.__package__+'.models');exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    comments=[models.MapComment(x=2.,y=3.,text='Grondwater\n grondwater\nStraße\nSTRASSE')]
    if name=='tiff-comments':
        comments.extend([models.MapComment(x=4.,y=3.,text='TIFF Ø\n grondwater\nExtra\tdetail'),models.MapComment(x=4.,y=3.,text='Extra slot\nTIFF Ø')])
    return exporter._template_comments_text(SimpleNamespace(bounds=models.Bounds(0.,0.,8.,6.)),comments)
def main():
    out=Path(sys.argv[1]);audit=json.loads((out/'audit.json').read_text(encoding='utf-8'));start=datetime.fromtimestamp((out/'started.txt').stat().st_mtime).date();end=datetime.fromtimestamp((out/'audit.json').stat().st_mtime).date();dates={(start+timedelta(days=i)).strftime('%d-%m-%Y') for i in range((end-start).days+1)}
    baseline=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf');base_profiles=len([e for e in baseline.modelspace() if e.dxf.layer=='X-XX-AL-PROFIELBALK-SD'])
    for case in audit:
        path=out/case['path'];doc=ezdxf.readfile(path);report=doc.audit();assert not report.has_errors,report.errors
        values=[(a.dxf.tag,a.dxf.text) for e in doc.layouts.get('Blad1').query('INSERT') for a in e.attribs]
        project=[text for tag,text in values if tag in ['GEM-PROJECT','PROJECTOMSCHRIJVING','PROJECTNAAM']];assert project and set(project)=={'Native venster Ø 漢'},project
        actual_dates=[text for tag,text in values if tag=='DATUM1'];assert actual_dates and len(set(actual_dates))==1 and set(actual_dates)<=dates,actual_dates
        assert ('TEKENAAR1','Native test') in values
        texts=[str(e.dxf.text) for e in doc.modelspace().query('TEXT')]+[str(e.text) for e in doc.modelspace().query('MTEXT')]
        assert 'Proefsleuf 12' in texts;assert texts.count(original_comments(case['name']))==(2 if case['name']=='ordered-variants' else 1)
        if case['name']=='ordered-variants':assert 'Proefsleuf 12B' in texts
        leaders=list(doc.modelspace().query('LEADER'))
        profile_objects=len([e for e in doc.modelspace() if e.dxf.layer=='X-XX-AL-PROFIELBALK-SD']);assert (profile_objects>base_profiles) if case['draw'] else (profile_objects==base_profiles)
        assert any(e.dxf.layer=='PROEFSLEUVEN_KADASTER' and e.dxf.get('true_color')==0xff0000 for e in doc.modelspace().query('LWPOLYLINE'))
        for e in leaders:
            if e.dxf.hasattr('annotation_handle') and e.dxf.annotation_handle!='0':assert doc.entitydb.get(e.dxf.annotation_handle) is not None
        if case['draw']:assert leaders,case['name']
        images=[Path(doc.entitydb[e.dxf.image_def_handle].dxf.filename) for e in doc.modelspace().query('IMAGE') if 'slot000' in doc.entitydb[e.dxf.image_def_handle].dxf.filename]
        assert len(images)==case['images'],(case['name'],images)
        for image in images:
            with Image.open(image) as pixels:
                pixels.load()
                if image.name.endswith('_kaart.png'):assert pixels.size==(3508,2480)
                else:assert pixels.width>0 and pixels.height>0
        if case['name']=='ordered-variants':assert not any('slot00002' in p.name for p in images) and any('slot00003' in p.name for p in images)
        if case['name']=='selected-points':selected_points(doc,images)
        if case['name']=='rotated-tiff':selected_points(doc,images,37.5)
        workbook(path.with_suffix('.xlsx'),case['name'])
        print(case['name'],': native worker titles/date/project, DXF ownership/annotations and',len(images),'durable PNGs passed')
    assert [c['name'] for c in audit]==['normal','reverse','image-only','ordered-variants','missing-measurements','selected-points','tiff-comments','rotated-tiff']
if __name__=='__main__':main()
