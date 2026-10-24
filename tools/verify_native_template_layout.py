"""Independent page/viewport and Unicode layout ownership readback."""
from pathlib import Path
import json
import sys
import ezdxf
from generate_template_layout_fixtures import ROOT,OUT,snapshot
from generate_template_geometry_fixtures import signature as geometry_signature
from generate_template_image_fixtures import signature as image_signature
from verify_native_template_images import compare,identity
from verify_native_template_edits import records

VIEW_CODES={12,22,16,26,36,17,27,37,45,51}

def main():
    output=Path(sys.argv[1]);states=0
    for case in json.loads((OUT/'reference.json').read_text(encoding='utf-8')):
        source=ROOT/case['path'];before=records(source);baseline=ezdxf.readfile(source).audit()
        for i,step in enumerate(case['steps']):
            path=output/f'{case["name"]}-{i}.dxf';doc=ezdxf.readfile(path)
            compare(snapshot(doc),step['expected'],f'{case["name"]}.{i}')
            assert json.loads(path.with_suffix('.json').read_text(encoding='utf-8'))==step['pages']
            after=records(path);assert len(before)==len(after)
            for old,new in zip(before,after):
                kind,handle=identity(old);assert identity(new)==(kind,handle)
                # Only the fitted viewport's view tags and title text can differ.
                # The full expected snapshot above checks every viewport/ATTRIB.
                mask=VIEW_CODES if kind=='VIEWPORT' else {1} if kind=='ATTRIB' else set()
                assert [(c,v) for c,v in old if c not in mask]==[(c,v) for c,v in new if c not in mask],(case['name'],i,kind,handle)
            audit=doc.audit();assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes]
            states+=1
    fixture=json.loads((OUT/'unicode-reference.json').read_text(encoding='utf-8'));doc=ezdxf.readfile(output/'unicode.dxf')
    compare(snapshot(doc)['attributes'],fixture['attributes'],'Unicode title fields')
    compare(snapshot(doc)['viewports'],snapshot(ezdxf.readfile(OUT/'sheets.dxf'))['viewports'],'Unicode viewports unchanged')
    for name,wanted in fixture['expected'].items():
        layout=doc.layouts.get(name)
        circles=list(layout.query('CIRCLE'));images=list(layout.query('IMAGE'))
        assert len(circles)==len(images)==1,name
        compare(geometry_signature(circles[0]),wanted['circle'],name+'.circle')
        compare(image_signature(doc,images[0]),wanted['image'],name+'.image')
        assert circles[0].dxf.owner==images[0].dxf.owner==layout.layout_key
    handles=[e.dxf.handle for e in doc.entitydb.values() if e.dxf.get('handle')]
    assert len(handles)==len(set(handles))
    assert int(doc.header['$HANDSEED'],16)>max(int(h,16) for h in handles)
    audit=doc.audit();assert not audit.errors and not audit.fixes
    print(f'{states} original page/viewport states and five Unicode layouts: numbers, scales, source tags, titles and circle/image ownership verified')

if __name__=='__main__':main()
