"""Independent XLSX ZIP/XML readback against the original template workflow."""
from pathlib import Path
import json,sys
from verify_native_discipline_exports import signature
ROOT=Path(__file__).resolve().parents[1]
def original_signature(rule_data,dataset_data,rows):
    """Run the source plan/count/writer for explicit independent input rows."""
    from types import SimpleNamespace
    import importlib,tempfile
    from generate_object_export_fixtures import reference
    ref=reference();report=importlib.import_module(ref.__package__+'.discipline_excel_export')
    rules=ref.build_object_layer_rules([(','.join(r['keywords']),r['target_layer'],r['color_aci'],r['label']) for r in rule_data])
    datasets=[SimpleNamespace(points=[SimpleNamespace(source_name=f['source_name']) for f in d['features'] if 'Point' in f['geometry']],polylines=[SimpleNamespace(source_name=f['source_name']) for f in d['features'] if 'Polyline' in f['geometry']]) for d in dataset_data]
    class App:
        def set_status(self,*_):pass
        def update_idletasks(self):pass
        def _load_maaiveld_dataset_for_layer(self,layer):return datasets[layer.index] if layer.index is not None else None
    app=App();layers=[None if row is None else SimpleNamespace(metadata={'template_proefsleuf_label':row[0]},index=row[1]) for row in rows]
    plan=report.template_export_plan(app,layers);summaries,warnings=report.summarize_template_export_plan(app,plan,rules)
    columns=report.discipline_columns(rules);canonical={c.casefold():c for c in columns};fixed=[]
    # Retain the documented fix: source spelling variants must not lose counts.
    for summary in summaries:
        counts={}
        for name,count in summary.counts.items():
            name=canonical[name.casefold()];counts[name]=counts.get(name,0)+count
        fixed.append(report.DisciplineSummary(summary.proefsleuf,counts))
    with tempfile.TemporaryDirectory(prefix='sleufbase-source-report-') as folder:
        path=Path(folder)/'original.xlsx';report.write_discipline_workbook(path,fixed,disciplines=columns)
        return signature(path),warnings
def main():
    out=Path(sys.argv[1]);cases=json.loads((ROOT/'rust/tests/fixtures/template-reports/reference.json').read_text(encoding='utf-8'))
    for case in cases:
        actual=signature(out/(case['name']+'.xlsx'))
        for key in actual:
            assert actual[key]==case['expected'][key],(case['name'],key,str(actual[key])[:1300],str(case['expected'][key])[:1300])
    print(f'{len(cases)} template-order workbooks / {sum(len(c["rows"]) for c in cases)} rows match original cells, counts, totals, literal formulas, styles, widths and filters (recorded casefold count correction retained)')
if __name__=='__main__':main()
