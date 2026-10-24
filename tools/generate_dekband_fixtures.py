"""Original declared deck-band profile injection, without app/settings/accounts.

Calls the original CadastralDxfExporter polyline builder against its actual
base profiles. No live-render patch or native geometry is imported.
"""
from pathlib import Path
import importlib,json,os
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_DEKBAND_FIXTURES',ROOT/'rust/tests/fixtures/dekband'))

def main():
    ref=reference()
    mod=importlib.import_module(ref.__package__+'.cadastral_export')
    importlib.import_module(ref.__package__+'.dxf_template_pipeline_v9_patch').install_dxf_template_pipeline_v9_patch()
    exporter=mod.CadastralDxfExporter(wfs_client=None)
    rules=ref.build_object_layer_rules()
    a=dict(x=155000.,y=463000.,z=2.3)
    b=dict(x=155006.,y=463008.,z=5.1)
    rows=[dict(start_chainage=2.,end_chainage=7.,start_depth=.25,end_depth=.85,source_name='  Dekband  '),
          dict(start_chainage=-3.,end_chainage=15.,start_depth=.8,end_depth=.1,source_name='Betondekband Ø'),
          dict(start_chainage=12.,end_chainage=14.,start_depth=.2,end_depth=.9,source_name='skip'),
          dict(start_chainage=1.,end_chainage=3.,start_depth=.3,end_depth=.4,source_name='')]
    points=tuple(ref.KickTheMapObjectPoint(name,'PUNT',**v) for name,v in [('Beginpunt',a),('Eindpunt',b)])
    cases=[]
    for reverse in [False,True]:
        dataset=ref.KickTheMapObjectDataset(73,'Deck oracle',Path('fixture.json'),points,(),tuple((b if reverse else a)[k] for k in ['x','y']))
        profile=exporter._build_template_cross_section_profile(dataset,rules,[],[],.1,False)
        assert profile is not None
        expected=[]
        for index,row in enumerate(rows,1):
            line=exporter._template_dekband_polyline(row,profile,index)
            if line is not None:
                expected.append(dict(object_name=line.object_name,source_name=line.source_name,
                    geometry=dict(Polyline=[dict(x=p.x,y=p.y,z=p.z) for p in line.vertices]),
                    attributes=[line.attribute_1,line.attribute_2,line.attribute_3]))
        cases.append(dict(reverse=reverse,source=dict(label='Deck oracle',job_id=73,width_meters=.6,start=a,end=b,dekband_rows=rows),expected=expected))
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'reference.json').write_text(json.dumps(dict(cases=cases),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('2 original profile directions, asymmetric depths, clipping, skipped segment and names')
if __name__=='__main__':main()
