"""Independent ezdxf readback of native sheet copies and owned object graphs."""
from pathlib import Path
import importlib
import json
import sys
import ezdxf
from generate_template_page_fixtures import ROOT,OUT,snapshot
from generate_object_export_fixtures import reference
from verify_native_template_images import compare
from verify_native_template_edits import records

def handle(tags):
    if tags[0][1] in ('SECTION','ENDSEC','EOF'):return None
    return next((v for c,v in tags if c in (5,105)),None)
def without_reactors(tags):
    result=[];depth=0
    for c,v in tags:
        if depth:
            if c==102:depth+=1 if v.startswith('{') else -1 if v=='}' else 0
            continue
        if (c,v)==(102,'{ACAD_REACTORS'):depth=1;continue
        result.append((c,v))
    return result

def preserve_source(before,after,layout_dict):
    indexed={handle(r):r for r in after if handle(r) is not None}
    for old in before:
        h=handle(old)
        if h is None:continue
        new=indexed[h];kind=old[0][1]
        mask={1} if kind=='ATTRIB' else {12,22,16,26,36,17,27,37,45,51} if kind=='VIEWPORT' else set()
        if kind=='TABLE' and (2,'BLOCK_RECORD') in old:mask={70}
        if h==layout_dict:mask={3,350}
        if kind=='IMAGEDEF':old=without_reactors(old);new=without_reactors(new)
        assert [(c,v) for c,v in old if c not in mask]==[(c,v) for c,v in new if c not in mask],(kind,h,'unrelated source tags changed')

def persisted_sequences(raw,original_handles):
    parent=None
    for r in raw:
        kind=r[0][1]
        if kind in ('INSERT','POLYLINE'):parent=handle(r)
        elif kind=='SEQEND':
            if handle(r) not in original_handles:
                assert parent is not None and next(v for c,v in r if c==330)==parent
            parent=None
        elif kind not in ('ATTRIB','VERTEX'):parent=None

def ownership(doc,original_handles):
    def check_dictionary(d):
        for _,child in d.items():
            assert child.dxf.owner==d.dxf.handle
            if child.dxftype()=='DICTIONARY':check_dictionary(child)
    for layout in doc.layouts:
        for e in layout:
            assert e.dxf.owner==layout.layout_key,(layout.name,e)
            if e.dxftype()=='INSERT':
                for a in e.attribs:assert a.dxf.owner==e.dxf.handle
                # ezdxf can synthesize an unpersisted layout-owned SEQEND for
                # an INSERT without attributes. Only linked sequences need it.
                if e.seqend and e.attribs:
                    # Binding active-paper entities can reset SEQEND ownership
                    # to the layout in ezdxf. Persisted owner tags are checked
                    # separately before this reader normalization.
                    assert e.seqend.dxf.owner in {e.dxf.handle,layout.layout_key}
            if e.dxftype()=='POLYLINE':
                for v in e.vertices:assert v.dxf.owner==e.dxf.handle
                if e.seqend:assert e.seqend.dxf.owner in {e.dxf.handle,layout.layout_key}
            if e.dxftype()=='IMAGE':
                reactor=doc.entitydb[e.dxf.image_def_reactor_handle]
                assert reactor.dxf.owner==e.dxf.handle and reactor.dxf.image_handle==e.dxf.handle
                definition=doc.entitydb[e.dxf.image_def_handle]
                assert reactor.dxf.handle in definition.get_reactors()
            if e.has_extension_dict:
                dictionary=e.get_extension_dict().dictionary
                assert dictionary.dxf.owner==e.dxf.handle
                check_dictionary(dictionary)
                if 'NESTED' in dictionary and e.dxf.handle not in original_handles:
                    assert dictionary['NESTED']['DATA'].dxf.handle not in original_handles
                    assert dictionary['WEAK']['DATA'].dxf.handle not in original_handles

def main():
    output=Path(sys.argv[1]);ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    total=0
    for c in json.loads((OUT/'reference.json').read_text(encoding='utf-8')):
        original=ezdxf.readfile(ROOT/c['path']);original_handles=set(original.entitydb);baseline=original.audit()
        source_records=records(ROOT/c['path']);layout_dict=original.layouts.get('Model').dxf_layout.dxf.owner
        for i,s in enumerate(c['steps']):
            if s['error']:continue
            p=output/f'{c["name"]}-{i}.dxf';doc=ezdxf.readfile(p)
            compare(snapshot(doc,exporter),s['expected'],f'{c["name"]}-{i}')
            ownership(doc,original_handles)
            raw=records(p);preserve_source(source_records,raw,layout_dict);persisted_sequences(raw,original_handles)
            # Embedded MTEXT stores direction at 10 and insertion at 11; only
            # the latter moves. Check persisted data, not regenerated DXF tags.
            for r in raw:
                if r[0]!=(0,'MTEXT') or handle(r) in original_handles:continue
                at=next((j for j,t in enumerate(r) if t[0]==101),None)
                if at is not None:
                    for a,b in [(10,11),(20,21),(30,31)]:
                        main=next(float(v) for code,v in r[:at] if code==a)
                        embedded=next(float(v) for code,v in r[at:] if code==b)
                        assert abs(main-embedded)<1e-9
            native=json.loads(p.with_suffix('.json').read_text(encoding='utf-8'))
            assert native['added_pages']==s['added_pages']
            assert len(native['slots'])==len(s['expected']['slots'])>=s['required']
            audit=doc.audit()
            assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes],(c['name'],i,[(e.code,e.message) for e in audit.errors],[(e.code,e.message) for e in audit.fixes])
            total+=1
    print(total,'original sheet-extension states, geometry, dictionaries, images, title dates and viewport links verified')

if __name__=='__main__':main()
