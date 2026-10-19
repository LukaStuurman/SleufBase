from pathlib import Path
import importlib,json,sys
import ezdxf
from generate_template_legend_fixtures import ROOT,OUT,legend_snapshot
from generate_object_export_fixtures import reference
from verify_native_template_images import compare
from verify_native_template_pages import ownership,preserve_source,persisted_sequences
from verify_native_template_edits import records
def main():
    output=Path(sys.argv[1]);ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    cases=json.loads((OUT/'reference.json').read_text(encoding='utf-8'))
    for c in cases:
        original=ezdxf.readfile(ROOT/c['path']);old_handles=set(original.entitydb);baseline=original.audit()
        path=output/(c['name']+'.dxf');doc=ezdxf.readfile(path)
        compare(legend_snapshot(doc,exporter),c['expected'],c['name'])
        ownership(doc,old_handles)
        raw=records(path);preserve_source(records(ROOT/c['path']),raw,original.layouts.get('Model').dxf_layout.dxf.owner);persisted_sequences(raw,old_handles)
        audit=doc.audit();assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes],(c['name'],[(e.code,e.message) for e in audit.fixes])
    print(len(cases),'original legend cases: real sheets, affine attributes, image graphs and all metadata verified')
if __name__=='__main__':main()
