"""Actual native DXF readback compared with original supported underlay copies."""
from pathlib import Path
import json,sys
import ezdxf
from generate_template_underlay_fixtures import ROOT,OUT,snapshot
from verify_native_template_images import compare
def main():
    output=Path(sys.argv[1]);source=ezdxf.readfile(OUT/'source.dxf');original=set(source.entitydb);total=0
    for step in json.loads((OUT/'reference.json').read_text(encoding='utf-8')):
        doc=ezdxf.readfile(output/f'underlay-{step["required"]}.dxf')
        compare(snapshot(doc),step['expected'],'underlays')
        definitions={e.dxf.handle:e for e in doc.objects if e.dxftype() in ('PDFDEFINITION','DGNDEFINITION','DWFDEFINITION')}
        assert len(definitions)==3
        copied_dicts=set()
        for layout in doc.layouts:
            for e in layout:
                if not e.dxftype().endswith('UNDERLAY'):continue
                total+=1;assert e.dxf.owner==layout.layout_key
                definition=definitions[e.dxf.underlay_def_handle];assert e.dxf.handle in definition.get_reactors()
                assert e.get_xdata('PAGE_TEST')[0].value=='retain underlay metadata'
                dictionary=e.get_extension_dict().dictionary;assert dictionary.dxf.owner==e.dxf.handle
                assert dictionary['UNDERLAY'].dxf.owner==dictionary.dxf.handle
                assert dictionary['UNDERLAY'].tags[0].value=='independent owned data'
                if e.dxf.handle not in original:
                    assert dictionary.dxf.handle not in original and dictionary.dxf.handle not in copied_dicts
                    copied_dicts.add(dictionary.dxf.handle)
        # Source definitions are shared; each copied entity is independently
        # registered. Every reactor must point at a still-present entity.
        for d in definitions.values():
            reactors=d.get_reactors()
            references={e.dxf.handle for e in doc.entitydb.values() if e.dxftype() in ('PDFUNDERLAY','DGNUNDERLAY','DWFUNDERLAY') and e.dxf.underlay_def_handle==d.dxf.handle}
            assert len(reactors)==len(set(reactors)),('duplicate reactors',d.dxf.handle)
            # Underlay definitions also register their owning dictionary.
            assert set(reactors)==references|{d.dxf.owner},('nonreciprocal definition',d.dxf.handle)
        audit=doc.audit();assert not audit.has_errors,(step['required'],audit.errors)
    print('3 actual native DXFs;',total,'original-matched underlays, shared definitions, independent dictionaries and reciprocal reactors')
if __name__=='__main__':main()
