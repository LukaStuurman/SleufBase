"""Fresh original comment batches against the optimized standalone native driver."""
from pathlib import Path
from tempfile import TemporaryDirectory
import copy,json,subprocess,sys
import generate_map_action_fixtures as oracle
def same(actual,expected):
    if isinstance(expected,dict):
        assert actual.keys()==expected.keys(),(actual.keys(),expected.keys())
        for k in expected:same(actual[k],expected[k])
    elif isinstance(expected,(list,tuple)):
        assert len(actual)==len(expected),(len(actual),len(expected))
        for a,b in zip(actual,expected):same(a,b)
    elif isinstance(expected,(int,float)) and not isinstance(expected,bool):assert abs(actual-expected)<2e-8,(actual,expected)
    else:assert actual==expected,(actual,expected)
def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-map-actions-runtime-') as temp:
        folder=Path(temp);oracle.OUT=folder/'original';oracle.SEED=71300;oracle.COUNT=73;cases=oracle.main()['cases']
        source=folder/'input.json';target=folder/'output.json';source.write_text(json.dumps(cases,ensure_ascii=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'))
        keys=['texts','combined','formatted','after_edit','after_selected_clear','after_clear','clear_status']
        same(actual,[{k:c[k] for k in keys} for c in cases])
        print(f'{len(cases)} fresh original complete comment/edit/clear batches / {len(cases)*3} affine TIFF notes match optimized native code within 0.00000002 m')
        if len(sys.argv)>2:
            import ezdxf
            from verify_native_template_edits import records
            root=Path(__file__).resolve().parents[1];binary=str(Path(sys.argv[2]).resolve())
            original=next(c['request'] for c in json.loads((root/'rust/tests/fixtures/template-export/reference.json').read_text(encoding='utf-8')) if c['name']=='native-tiff')
            note='Opmerkingen:\\PMeetpunt Ø\tKolom'
            for paired in [False,True]:
                request=copy.deepcopy(original);request['template']=str(root/request['template']);request['settings']['paired']=paired;request['slots']=request['slots'][:1]
                slot=request['slots'][0];slot['profile']=None;slot['comments']=note;slot['tiff_source']['path']=str(root/slot['tiff_source']['path'])
                slot.pop('map_png');slot['bounds']=dict(min_x=0.,min_y=0.,max_x=8.,max_y=6.)
                slot['map_source']=dict(background=dict(kind='solid',color=[255,255,255,255]),settings=dict(dpi=72,scale=225.))
                manifest=folder/f'tabs-{paired}.json';drawing=folder/f'tabs-{paired}.dxf';manifest.write_text(json.dumps(request,ensure_ascii=False),encoding='utf-8')
                command=[binary,'--template-dxf',str(drawing),str(manifest)];result=subprocess.run(command,capture_output=True,text=True,encoding='utf-8',timeout=120)
                assert result.returncode==0,(result.returncode,result.stdout,result.stderr)
                doc=ezdxf.readfile(drawing);assert not doc.audit().has_errors
                assert sum(e.dxftype()=='MTEXT' and e.text==note for e in doc.entitydb.values())==1,'Single shared note must survive in both export modes'
                if paired:
                    count=lambda path:sum(r[0]==(0,'BLOCKVISIBILITYPARAMETER') and (301,'Versie') in r for r in records(path))
                    assert count(drawing)>count(root/original['template'])
                before=drawing.read_bytes();again=subprocess.run(command,capture_output=True,text=True,encoding='utf-8',timeout=30);assert again.returncode==2 and drawing.read_bytes()==before
            print('Real optimized CLI: original internal MTEXT tabs persist in single and paired exports, CAD audit passes and prior output remains protected')
if __name__=='__main__':main()
