"""Original non-secret preference contracts; never calls load/save_settings."""
import importlib
import json
import os
from pathlib import Path
from generate_object_export_fixtures import reference

ROOT=Path(__file__).resolve().parents[1]
s=importlib.import_module(reference().__package__+'.settings')
defaults=s.AppSettings()
flags=['include_tiffs_in_dxf_export','template_include_cross_sections',
 'template_generate_reverse_cross_section_sheet','template_show_profile_direction',
 'template_enable_maaiveld_edit_points','template_avoid_multileader_collisions',
 'template_clip_cross_section_markers_to_profile']
texts=['cadastral_template_path','template_client_logo_path','template_cross_section_diameter_text']
colors=['dxf_centerline_color','dxf_label_color']
def expected(value):
 out={k:s._normalize_bool(value.get(k,getattr(defaults,k)),getattr(defaults,k)) for k in flags}
 out.update({k:s._normalize_text(value.get(k,getattr(defaults,k)),getattr(defaults,k)) for k in texts})
 out.update({k:s._normalize_hex_color(value.get(k,getattr(defaults,k)),getattr(defaults,k)) for k in colors})
 out['dxf_trench_export_mode']=s._normalize_choice(value.get('dxf_trench_export_mode',defaults.dxf_trench_export_mode),s.DXF_TRENCH_EXPORT_OPTIONS,defaults.dxf_trench_export_mode)
 out['template_client_logo_preset']=s._normalize_choice(value.get('template_client_logo_preset',defaults.template_client_logo_preset),s.TEMPLATE_CLIENT_LOGO_PRESET_OPTIONS,defaults.template_client_logo_preset)
 out['template_cross_section_marker_diameter']=s._normalize_float(value.get('template_cross_section_marker_diameter',.02),.02,.005,.2)
 return out
cases=[]
values=[None,False,True,0,1,-2,.25,' ON ','off','aan','uit','yes','nee','  Polygon ','custom',' #ab90cd ','bad','0.005','0.9','inf','nan','٣.٠',[],{},'\x1ctest\x1f']
keys=flags+texts+colors+['dxf_trench_export_mode','template_client_logo_preset','template_cross_section_marker_diameter']
for value in values:
 raw={k:value for k in keys}
 cases.append(dict(raw=raw,expected=expected(raw)))
for preset,_ in s.TEMPLATE_CLIENT_LOGO_PRESET_OPTIONS:
 raw={'template_client_logo_preset':preset,'dxf_trench_export_mode':'none'}
 cases.append(dict(raw=raw,expected=expected(raw)))
choices=[[],[' Straße ','STRASSE','é',123,True,None,0,False,{},[],{'a':1}],['A','a','\x1cB\x1f','B']]
payload=dict(cases=cases,choices=[dict(raw=v,expected=s.normalize_kickthemap_material_choices(v)) for v in choices],mappings=[dict(raw={' Straße ':'Water','STRASSE':'Gas',' A\x1cB ':' X ', 'Empty':None,'Numeric':123},expected=s.normalize_marxact_name_mappings({' Straße ':'Water','STRASSE':'Gas',' A\x1cB ':' X ', 'Empty':None,'Numeric':123}))])
out=Path(os.environ.get('SLEUFBASE_PREFERENCES_FIXTURE',ROOT/'rust/tests/fixtures/preferences.json'))
out.write_text(json.dumps(payload,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(f'{len(cases)} original preference states and Unicode/list/mapping contracts: {out}')
