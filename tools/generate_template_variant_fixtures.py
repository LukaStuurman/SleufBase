"""Original normal/reverse containers and AutoCAD donor graph, offline.

The small envelope carries the actual eight metadata records from the approved
CAD template. Geometry is synthetic; no app/settings/account is initialized.
"""
from pathlib import Path
from unittest.mock import patch
from uuid import UUID
import importlib,json,os
import ezdxf
from PIL import Image
from generate_object_export_fixtures import reference
from generate_template_page_fixtures import entity,attributes

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_VARIANT_FIXTURES',ROOT/'rust/tests/fixtures/template-variants'))
FIXED=UUID('1d0c69dc-01d4-40b7-8725-d046484405bd')
def modules():
    ref=reference();pkg=ref.__package__
    return tuple(importlib.import_module(pkg+'.'+n) for n in ['autocad_dynamic_visibility','template_reverse_patch','template_dynamic_visibility_patch','dxf_template_pipeline_patch','dynamic_visibility_finalize_patch'])

def snapshot(path,wrappers):
    dv,reverse,dynamic,pipeline,finalize=modules()
    doc=ezdxf.readfile(path);report=doc.audit();assert not report.has_errors and not report.has_fixes,(report.errors,report.fixes)
    pairs,_,_=dv._read_pairs(path);records=dv._split_records(pairs);sections=dv._record_sections(records)
    # ezdxf binds linked owners while reading, before audit() can report repairs.
    # Inspect original tags as well, so those implicit corrections cannot hide
    # stale modelspace owners in a native output.
    raw_owners={dv._record_handle(r):dv._record_owner(r) for r in records if dv._record_handle(r)}
    result=dict(model=[],pairs={})
    for e in doc.modelspace():
        if e.dxftype()=='INSERT' and e.dxf.name in wrappers:
            result['model'].append(dict(wrapper=e.dxf.name,layer=e.dxf.layer,insert=list(e.dxf.insert)))
        else:result['model'].append(entity(e,{},doc))
    for name in wrappers:
        block=doc.blocks.get(name);refs=list(block.query('INSERT'));assert len(refs)==2
        def content(ref):
            layout=doc.blocks.get(ref.dxf.name);result=[]
            for e in layout:
                assert raw_owners[e.dxf.handle]==layout.block_record_handle,(e.dxftype(),'content owner')
                if e.dxftype() in ('INSERT','POLYLINE'):
                    children=e.attribs if e.dxftype()=='INSERT' else e.vertices
                    for child in children:
                        assert raw_owners[child.dxf.handle]==e.dxf.handle,(child.dxftype(),'parent owner')
                    if e.seqend:
                        assert raw_owners[e.seqend.dxf.handle]==e.dxf.handle,('SEQEND','parent owner')
                result.append(entity(e,{},doc))
            return result
        _index,br=dv._target_block_record(records,sections,name);root=finalize._extension_dictionary_handle(br)
        owned={root};changed=True
        while changed:
            changed=False
            for r,s in zip(records,sections):
                h=dv._record_handle(r)
                if s=='OBJECTS' and dv._record_owner(r) in owned and h not in owned:owned.add(h);changed=True
        metadata=[r for r,s in zip(records,sections) if s=='OBJECTS' and dv._record_handle(r) in owned]
        handles={dv._record_handle(r):f'object:{i}' for i,r in enumerate(metadata)}
        handles[dv._record_handle(br)]='wrapper';handles[refs[0].dxf.handle]='normal-ref';handles[refs[1].dxf.handle]='reverse-ref'
        def tags(r):
            out=[]
            for c,v in r:
                v=v.strip()
                if c==5 or c in dv._HANDLE_REFERENCE_CODES:
                    v=handles.get(v.upper(),'0' if v=='0' else 'shared:'+v.upper())
                elif c==1000 and v.startswith('{') and v.endswith('}'):
                    parsed=UUID(v);assert parsed.version==4;v='{UUID4}'
                out.append([c,v])
            return out
        xdata=[(c,v) for c,v in br if c>=1000]
        layers=[name.removesuffix('_VERSIE')+suffix for suffix in ['_NORMAAL','_REVERSE']]
        result['pairs'][name]=dict(
            block_xdata=tags(xdata),metadata=[tags(r) for r in metadata],
            normal=content(refs[0]),
            reverse=content(refs[1]),
            nested=[dict(name=e.dxf.name,layer=e.dxf.layer,invisible=e.dxf.invisible,insert=list(e.dxf.insert),xdata=tags([(t.code,str(t.value)) for t in e.get_xdata('AcDbBlockRepETag')])) for e in refs],
            layers_on=[doc.layers.get(n).is_on() for n in layers],
            layers=[attributes(doc.layers.get(n),('handle','owner')) for n in layers],
        )
    return result

