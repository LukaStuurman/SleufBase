"""Paired worker output: both single-direction contents/pixels and original CAD graph."""
from pathlib import Path
import hashlib,json,re,sys,tempfile
import ezdxf
from PIL import Image
from generate_template_page_fixtures import entity
from generate_template_variant_fixtures import modules,snapshot
from verify_native_template_extents import compare

IMAGE=re.compile(r'(slot\d+_(?:tiff|kaart|photo\d+))(?:_normaal|_reverse)?\.png$')

def image_role(e,doc):
    path=Path(doc.entitydb[e.dxf.image_def_handle].dxf.filename);match=IMAGE.fullmatch(path.name)
    assert match,path
    return match[1],path

def content(entities,doc):
    indices={}
    for i,e in enumerate(entities):
        indices[e.dxf.handle]=f'parent:{i}'
        for j,c in enumerate(e.attribs if e.dxftype()=='INSERT' else e.vertices if e.dxftype()=='POLYLINE' else []):indices[c.dxf.handle]=f'parent:{i}/child:{j}'
    result=[]
    def normalize(e):
        out=entity(e,indices,doc)
        if e.dxftype()=='IMAGE':
            role,path=image_role(e,doc);out['attrs']['image_def_handle']=role
            with Image.open(path) as im:im=im.convert('RGBA');out['pixels']=[list(im.size),hashlib.sha256(im.tobytes()).hexdigest()]
        def handles(obj):
            if isinstance(obj,dict):
                for k,v in obj.items():
                    if k=='annotation_handle':obj[k]=indices.get(v,'shared:'+v)
                    elif k=='reactors':obj[k]=['independent reactor']*len(v)
                    elif k=='xdata':
                        for tags in v.values():
                            for tag in tags:
                                if tag[0]==1005:tag[1]=indices.get(tag[1],'shared:'+tag[1])
                    else:handles(v)
            elif isinstance(obj,list):
                for v in obj:handles(v)
        handles(out);return out
    for e in entities:result.append(normalize(e))
    return result

def single_groups(doc):
    result={};current=None
    for e in doc.modelspace():
        if e.dxftype()=='IMAGE':
            role,_=image_role(e,doc)
            if role.endswith('_tiff'):current=int(role[4:9]);result[current]=[]
        if current is not None:result[current].append(e)
    return result

def original_graph(path,wrappers,doc,baseline_fixes):
    dv,reverse,dynamic,pipeline,finalize=modules()
    pairs,nl,bom=dv._read_pairs(path);records=dv._split_records(pairs);sections=dv._record_sections(records)
    roots=set()
    for name in wrappers:
        _,br=dv._target_block_record(records,sections,name);roots.add(finalize._extension_dictionary_handle(br))
    owned=set(roots)
    while True:
        expanded=owned|{dv._record_handle(r) for r,s in zip(records,sections) if s=='OBJECTS' and dv._record_owner(r) in owned}
        if expanded==owned:break
        owned=expanded
    # Replace only the native graph with one independently constructed by the
    # original promotion/finalization functions, retaining all entity handles.
    records=[r for r,s in zip(records,sections) if not(s=='OBJECTS' and dv._record_handle(r) in owned)]
    sections=dv._record_sections(records)
    promoted,next_handle=pipeline._promote_records(records,sections,wrappers,next_handle=max(dv._max_handle_value(pairs)+1,dv._header_handseed(pairs)))
    assert promoted==len(wrappers)
    pipeline._set_handseed_in_records(records,next_handle)
    locations=pipeline._selector_locations_from_document(doc,wrappers)
    assert pipeline._finalize_records(records,dv._record_sections(records),wrappers,locations)==len(wrappers)
    with tempfile.TemporaryDirectory(prefix='SleufBase-original-pair-graph-') as temp:
        oracle=Path(temp)/'original.dxf';dv._write_pairs(oracle,dv._flatten_records(records),newline=nl,had_bom=bom)
        compare(snapshot(path,wrappers,baseline_fixes),snapshot(oracle,wrappers,baseline_fixes),'original graph/grip '+path.parent.name)

