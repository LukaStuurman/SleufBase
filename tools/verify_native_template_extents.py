"""Independent native readback, including original metrics of installed fonts."""
from pathlib import Path
import json,subprocess,sys,tempfile
import ezdxf
from ezdxf import bbox
from ezdxf.disassemble import make_primitive,recursive_decompose,get_font_name
from ezdxf.fonts import fonts
from ezdxf.tools.text import plain_text

ROOT=Path(__file__).resolve().parents[1]

def compare(a,b,path='result'):
    if isinstance(a,(int,float)) and isinstance(b,(int,float)):
        assert abs(a-b)<=1e-12+32*sys.float_info.epsilon*max(abs(a),abs(b)),(path,a,b)
    elif isinstance(a,list) and isinstance(b,list):
        if path.endswith('.corners'):
            for left,right in [(a,b),(b,a)]:
                for p in left:assert any(all(abs(x-y)<=1e-12+32*sys.float_info.epsilon*max(abs(x),abs(y)) for x,y in zip(p,q)) for q in right),(path,p,right)
        else:
            assert len(a)==len(b),(path,'length')
            for i,(x,y) in enumerate(zip(a,b)):compare(x,y,f'{path}[{i}]')
    elif isinstance(a,dict) and isinstance(b,dict):
        assert a.keys()==b.keys(),(path,'keys')
        for k in a:compare(a[k],b[k],path+'.'+k)
    else:assert a==b,(path,a,b)

def run(exe,request,expected,label):
    with tempfile.TemporaryDirectory(prefix='SleufBase-metric-audit-') as temp:
        inp=Path(temp)/'input.json';out=Path(temp)/'output.json'
        inp.write_text(json.dumps(request,ensure_ascii=False),encoding='utf-8')
        r=subprocess.run([str(exe),str(inp),str(out)],capture_output=True,text=True,timeout=120)
        assert r.returncode==0,(label,r.stderr[-1800:])
        compare(json.loads(out.read_text(encoding='utf-8')),expected,label)

def assembled_profiles(exe,directory):
    import importlib
    from generate_object_export_fixtures import reference
    ref=reference();pipeline=importlib.import_module(ref.__package__+'.dxf_template_pipeline_patch')
    source=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf');old=set(source.entitydb.keys());del source
    count=0
    fixture_cases=json.loads((ROOT/'rust/tests/fixtures/template-export/reference.json').read_text(encoding='utf-8'))
    for case in fixture_cases:
        path=directory/(case['name']+'.dxf');assert path.is_file(),path
        doc=ezdxf.readfile(path);groups=[];current=[];images=0;started=False
        for e in doc.modelspace():
            if e.dxf.handle in old:continue
            kind=e.dxftype()
            # Global overview geometry and copied sheet objects precede the
            # collected slot images/profiles; production never wraps those.
            if kind=='IMAGE':started=True
            if not started:continue
            if kind=='INSERT' and not (e.dxf.name.startswith('SAL-VERWIJZING_PROFIEL') or e.dxf.name=='SAL-VERWIJZING_LEIDING_BOVENKANT_INFO_DIEPTE-SOD'):continue
            if kind not in ('IMAGE','INSERT','LINE','LWPOLYLINE','CIRCLE','LEADER','TEXT','POINT'):continue
            if kind=='IMAGE':
                if images==2:groups.append(current);current=[];images=0
                images+=1
            current.append(e)
        if current:groups.append(current)
        assert len(groups)==sum(slot is not None for slot in case['request']['slots']),(path.name,'incomplete slot coverage',len(groups))
        names={get_font_name(e) for group in groups for e in recursive_decompose(group) if e.dxftype() in ('TEXT','ATTRIB','ATTDEF')}
        installed={}
        for name in names:
            font=fonts.make_font(name,1.)
            if isinstance(font,fonts.MonospaceFont):installed[name]='*monospace'
            else:
                assert isinstance(font,fonts.TrueTypeFont),(path.name,name,'requires a native shape-font implementation')
                installed[name]=str(fonts.font_manager._font_cache.get(font.name,fonts.font_manager.fallback_font_name()).file_path)
        expected=[]
        for i,group in enumerate(groups):
            b=bbox.extents(group,fast=True)
            wrapper=doc.blocks.new(f'_SLEUFBASE_METRIC_ORACLE_{i}')
            content=doc.blocks.new(wrapper.name+'_NORMAAL_CONTENT')
            for e in group:content.add_entity(e.copy())
            wrapper.add_blockref(content.name,(0,0))
            selector=pipeline._selector_locations_from_document(doc,[wrapper.name]).get(wrapper.name)
            expected.append(dict(bounds=dict(min=list(b.extmin),max=list(b.extmax)) if b.has_data else None,selector=list(selector) if selector is not None else None))
            doc.blocks.delete_block(wrapper.name,safe=False);doc.blocks.delete_block(content.name,safe=False)
        run(exe,dict(fonts=installed,drawing=str(path.resolve()),groups=[[int(e.dxf.handle,16) for e in group] for group in groups]),dict(texts=[],groups=expected),'assembled '+path.name)
        count+=len(groups)
    assert count>0,'No assembled drawings audited'
    return count