def main():
    assert ezdxf.__version__=='1.4.4';ezdxf.options.write_fixed_meta_data_for_testing=True
    OUT.mkdir(parents=True,exist_ok=True)
    dv,reverse,dynamic,pipeline,finalize=modules()
    source,_,_=dv._read_pairs(ROOT/'assets/cadastral_template.dxf');src=dv._split_records(source);sec=dv._record_sections(src);donor=dv._discover_donor(src,sec)
    selected=[r for r in src if dv._record_handle(r) in donor.metadata_handles]
    doc=ezdxf.new('R2018');doc.layers.new('KABELS',dxfattribs={'color':3});doc.appids.new('CUSTOM')
    for app in ['AcDbBlockRepETag','AcDbDynamicBlockTrueName','AcDbDynamicBlockGUID']:doc.appids.new(app)
    for mode,color in [(reverse.NORMAL_MODE,5),(reverse.REVERSE_MODE,6)]:
        layer=doc.layers.new(reverse.variant_layer_name('PS 2/A?',3,mode),dxfattribs={'color':color,'lineweight':35});layer.off()
    for name in ['DONOR_A','DONOR_B']:
        b=doc.blocks.new(name);b.add_line((0,0),(1,1))
    b=doc.blocks.new('VISIBILITY_DONOR');states=[b.add_blockref(n,(0,0)) for n in ['DONOR_A','DONOR_B']]
    label=doc.blocks.new('LABEL');label.add_circle((0,0),.1);label.add_attdef('INFO',(0,0),height=.2)
    data=[];ms=doc.modelspace()
    image_defs=[doc.add_image_def(filename=n,size_in_pixel=(8,6),name='FIXTURE_'+str(i)) for i,n in enumerate(['image.png','reverse.png'])]
    for i,(name,index) in enumerate([('PS 2/A?',3),('PS 2/A?',1),('ß漢'*100,100)]):
        normal=[];back=[]
        normal.append(ms.add_line((i*10,1,2),(i*10+4,2,3),dxfattribs={'layer':'KABELS','color':256,'true_color':0xff55aa}))
        normal[-1].set_xdata('CUSTOM',[(1000,'Bewaard Ø 漢'),(1070,17)])
        normal[-1].new_extension_dict().add_xrecord('USER').tags.extend([(1,'Eigen data'),(90,42)])
        normal.append(ms.add_image(image_defs[0],insert=(i*10,3),size_in_units=(4,3)))
        ann=ms.add_blockref('LABEL',(i*10+1,8),dxfattribs={'layer':'KABELS'});ann.add_auto_attribs({'INFO':'Water Ø 110'})
        normal.append(ann);leader=ms.add_leader([(i*10,5),(i*10+1,8)],dxfattribs={'layer':'KABELS'});leader.dxf.annotation_handle=ann.dxf.handle;normal.append(leader)
        back.append(ms.add_polyline3d([(i*10,0,1),(i*10+2,1,3),(i*10+3,0,2)],dxfattribs={'layer':'KABELS'}))
        back.append(ms.add_image(image_defs[1],insert=(i*10,3),size_in_units=(4,3)))
        data.append(dict(label=name,slot_index=index,normal=[int(e.dxf.handle,16) for e in normal],reverse=[int(e.dxf.handle,16) for e in back],selector=[i*10+5.25,9.5,2.]))
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    mapping={donor.block_record_handle:b.block_record_handle,donor.state_entity_handles[0]:states[0].dxf.handle,donor.state_entity_handles[1]:states[1].dxf.handle}
    mapping.update({dv._record_handle(r):doc.entitydb.next_handle() for r in selected})
    path=OUT/'input.dxf';doc.saveas(path)
    pairs,nl,bom=dv._read_pairs(path);records=dv._split_records(pairs);sections=dv._record_sections(records)
    index,record=dv._target_block_record(records,sections,'VISIBILITY_DONOR')
    with patch.object(dv,'uuid4',return_value=FIXED):records[index]=dv._inject_dynamic_block_record_data(record,xdictionary_handle=mapping[donor.xdictionary_handle],block_name='VISIBILITY_DONOR',block_rep_e_tag=donor.block_rep_e_tag)
    metadata=[]
    for r in selected:
        metadata.append([(c,mapping.get(v.strip().upper(),v) if c==5 or c in dv._HANDLE_REFERENCE_CODES else v) for c,v in r])
    at=dv._objects_end_index(records,sections);records[at:at]=metadata
    # Carry the actual AutoCAD class definitions; no opaque type is invented.
    kinds={dv._record_type(r) for r in selected};classes=[r for r,s in zip(src,sec) if s=='CLASSES' and dv._first_value(r,1) in kinds]
    sections=dv._record_sections(records);at=next(i for i,(r,s) in enumerate(zip(records,sections)) if s=='CLASSES' and dv._record_type(r)=='ENDSEC');records[at:at]=classes
    dv._write_pairs(path,dv._flatten_records(records),newline=nl,had_bom=bom)
    Image.new('RGBA',(8,6),(200,40,80,210)).save(OUT/'image.png');Image.new('RGBA',(8,6),(20,110,210,180)).save(OUT/'reverse.png')
    cases=[]
    for name,indices,selector in [('one',[0],'original'),('all-shuffled',[2,0,1],'original'),('same-label',[1,0],'original'),('without-selector',[0],None),('coordinate-precision',[0],[123456.78901234,-.0000123456,55.012345678901])]:
        d=ezdxf.readfile(path);specs=[]
        for i in indices:
            spec=dict(data[i]);spec['selector']=spec['selector'] if selector=='original' else selector;specs.append(spec)
            for key,mode in [('normal',reverse.NORMAL_MODE),('reverse',reverse.REVERSE_MODE)]:
                reverse._move_entities_to_variant_container(d,d.modelspace(),[d.entitydb[f'{h:X}'] for h in spec[key]],label=spec['label'],slot_index=spec['slot_index'],mode=mode)
        wrappers=dynamic._wrap_variant_pairs_as_static_blocks(d)
        for layer in d.layers:
            if dynamic._variant_pair_key(layer.dxf.name):layer.on()
        target=OUT/'working.dxf';d.saveas(target)
        raw,nl,bom=dv._read_pairs(target);records=dv._split_records(raw);sections=dv._record_sections(records)
        locations={dynamic.dynamic_block_name(s['label'],s['slot_index']):s['selector'] for s in specs if s['selector'] is not None}
        with patch.object(dv,'uuid4',return_value=FIXED):promoted,next_id=pipeline._promote_records(records,sections,wrappers,next_handle=max(dv._max_handle_value(raw)+1,dv._header_handseed(raw)))
        assert promoted==len(specs);pipeline._set_handseed_in_records(records,next_id);assert pipeline._finalize_records(records,dv._record_sections(records),wrappers,locations)==len(specs)
        dv._write_pairs(target,dv._flatten_records(records),newline=nl,had_bom=bom)
        cases.append(dict(name=name,pairs=specs,wrappers=wrappers,expected=snapshot(target,wrappers)))
    (OUT/'working.dxf').unlink()
    labels=['','._.','PS 5/A?','Straße ſ K İ 漢','a\u001cb\u0085c','x'*119+'__Y','漢'*125]
    names=[dict(label=s,index=i,expected=[reverse.variant_layer_name(s,i,reverse.NORMAL_MODE),reverse.variant_layer_name(s,i,reverse.REVERSE_MODE),dynamic.dynamic_block_name(s,i)]) for s in labels for i in [-5,0,1,9,99,100,5000]]
    (OUT/'reference.json').write_text(json.dumps(dict(cases=cases,names=names),ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'{len(cases)} original dynamic container cases / {len(names)} name cases generated')
if __name__=='__main__':main()
