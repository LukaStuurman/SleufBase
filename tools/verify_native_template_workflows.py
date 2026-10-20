"""Read native worker files through independent ezdxf/Pillow.

The same exporter is exercised by actual egui click/lifecycle tests. This readback
checks published output, title fields, annotations, images and stable variants.
"""
from pathlib import Path
from datetime import datetime,timedelta
import json,sys
from verify_native_discipline_exports import signature
from verify_native_template_reports import original_signature
import ezdxf
from PIL import Image
ROOT=Path(__file__).resolve().parents[1]
def workbook(path,name):
    original=next(c['request'] for c in json.loads((ROOT/'rust/tests/fixtures/template-export/reference.json').read_text(encoding='utf-8')) if c['name']=='native-tiff')
    rows=[('PS12',None if name=='missing-measurements' else 0)]
    if name=='ordered-variants':rows.extend([None,('PS12B',0)])
    expected,_=original_signature(original['rules'],original['datasets'],rows)
    assert signature(path)==expected,(name,'automatic workbook differs from original')
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
        assert 'Proefsleuf 12' in texts;assert texts.count('Opmerkingen:\\PGrondwater\\PStraße')==(2 if case['name']=='ordered-variants' else 1)
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
        workbook(path.with_suffix('.xlsx'),case['name'])
        print(case['name'],': native worker titles/date/project, DXF ownership/annotations and',len(images),'durable PNGs passed')
    assert [c['name'] for c in audit]==['normal','reverse','image-only','ordered-variants','missing-measurements']
if __name__=='__main__':main()