def main():
    exe=Path(sys.argv[1]).resolve()
    metrics=ROOT/'rust/tests/fixtures/dxf-text-metrics';geometry=ROOT/'rust/tests/fixtures/template-extents'
    cases=json.loads((metrics/'reference.json').read_text(encoding='utf-8'))['cases']
    groups=json.loads((geometry/'reference.json').read_text(encoding='utf-8'))
    run(exe,dict(fonts={'ttf':str(metrics/'SleufBaseFixture.ttf'),'mono':'*monospace','*monospace':'*monospace','SleufBaseFixture.ttf':str(metrics/'SleufBaseFixture.ttf')},texts=[dict(font=c['font'],text=c['text']) for c in cases],drawing=str(geometry/'input.dxf'),groups=[c['handles'] for c in groups]),dict(texts=[c['expected'] for c in cases],groups=[dict(bounds=c['bounds'],selector=c['selector']) for c in groups]),'frozen source fixtures')
    # Resolve through the original font manager, then give native code the
    # exact same file. This checks runtime font versions without redistributing
    # proprietary files or baking one machine's fallback choices into fixtures.
    installed={};texts=[];expected=[];doc=ezdxf.new('R2018')
    for name in ['ArialUni.ttf','Arial.ttf','SegoeUI.ttf','txt_IV25.ttf','NLCS-ISO.ttf','LiberationMono-Regular.ttf']:
        for i,content in enumerate(['Axp MØ','Water Ø 110 PVC','   ','AV To 12345','Straße 漢🛠','%%c %%d %%p']):
            h=[.25,1.,2.5][i%3];w=[.7,1.4][i%2]
            font=fonts.make_font(name,h,w)
            if not isinstance(font,fonts.TrueTypeFont):continue
            cache=fonts.font_manager._font_cache.get(font.name,fonts.font_manager.fallback_font_name())
            installed[name]=str(cache.file_path)
            doc.styles.get('Standard').dxf.font=name
            e=doc.modelspace().add_text(content,dxfattribs=dict(insert=(10,20,3),align_point=(16,28,5),height=h,width=w,rotation=23,oblique=-11,halign=i%3,valign=i%4,extrusion=(1,2,3)))
            d=e.dxf
            spec=dict(content=d.text,insert=list(d.insert),align_point=list(d.align_point),height=h,width=w,rotation=d.rotation,oblique=d.oblique,halign=d.halign,valign=d.valign,normal=list(d.extrusion))
            path=make_primitive(e).path;verts=list(path.control_vertices()) if path else []
            texts.append(dict(font=name,text=spec));expected.append(dict(width=font.text_width(plain_text(content)),measurements=dict(zip(font.measurements._fields,font.measurements)),corners=[list(v) for v in verts[:4]] if verts else None))
    assert len(texts)==36,('runtime TTF coverage missing',len(texts))
    run(exe,dict(fonts=installed,texts=texts,drawing=None),dict(texts=expected,groups=[]),'runtime original font files')
    assembled=assembled_profiles(exe,Path(sys.argv[2])) if len(sys.argv)>2 else 0
    print(f'{len(cases)} original text extents / {len(groups)} complete geometry/selector cases / {len(texts)} runtime font cases / {assembled} assembled groups match native output')

if __name__=='__main__':main()
