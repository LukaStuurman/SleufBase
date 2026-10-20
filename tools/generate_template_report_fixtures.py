"""Original template plan/count/workbook contract, without starting the app."""
from pathlib import Path
from types import SimpleNamespace
import importlib,json,os,tempfile
from generate_object_export_fixtures import reference
from verify_native_discipline_exports import signature

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_REPORT_FIXTURES',ROOT/'rust/tests/fixtures/template-reports/reference.json'))

def main():
    ref=reference();report=importlib.import_module(ref.__package__+'.discipline_excel_export')
    result=[]
    bases=json.loads((ROOT/'rust/tests/fixtures/disciplines/workbooks.json').read_text(encoding='utf-8'))
    for base in bases:
        rules=ref.build_object_layer_rules([(','.join(r['keywords']),r['target_layer'],r['color_aci'],r['label']) for r in base['rules']])
        datasets=[SimpleNamespace(points=[SimpleNamespace(source_name=f['source_name']) for f in d['features'] if 'Point' in f['geometry']],polylines=[SimpleNamespace(source_name=f['source_name']) for f in d['features'] if 'Polyline' in f['geometry']]) for d in base['datasets']]
        class App:
            def set_status(self,*_):pass
            def update_idletasks(self):pass
            def _load_maaiveld_dataset_for_layer(self,layer):
                if layer.index==-1:raise RuntimeError('Bron kon niet worden gelezen')
                return datasets[layer.index] if layer.index is not None else None
        app=App();layers=[]
        for label,index in [(' PS12B ',1),('PS12',0),('PS12B',1),('Geen gegevens Ø 漢',None),('=1+1',0),('Foutbron',-1)]:
            layers.append(SimpleNamespace(metadata={'template_proefsleuf_label':label},index=index))
        # An explicit blank consumes a DXF slot, never an Excel row.
        layers.insert(1,None)
        plan=report.template_export_plan(app,layers)
        summaries,warnings=report.summarize_template_export_plan(app,plan,rules)
        columns=report.discipline_columns(rules)
        if base['correction']:
            canonical={c.casefold():c for c in columns};fixed=[]
            for summary in summaries:
                counts={}
                for name,count in summary.counts.items():
                    name=canonical[name.casefold()];counts[name]=counts.get(name,0)+count
                fixed.append(report.DisciplineSummary(summary.proefsleuf,counts))
            summaries=fixed
        with tempfile.TemporaryDirectory(prefix='sleufbase-original-report-') as folder:
            path=Path(folder)/'original.xlsx';report.write_discipline_workbook(path,summaries,disciplines=columns);expected=signature(path)
        rows=[dict(label=label,dataset=layer.index if layer.index is not None and layer.index>=0 else None) for label,layer in plan]
        result.append(dict(name=base['name'],rules=base['rules'],datasets=base['datasets'],rows=rows,expected=expected,warnings=warnings,correction=base['correction']))
    OUT.parent.mkdir(parents=True,exist_ok=True);OUT.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'{len(result)} original template report plans / {sum(len(c["rows"]) for c in result)} rows generated')
if __name__=='__main__':main()
