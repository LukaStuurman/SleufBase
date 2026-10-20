"""Original source dialog's pure normalization/order helpers; no GUI/login."""
from pathlib import Path
from types import SimpleNamespace
import ast,inspect,json,os,random,re
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_ORDER_FIXTURES',ROOT/'rust/tests/fixtures/template-order'))
def main():
    source=ast.parse((ROOT/'app.py').read_text(encoding='utf-8'))
    node=next(n for n in source.body if isinstance(n,ast.FunctionDef) and n.name=='_install_template_variant_order_patch')
    class Viewer:
        def _normalize_template_export_slot_entries(self,v):return v
        def _store_template_export_order_state(self,*args):pass
        def _template_export_layer_key(self,layer):return str(layer.path)
    env=dict(KlicViewerApp=Viewer,re=re,__package__='_sleufbase_export_reference')
    exec(compile(ast.Module(body=[node],type_ignores=[]),str(ROOT/'app.py'),'exec'),env);env[node.name]()
    closure=inspect.getclosurevars(Viewer._template_export_variant_labels).nonlocals
    number=closure['_normalize_ps_number'];variant=closure['_normalize_variant_code']
    ref=reference();import importlib
    exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    viewer=Viewer();viewer.cadastral_exporter=exporter;viewer.template_export_slot_order=[]
    numbers=['','0','PS0','00012','ps 001',' PS\t１２ ','PS+1','PS-1','1.0','١٢','𝟘𝟟','𞥑𞥙','\u200b1','\u00a01\u00a0','9'*80,'PS\n1','PSſ1']
    variants=[None,True,False,0,1,-2,0.1,[],[1],{},dict(x=1),'',' ','0','false','Nee','NO','off','geen','auto','Automatisch','ja','true','1','a','ß','ﬀ','ſ','ı','İ','K','B/C','123','??',' B ']
    names=['fase_2_ps_21_66587.tiff','PS01B.tif','ps-۱۲AA.tiff','PS1AUTO.tif','PS1B\u200d.tif','PS1B́.tif','PS1ſ.tif','PS1İ.tif','PS1K.tif','no digits.tif','abc12.tif','Aps12B.tif','Kps12B.tif','ſps12B.tif','♠ps2C.tif','ps_0002_Z.tif','PS١٢B.tif','PS1BBB.tif','PS1B-PS2C.tif','xPS1A_ps2B.tif']
    filenames=[]
    for i,name in enumerate(names):
        path=Path('fixture/2026')/name;layer=SimpleNamespace(path=path,metadata={})
        filenames.append(dict(path=path.as_posix(),index=i+1,number=viewer._default_template_export_ps_number(layer,i+1),variant=viewer._template_export_variant_codes_for_order([layer])[0]))
    cases=[];rng=random.Random(417)
    for i in range(140):
        rows=[]
        for j in range(rng.randint(1,35)):
            rows.append(dict(raster=None if rng.randrange(7)==0 else f'PS{j}.tif',included=bool(rng.randrange(3)),number=rng.choice(numbers),variant=rng.choice(['','AUTO','B','C','AA','false','??','ß'])))
        labels,invalid=Viewer._template_export_variant_labels([None if r['raster'] is None else object() for r in rows],[r['included'] for r in rows],[r['variant'] for r in rows],[r['number'] for r in rows])
        cases.append(dict(rows=rows,labels=labels,invalid=invalid))
    data=dict(numbers=[dict(input=s,expected=number(s) or None) for s in numbers],variants=[dict(input=v,expected=variant(v)) for v in variants],filenames=filenames,cases=cases,suffixes=[dict(input=n,expected=Viewer._template_export_variant_suffix(n)) for n in [0,1,2,25,26,27,52,702,703,5000]])
    moves=[];move_node=next(n for n in ast.walk(node) if isinstance(n,ast.FunctionDef) and n.name=='move_selected')
    for i in range(64):
        count=rng.randint(2,35);selection=[j for j in range(count) if rng.randrange(3)==0];offset=rng.choice([-1,1]);captured={}
        scope=dict(ordered_layers=list(range(count)),included_flags=[True]*count,scale_denominators=[None]*count,ps_numbers=[str(i) for i in range(count)],variant_codes=['']*count,selected_indices=lambda:selection,refresh_listbox=lambda selected:captured.update(selection=selected))
        exec(compile(ast.Module(body=[move_node],type_ignores=[]),str(ROOT/'app.py'),'exec'),scope);scope['move_selected'](offset)
        moves.append(dict(count=count,selection=selection,offset=offset,order=scope['ordered_layers'],expected_selection=captured.get('selection',selection)))
    data['moves']=moves;comments=[]
    for i in range(48):
        bounds=ref.Bounds(0.,0.,10.+i,6.) if hasattr(ref,'Bounds') else importlib.import_module(ref.__package__+'.models').Bounds(0.,0.,10.+i,6.)
        values=[dict(x=rng.choice([0.,-0.5,-2.,10.,10.+i,11.+i,500.]),y=rng.choice([0.,3.,6.,8.,500.]),text=rng.choice(['  Grondwater\nGrondwater  ','Straße\r\nSTRASSE','A\vB\fC\x1cD\x1dE\x1eF\x85G\u2028H\u2029I','Σ\nς\nσ','\x1f A \x1f',' Café Ø ',''])) for _ in range(12)]
        layer=SimpleNamespace(bounds=bounds);expected=exporter._template_comments_text(layer,[SimpleNamespace(**v) for v in values]);comments.append(dict(bounds=dict(min_x=bounds.min_x,min_y=bounds.min_y,max_x=bounds.max_x,max_y=bounds.max_y),comments=values,expected=expected))
    data['comments']=comments
    OUT.mkdir(parents=True,exist_ok=True);(OUT/'reference.json').write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('Original template order references:',len(cases),'orders,',len(filenames),'filenames')
if __name__=='__main__':main()
