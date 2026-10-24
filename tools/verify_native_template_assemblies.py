"""Independent full readback of native prepared-export files and owned assets."""
from pathlib import Path
import importlib,json,os,subprocess,sys
import ezdxf
from PIL import Image
from generate_object_export_fixtures import reference
from generate_template_export_fixtures import ROOT,OUT,snapshot
from verify_native_template_images import compare
from verify_native_template_pages import ownership,persisted_sequences
from verify_native_template_setup import preserve_source
from verify_native_template_edits import records

def main():
    # Use one physical parent for both relpath() and the CLI manifest argument.
    # Windows app-data aliases/junctions can resolve to a different-depth path.
    output=Path(sys.argv[1]).resolve();ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    total_images=total_profiles=0
    for case in json.loads((OUT/'reference.json').read_text(encoding='utf-8')):
        request=case['request'];original=ezdxf.readfile(ROOT/request['template']);old_handles=set(original.entitydb)
        targets=exporter._detect_template_slots(original)
        texts={getattr(s,field) for s in targets[:len(request['slots'])] for field in ('label_handle','address_handle','comments_handle')}
        baseline=original.audit();path=output/(case['name']+'.dxf');doc=ezdxf.readfile(path)
        compare(snapshot(doc,exporter),case['expected'],case['name'])
        metadata=json.loads((output/(case['name']+'.json')).read_text(encoding='utf-8'))
        for key in ('filled','profiles','added_pages'):assert metadata[key]==case[key],(case['name'],key)
        bundle=Path(metadata['assets']);assert bundle.parent.resolve()==(output/(case['name']+'_assets')).resolve()
        image_count=0
        for entity in doc.modelspace():
            if entity.dxftype()=='IMAGE' and entity.dxf.handle not in old_handles:
                image_count+=1;p=Path(entity.image_def.dxf.filename);assert p.parent==bundle and p.is_file()
                generated=case.get('prepared_tiffs',{}).get(str(int(p.stem[4:9])-1)) if p.stem.endswith('_tiff') else None
                source=OUT/generated if generated else OUT/('tiff.png' if p.stem.endswith('_tiff') else 'map.png')
                if not generated:assert p.read_bytes()==source.read_bytes()
                with Image.open(p) as a,Image.open(source) as b:assert a.convert('RGBA').tobytes()==b.convert('RGBA').tobytes()
            if entity.dxftype()=='LEADER':assert doc.entitydb[entity.dxf.annotation_handle].dxftype()=='INSERT'
        assert image_count==2*case['filled'];assert len(list(bundle.iterdir()))==image_count
        raw=records(path);ownership(doc,old_handles);persisted_sequences(raw,old_handles)
        changed_layers={r['target_layer'].lower() for r in request['rules']}|{'b-we-og-terrein_profiellijn-gd','x-xx-al-profielbalk-sd','x-xx-al-verwijzing-sd'}
        image_dict=original.rootdict.get('ACAD_IMAGE_DICT')
        mutable=() if image_dict is None else (image_dict.dxf.handle,)
        # Contact repair is part of the original complete exporter. Derive its
        # exact expected TEXT values independently and keep every other source
        # tag under the existing strict preservation comparison.
        old_texts={entity.dxf.handle:entity.dxf.text for entity in original.entitydb.values() if entity.dxftype()=='TEXT'}
        exporter._restore_template_layout_contact_text(original)
        restored={handle:original.entitydb[handle].dxf.text for handle,value in old_texts.items() if original.entitydb[handle].dxf.text!=value}
        source_records=records(ROOT/request['template'])
        for record in source_records:
            if record[0]!=(0,'TEXT'):continue
            handle=next((value for code,value in record if code==5),None)
            if handle in restored:
                expected=restored[handle]
                record[:]=[(code,expected if code==1 else value) for code,value in record]
        preserve_source(source_records,raw,original,changed_layers,texts,mutable)
        audit=doc.audit();assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes],(case['name'],[(e.code,e.message) for e in audit.fixes])
        total_images+=image_count;total_profiles+=case['profiles']
    print(len(json.loads((OUT/'reference.json').read_text(encoding='utf-8'))),'native prepared exports,',total_profiles,'complete profiles and',total_images,'permanent PNGs: full order/fields/graphs/pixels and source preservation verified')
    if len(sys.argv)>2:
        case=next(c for c in json.loads((OUT/'reference.json').read_text(encoding='utf-8')) if c['name']=='native-tiff')
        request=case['request'];request['settings']['discipline_excel']=True;request['template']=os.path.relpath(ROOT/request['template'],output)
        for slot in filter(None,request['slots']):
            for key in ('tiff_png','map_png'):
                if key in slot:slot[key]=os.path.relpath(ROOT/slot[key],output)
            if slot.get('tiff_source') is not None:slot['tiff_source']['path']=os.path.relpath(ROOT/slot['tiff_source']['path'],output)
        manifest=output/'prepared-request.json';manifest.write_text(json.dumps(request,ensure_ascii=False),encoding='utf-8')
        ignored=output/'unused-settings.json';ignored.write_text('not valid settings JSON',encoding='utf-8')
        target=output/'cli.dxf';command=[str(Path(sys.argv[2]).resolve()),'--settings',str(ignored.resolve()),'--template-dxf',str(target.resolve()),str(manifest.resolve())]
        result=subprocess.run(command,cwd=output,text=True,encoding='utf-8',capture_output=True)
        assert result.returncode==0,(result.returncode,result.stdout,result.stderr)
        from verify_native_template_reports import original_signature
        from verify_native_discipline_exports import signature
        rows=[None if s is None else (s.get('report_label',s['label']),s.get('report_dataset',(s.get('profile') or {}).get('dataset'))) for s in request['slots']]
        expected,_=original_signature(request['rules'],request['datasets'],rows)
        assert signature(target.with_suffix('.xlsx'))==expected,'CLI automatic workbook differs from original'
        doc=ezdxf.readfile(target);compare(snapshot(doc,exporter),case['expected'],'real CLI')
        old_handles=set(ezdxf.readfile(ROOT/'assets/cadastral_template.dxf').entitydb)
        for e in doc.modelspace().query('IMAGE'):
            if e.dxf.handle not in old_handles:assert Path(e.image_def.dxf.filename).is_file()
        before={p:p.read_bytes() for p in output.rglob('*') if p.is_file()}
        again=subprocess.run(command,cwd=output,text=True,encoding='utf-8',capture_output=True)
        assert again.returncode==2 and {p:p.read_bytes() for p in output.rglob('*') if p.is_file()}==before
        print('Real CLI: manifest-relative inputs, full prepared export and refusal to overwrite verified')
        notices=subprocess.run([str(Path(sys.argv[2]).resolve()),'--licenses'],text=True,encoding='utf-8',capture_output=True)
        assert notices.returncode==0 and 'Secret Labs AB' in notices.stdout and 'Pillow 11.3.0' in notices.stdout
        print('Standalone binary contains the adapted Pillow copyright/license notice')
if __name__=='__main__':main()