def main():
    out=Path(sys.argv[1]);total=0;profiles=0
    for name in ['profiles','photos-no-draw','virtual','image-only','extended','stroke-font','lff-font','bigfont-fallback']:
        folder=out/name;paired=ezdxf.readfile(folder/'paired.dxf')
        request=json.loads((folder/'paired-input.json').read_text(encoding='utf-8'))
        # The approved source contains one existing owner-0 proxy that ezdxf's
        # auditor removes. Preserve its original tags; forbid every new repair.
        dv,*_=modules();source_pairs,*_=dv._read_pairs(Path(request['template']));source_records=dv._split_records(source_pairs)
        actual_pairs,*_=dv._read_pairs(folder/'paired.dxf');actual_records=dv._split_records(actual_pairs)
        originals={dv._record_handle(r):r for r in source_records if dv._record_type(r)=='ACAD_PROXY_OBJECT'}
        actual_by_handle={dv._record_handle(r):r for r in actual_records}
        for h,r in originals.items():assert actual_by_handle.get(h)==r,('source proxy changed',h)
        source=ezdxf.readfile(request['template']);baseline=source.audit();assert not baseline.has_errors
        baseline_fixes=[(e.code,e.message) for e in baseline.fixes]
        report=paired.audit();assert not report.has_errors and [(e.code,e.message) for e in report.fixes]==baseline_fixes,(name,report.errors,[(e.code,e.message) for e in report.fixes])
        _,reverse,dynamic,_,_=modules()
        wrappers=[dynamic.dynamic_block_name(slot['label'],i+1) for i,slot in enumerate(request['slots']) if slot is not None]
        model_refs=[e.dxf.name for e in paired.modelspace() if e.dxftype()=='INSERT' and e.dxf.name in wrappers]
        assert model_refs==sorted(wrappers),(name,'wrapper ordering')
        for mode,suffix in [('normal','_NORMAAL_CONTENT'),('reverse','_REVERSE_CONTENT')]:
            single=ezdxf.readfile(folder/(mode+'.dxf'));expected=single_groups(single)
            assert len(expected)==len(wrappers)
            for i,slot in enumerate(request['slots']):
                if slot is None:continue
                wrapper=dynamic.dynamic_block_name(slot['label'],i+1)
                ref=next(e for e in paired.blocks.get(wrapper).query('INSERT') if e.dxf.name.endswith(suffix))
                actual=list(paired.blocks.get(ref.dxf.name))
                compare(content(actual,paired),content(expected[i+1],single),f'{name}/{mode}/slot{i+1}')
                profiles+=sum(e.dxftype()=='LEADER' for e in actual)
            # Companion counts/order are per selected slot, not doubled by the
            # two states. Compare actual XML cells/style/worksheet components.
            import zipfile
            with zipfile.ZipFile(folder/'paired.xlsx') as a,zipfile.ZipFile(folder/(mode+'.xlsx')) as b:
                assert sorted(a.namelist())==sorted(b.namelist())
                for member in a.namelist():
                    left,right=a.read(member),b.read(member)
                    if member=='docProps/core.xml':
                        import xml.etree.ElementTree as ET
                        from datetime import datetime
                        roots=[ET.fromstring(v) for v in (left,right)]
                        for root in roots:
                            for node in root.iter():
                                if node.tag.rsplit('}',1)[-1] in ('created','modified'):
                                    datetime.fromisoformat(node.text.replace('Z','+00:00'));node.text='{timestamp}'
                        left,right=(ET.tostring(root) for root in roots)
                    assert left==right,(name,mode,member)
        if name=='stroke-font':
            from unittest.mock import patch
            from ezdxf.fonts import fonts,shapefile
            cache=shapefile.GlyphCache(shapefile.readfile(str(folder/'cad-fonts/fixture.shx')))
            original=fonts.font_manager.get_shapefile_glyph_cache
            with patch.object(fonts.font_manager,'get_shapefile_glyph_cache',side_effect=lambda font:cache if font.lower()=='fixture.shx' else original(font)):
                original_graph(folder/'paired.dxf',wrappers,paired,baseline_fixes)
        elif name=='lff-font':
            from unittest.mock import patch
            from ezdxf.fonts import fonts,lff
            cache=lff.GlyphCache(lff.loads((folder/'cad-fonts/fixture.lff').read_text(encoding='utf-8')))
            original=fonts.font_manager.get_lff_glyph_cache
            with patch.object(fonts.font_manager,'get_lff_glyph_cache',side_effect=lambda font:cache if font.lower()=='fixture.lff' else original(font)):
                original_graph(folder/'paired.dxf',wrappers,paired,baseline_fixes)
        elif name=='bigfont-fallback':
            from unittest.mock import patch
            from ezdxf.fonts import fonts
            from ezdxf.fonts.font_manager import FontManager
            manager=FontManager();manager.build();manager.build([str(folder/'cad-fonts')],support_dirs=False)
            assert manager.has_font('big.shx')
            with patch.object(fonts,'font_manager',manager):
                original_graph(folder/'paired.dxf',wrappers,paired,baseline_fixes)
        else:original_graph(folder/'paired.dxf',wrappers,paired,baseline_fixes)
        total+=len(wrappers)
    print(f'8 real paired exports / {total} dynamic pairs: both directions match singles in complete geometry, linked attributes, image pixels and Excel; original promotion/grip metadata match, including SHX/LFF fonts and BIGFONT factory fallback, with no new audit repairs ({profiles} leaders)')

if __name__=='__main__':main()
