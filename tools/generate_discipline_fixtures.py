"""Original rule normalization and workbook oracles; no application startup."""
from pathlib import Path
import importlib
import json
import tempfile
from generate_object_export_fixtures import reference
from verify_native_discipline_exports import signature

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/disciplines"

def main():
    ref=reference()
    report=importlib.import_module(ref.__package__+".discipline_excel_export")
    variants=[
        ("defaults",None,["water","LS","water","MS","nothing","data"]),
        ("configured",[["*wat?r*; leiding\npipe","DRINKWATER",5,"Drinkwater"],["d[ae]ta?","DATANET",3,"Data & glas"],["*","REST",256,"Overig"]],["WATER","leiding","data1","unknown"]),
        ("empty-rules",[],["water","LS"]),
        ("same-label",[["water","W",5,"Leiding"],["gas","G",30,"Leiding"]],["water","gas","water"]),
        ("casefold-labels",[["water","W",5,"Straße"],["gas","G",30,"STRASSE"],["LS","L",190,"ı"],["data","D",3,"I"]],["water","gas","LS","data"]),
        ("fallback-label",[["water","W",5],["gas","G",30,""]],["water","gas"]),
        ("literal-formulas",[["water","W",5,"=1+1"],["gas","G",30,"<gas>&"]],["water","gas"]),
        ("normalization",[[" ; , ","SKIP",1,"skip"],["water"," W ","٢_٥٥"," Water "],["gas","G","invalid",True],["LS","L",-5.5,None],["data","D",999.9,42],["tiny"]],["water","gas","LS","data"]),
        ("unicode-case",[["*ß*","SHARP",4,"Straße"],["mſ","MS",5,"MS"],["*\u1c8a*","NEW",6,"Unassigned in Unicode 14"]],["strasse","mſ","\u1c89","\u1c8a"]),
    ]
    cases=[]
    for name,rows,names in variants:
        if name=="normalization":
            colors=[0,False,True,1,255,256,257,-1,1.9,None,"bad","-99999","-1_0","1__0","1_0","٢٥٥","+10"," -٢ ",1e-5,-10**400,10**400,"\u001c1"]
            for index,color in enumerate(colors):
                code=f"=C{index:03}="
                rows.append([code,f"COLOR_{index}",color,f"Kleur {index}"]);names.append(code)
        rules=ref.build_object_layer_rules(rows)
        datasets=[]; summaries=[]; object_styles=[]
        for i,source_names in enumerate([names,list(reversed(names[:3]))]):
            title=" =1+1 " if name=="literal-formulas" else f" PS{i+1} "
            points=tuple(ref.KickTheMapObjectPoint(source,source,float(j),float(i),None) for j,source in enumerate(source_names))
            dataset=ref.KickTheMapObjectDataset(i+1,title,Path("fixture.json"),points)
            summaries.append(report.summarize_dataset(title,dataset,rules))
            for point in points:
                matched=next((rule for rule in rules if rule.matches(point.source_name)),None)
                object_styles.append({"layer":matched.target_layer if matched else "0","color":matched.color if matched else 256})
            datasets.append({"job_id":i+1,"title":title,"source_path":"fixture.json","features":[{"object_name":p.object_name,"source_name":p.source_name,"geometry":{"Point":{"x":p.x,"y":p.y,"z":p.z}},"attributes":["","",""]} for p in points]})
        columns=report.discipline_columns(rules)
        correction=None
        if name=="casefold-labels":
            correction="Merge counts into first casefold-equivalent column; Python otherwise loses later spelling variants in cells and totals"
            canonical={column.casefold():column for column in columns}
            fixed=[]
            for summary in summaries:
                counts={}
                for key,count in summary.counts.items():
                    target=canonical[key.casefold()];counts[target]=counts.get(target,0)+count
                fixed.append(report.DisciplineSummary(summary.proefsleuf,counts))
            summaries=fixed
        with tempfile.TemporaryDirectory() as tmp:
            file=Path(tmp)/"reference.xlsx"
            report.write_discipline_workbook(file,summaries,disciplines=columns)
            expected=signature(file)
        cases.append({"name":name,"rows":rows,"rules":[{"keywords":list(r.keywords),"target_layer":r.target_layer,"color_aci":r.color,"label":r.profile_label} for r in rules],"datasets":datasets,"expected":expected,"object_styles":object_styles,"correction":correction})
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/"workbooks.json").write_text(json.dumps(cases,ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    print(f"Generated {len(cases)} original discipline workbook references")

if __name__=="__main__":main()
