from pathlib import Path
import importlib,json,sys
import ezdxf
from generate_template_setup_fixtures import ROOT,OUT,snapshot
from generate_object_export_fixtures import reference
from verify_native_template_images import compare
from verify_native_template_pages import ownership,persisted_sequences,handle,without_reactors
from verify_native_template_edits import records
def preserve_source(before,after,original,changed_layers,texts=(),mutable_dictionaries=()):
    indexed={handle(r):r for r in after if handle(r) is not None}
    mutable_dicts={original.rootdict.dxf.handle,original.layouts.get('Model').dxf_layout.dxf.owner}
    mutable_dicts.update(mutable_dictionaries)
    scales=original.rootdict.get('ACAD_SCALELIST')
    if scales is not None:mutable_dicts.add(scales.dxf.handle)
    for old in before:
        h=handle(old)
        if h is None:continue
        new=indexed[h];kind=old[0][1];mask=set()
        if kind=='ATTRIB':mask={1}
        elif kind in {'TEXT','MTEXT'} and h in texts:mask={1,3}
        elif kind=='VIEWPORT':mask={12,22,16,26,36,17,27,37,45,51}
        elif kind=='LAYER' and next(v for c,v in old if c==2).lower() in changed_layers:mask={62,370}
        elif kind=='TABLE' and next(v for c,v in old if c==2) in {'BLOCK_RECORD','LAYER','STYLE'}:mask={70}
        elif kind=='SCALE':mask={300,140,141,290}
        elif kind=='RASTERVARIABLES':mask={70,71,72}
        if h in mutable_dicts:mask={3,350,360}
        if kind=='IMAGEDEF':old=without_reactors(old);new=without_reactors(new)
        assert [(c,v) for c,v in old if c not in mask]==[(c,v) for c,v in new if c not in mask],(kind,h,'unrelated source tags changed')
    def header(raw):
        tags=next(r for r in raw if r[:2]==[(0,'SECTION'),(2,'HEADER')]);result=[];keep=True
        for c,v in tags:
            if c==9:keep=v not in {'$HANDSEED','$INSUNITS'}
            if keep:result.append((c,v))
        return result
    assert header(before)==header(after),'unrelated header variables changed'

def main():
    output=Path(sys.argv[1]);ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None);count=0
    for c in json.loads((OUT/'reference.json').read_text(encoding='utf-8')):
        original=ezdxf.readfile(ROOT/c['path']);old_handles=set(original.entitydb);baseline=original.audit()
        changed_layers=set()
        for i,s in enumerate(c['steps']):
            p=output/f'{c["name"]}-{i}.dxf';doc=ezdxf.readfile(p)
            compare(snapshot(doc,exporter),s['expected'],f'{c["name"]}-{i}')
            if s['profile_layers'] is not None:
                changed_layers.update(l['name'].lower() for l in s['profile_layers'])
                changed_layers.update({'b-we-og-terrein_profiellijn-gd','x-xx-al-profielbalk-sd','x-xx-al-verwijzing-sd'})
            raw=records(p);ownership(doc,old_handles);persisted_sequences(raw,old_handles)
            preserve_source(records(ROOT/c['path']),raw,original,changed_layers)
            audit=doc.audit();assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes],(c['name'],i,[(e.code,e.message) for e in audit.fixes])
            count+=1
    print(count,'composed preparation states: pages, symbols, legends, units/scales, project/date and viewports verified')
if __name__=='__main__':main()
