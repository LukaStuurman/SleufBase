"""Freeze original AppSettings defaults and label-gap conversion without loading user files."""
from pathlib import Path
import importlib
import json
from generate_object_export_fixtures import reference

root=Path(__file__).resolve().parents[1]
ref=reference()
settings=importlib.import_module(ref.__package__+".settings")
# Constructing defaults does not call load_settings or read accounts/configuration.
app=settings.AppSettings()
rules=ref.build_object_layer_rules([(r.keywords,r.target_layer,r.color,r.profile_label) for r in app.kickthemap_object_layer_rules])
values=[None,False,True,-2,0,3.5,100,1000,"3.5","3,5","٣.٥","1_0","nan","inf","-inf","1e999","",[],{},"\u001c1"]
payload={"label_gap":app.dxf_label_gap,"rules":[{"keywords":list(r.keywords),"target_layer":r.target_layer,"color_aci":r.color,"label":r.profile_label} for r in rules],"gap_cases":[{"value":v,"expected":settings._normalize_float(v,settings.DEFAULT_DXF_LABEL_GAP,settings.MIN_DXF_LABEL_GAP,settings.MAX_DXF_LABEL_GAP)} for v in values]}
out=root/"rust/tests/fixtures/disciplines/application-settings.json"
out.write_text(json.dumps(payload,ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
print(f"Generated original application defaults ({len(rules)} rules) and {len(values)} label-gap cases")
