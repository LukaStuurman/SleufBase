"""Isolated original Gemma protocol/parser/map-bounds/runtime command oracles.

No app, settings, accounts, GPU/service, model download or real subprocess.
requests and Popen are captured fakes; map renderer is a blank surface recorder.
"""
from pathlib import Path
from dataclasses import asdict
from unittest.mock import patch
from types import SimpleNamespace
import importlib,json,os,tempfile
from PIL import Image
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_AI_FIXTURES',ROOT/'rust/tests/fixtures/ai-maaiveld'))
def main():
    ref=reference();ai=importlib.import_module(ref.__package__+'.ai_maaiveld');runtime=importlib.import_module(ref.__package__+'.vllm_runtime');models=importlib.import_module(ref.__package__+'.models')
    raw=['{"1":"  Maaiveld: -2,31 N.A.P.  ","2":"INVULLEN","3":"steen"}','```JSON\n{"1":0,"2":false,"3":[1,true,null]}\n```','tekst {"1":5,"2":{"a":true},"3":""} einde','{}','{"1":"\\u001c\\u00a0 tekst \\u001f"}','[]','not json','{"1":"goed"} trailing']
    cases=[]
    for text in raw:
        try:cases.append(dict(raw=text,expected=ai._parse_maaiveld_json(text)))
        except ai.AiMaaiveldError:cases.append(dict(raw=text,error=True))
    captured={}
    def post(url,**kwargs):captured.update(url=url,**kwargs);return SimpleNamespace(raise_for_status=lambda:None,json=lambda:{'choices':[{'message':{'content':[{'text':'{"1":"a",'}, {'text':'"2":"b","3":"c"}'}]}}]})
    with patch.object(ai,'ensure_vllm_server',lambda **kw:kw['base_url']),patch.object(ai.requests,'post',post):
        result=ai.request_maaiveld_from_vllm(Image.new('RGB',(2,2),(12,34,56)),model='gemma-fixture',base_url='http://localhost:9876/v1/',api_key='test-key',timeout_seconds=17)
    # PNG compression is intentionally not a wire-level dependency; decoded
    # image pixels, exact prompt and all remaining JSON fields are contractual.
    image_url=captured['json']['messages'][0]['content'][1]['image_url']['url'];captured['json']['messages'][0]['content'][1]['image_url']['url']='PNG_DATA_URL'
    layout=[]
    for bounds,coords,size in [([0,0,10,4],[(0,0),(5,2),(10,4)],(1400,1000)),([155000,463000,155020,463010],[(154998,463002),(155010,463005),(155030,463007)],(1000,1400))]:
        points=[ai.AiMaaiveldPoint(k,l,*p) for (k,l),p in zip([('start','Beginpunt'),('middle','Middenpunt'),('end','Eindpunt')],coords)]
        call={}
        def render(b,s,*args,**kwargs):call.update(bounds=asdict(b),size=list(s));return Image.new('RGBA',s,(80,90,100,255))
        image=ai.build_numbered_maaiveld_image(layer=SimpleNamespace(bounds=models.Bounds(*bounds)),points=points,renderer=SimpleNamespace(render=render),size=size)
        layout.append(dict(layer=dict(zip(['min_x','min_y','max_x','max_y'],bounds)),points=[asdict(p) for p in points],size=list(size),expected=call['bounds']))
        image.close()
    launches=[]
    with tempfile.TemporaryDirectory() as temp:
        root=Path(temp);exe=root/'.tools/vllm/Scripts/vllm.exe';exe.parent.mkdir(parents=True);exe.touch()
        for overrides in [{},{'SLEUFBASE_VLLM_MODEL':'test/AWQ','SLEUFBASE_VLLM_ENFORCE_EAGER':'nee','SLEUFBASE_VLLM_EXTRA_ARGS':'  --seed 7  --disable-log-requests ','SLEUFBASE_VLLM_DTYPE':'float16','SLEUFBASE_VLLM_QUANTIZATION':'custom'}]:
            calls=[];ready=iter([False,True]);env={'PATH':'fixture-path',**overrides}
            def popen(command,**kw):calls.append(dict(command=command,cwd=kw['cwd'],env=kw['env']));return SimpleNamespace()
            with patch.dict(os.environ,env,clear=True),patch.object(runtime,'_workspace_root',lambda:root),patch.object(runtime,'_server_ready',lambda url:next(ready)),patch.object(runtime.subprocess,'Popen',popen):
                runtime.ensure_vllm_server()
            item=calls[0]
            launches.append(dict(overrides=overrides,args=[s.replace(str(root),'ROOT') for s in item['command'][1:]],env={k:v.replace(str(root),'ROOT') for k,v in item['env'].items() if k!='PATH'}))
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'reference.json').write_text(json.dumps(dict(parse=cases,protocol=captured,response=asdict(result),layout=layout,launches=launches),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('8 original parser cases, exact prompt/protocol, 2 numbered-map layouts, 2 fake runtime launches')
if __name__=='__main__':main()
