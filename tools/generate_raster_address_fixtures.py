"""Invoke only original validated row/copy functions on synthetic objects."""
from pathlib import Path
from types import CodeType,FunctionType,SimpleNamespace
import importlib.util,json,math,os,random
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_RASTER_ADDRESS_FIXTURES',ROOT/'rust/tests/fixtures/raster-addresses'))
SEED=6619
COUNT=48
def cell(value):return (lambda: value).__closure__[0]
def main():
    spec=importlib.util.spec_from_file_location('raster_address_legacy_helper',ROOT/'legacy_bytecode.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    root=helper.read_validated_code('app',ROOT/'legacy_bytecode.py')
    cls=next(c for c in root.co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    row=FunctionType(next(c for c in cls.co_consts if isinstance(c,CodeType) and c.co_name=='_lookup_tiff_address_record'),{})
    dialog=next(c for c in cls.co_consts if isinstance(c,CodeType) and c.co_name=='_open_tiff_addresses_dialog')
    copy=next(c for c in dialog.co_consts if isinstance(c,CodeType) and c.co_name=='copy_selected')
    rng=random.Random(SEED)
    cases=[]
    distances=[0.,-0.,1.25,math.nextafter(1.25,0),math.nextafter(1.25,2),.05,.15,99.95,-.15]
    for n in range(COUNT):
        name=['scan.tif','Proefsleuf Ø.tiff','scan.tif','x'*800+'.tif'][n%4]
        center=[168000.125+n*.37,401000.375-n*.23]
        response=[] if n%7==1 else [dict(label=f'Fictief adres {n} Ø, 1234AB Testplaats',distance_m=distances[n%len(distances)] if n<18 else rng.uniform(-1,150),location_type='adres'),
            dict(label='Must not be used',distance_m=0.,location_type='adres')]
        error='Tijdelijke netwerkfout Ø' if n%7==2 else None
        calls=[]
        def reverse(x,y,rows):
            calls.append([x,y,rows])
            if error:raise RuntimeError(error)
            return [SimpleNamespace(**r) for r in response]
        result=row(SimpleNamespace(location_client=SimpleNamespace(reverse_lookup=reverse)),SimpleNamespace(name=name,bounds=SimpleNamespace(center_x=center[0],center_y=center[1])))
        copies=[]
        for include_name,selected in [(False,True),(True,True),(False,False)]:
            clipboard=[];status=[]
            workspace=SimpleNamespace(clipboard_clear=lambda:clipboard.clear(),clipboard_append=clipboard.append,
                update_idletasks=lambda:None,set_status=status.append)
            values={'record_map':{'row':result},'self':workspace,'tree':SimpleNamespace(selection=lambda:['row'] if selected else [])}
            fn=FunctionType(copy,{},argdefs=(False,),closure=tuple(cell(values[key]) for key in copy.co_freevars))
            fn(include_name)
            copies.append(dict(include_name=include_name,selected=selected,clipboard=clipboard,status=status))
        cases.append(dict(source=dict(name=name,center=center,epsg=28992),response=response,error=error,record=result,copies=copies,calls=calls))
    OUT.mkdir(parents=True,exist_ok=True)
    result=dict(cases=cases)
    (OUT/'reference.json').write_text(json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original TIFF-address rows / {len(cases)*3} actual clipboard actions; errors, empty/multiple results, duplicate/long Unicode names and decimal rounding')
    return result
if __name__=='__main__':main()
