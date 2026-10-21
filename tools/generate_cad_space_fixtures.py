"""Modelspace membership from the actual original dxf.py and ezdxf loader.

AST-only original functions avoid application initialization and preferences.
Fixtures deliberately contradict owner/67/410 and contain ignored heavy pages.
"""
from collections import namedtuple
from io import StringIO,BytesIO
from pathlib import Path
from types import SimpleNamespace
import json
import os

import ezdxf
from ezdxf import colors
from ezdxf.lldxf.tagwriter import BinaryTagWriter
from generate_marxact_alignment_fixtures import functions

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_CAD_SPACE_FIXTURES',ROOT/'rust/tests/fixtures/cad-space'))
VERSIONS=['R12','R2000','R2007','R2018']


def original():
    names=['load_dxf','_polyline_from_entity','_points_bounds','_resolve_entity_color','_format_xdata','_format_insert_attributes','_entity_metadata']
    return functions('dxf.py',names,dict(Path=Path,ezdxf=ezdxf,colors=colors,DxfError=RuntimeError,
        Bounds=namedtuple('Bounds','min_x min_y max_x max_y'),CableFeature=lambda **kw:SimpleNamespace(**kw),DxfOverlay=lambda **kw:SimpleNamespace(**kw)))


def build(version,shift=0):
    doc=ezdxf.new(version);doc.styles.get('Standard').dxf.font='*monospace'
    model=doc.modelspace();paper=doc.layout();other=doc.layouts.new('Other Sheet')
    box=doc.blocks.new('Box');line=box.add_line((0,0),(2,3));line.dxf.paperspace=1  # block context wins
    marker={}
    def line(layout,name,a,b):
        e=layout.add_line(a,b);marker[name]=e.dxf.handle;return e
    x=168000+shift;y=401000-shift
    line(model,'base',(x,y),(x+100,y+100))
    e=line(model,'model-owner-paper-flag',(x+5,y+5),(x+10,y+10));e.dxf.paperspace=1
    e=line(paper,'paper-owner-model-flag',(1e6,1e6),(1e6+10,1e6+10));e.dxf.paperspace=0
    e=line(model,'unknown-owner-paper',(1e6,-1e6),(1e6+10,-1e6));e.dxf.owner='ABCDEF';e.dxf.paperspace=1
    e=line(paper,'unknown-owner-model',(x+40,y+40),(x+45,y+45));e.dxf.owner='ABCDEE';e.dxf.paperspace=0
    e=line(model,'reactor-shadow',(x+20,y+20),(x+25,y+25))
    modern=doc.dxfversion>'AC1009'
    marker['model-block-header']=model.block_record.block.dxf.handle
    if modern:
        doc.block_records.get('*Model_Space').set_app_data('NATIVE_RECORD',[(2,'*Paper_Space'),(5,'FFFF'),(100,'AcDbBlockTableRecord')])
        model.block_record.block.set_app_data('NATIVE_BLOCK',[(2,'*Paper_Space'),(67,1)])
    if modern:e.set_app_data('NATIVE_SPACE',[(330,paper.block_record_handle),(67,1),(100,'AcDbEntity'),(66,1)])
    pl=model.add_polyline2d([(x+10,y+10),(x+15,y+10),(x+15,y+15)],close=True);marker['model-poly']=pl.dxf.handle
    for v in pl.vertices:v.dxf.paperspace=1
    pl=paper.add_polyline2d([(1e6,1e6),(1e6+5,1e6),(1e6+5,1e6+5)],close=True);marker['paper-poly']=pl.dxf.handle
    for v in pl.vertices:v.dxf.paperspace=0
    ins=model.add_blockref('Box',(x+30,y+30));marker['model-insert']=ins.dxf.handle
    a=ins.add_attrib('CODE','tag',(x+31,y+31));a.dxf.paperspace=1
    ins=paper.add_blockref('Box',(1e6,1e6));marker['paper-insert']=ins.dxf.handle
    ins.add_attrib('CODE','page-only',(1e6,1e6)).dxf.paperspace=0
    ins=paper.add_blockref('Box',(1e6,1e6));marker['paper-no-attrs']=ins.dxf.handle
    if modern:ins.set_app_data('NATIVE_FOLLOW',[(66,1),(67,0),(330,model.block_record_handle)])
    # These must never be tessellated or reported as missing map features.
    paper.add_circle((1e6,1e6),1e100);other.add_circle((-1e6,1e6),1e100)
    if modern:paper.add_mtext('ignored page text');other.add_mtext('ignored inactive page')
    else:paper.add_text('ignored page text');other.add_text('ignored inactive page')
    marker['moved']=line(model,'moved-model-content',(x+50,y+50),(x+55,y+55)).dxf.handle
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    out=StringIO();doc.write(out);return doc,marker,out.getvalue()


def records(text):
    lines=text.splitlines();tags=[(int(lines[i]),lines[i+1]) for i in range(0,len(lines),2)];out=[]
    for tag in tags:
        if tag[0]==0:out.append([])
        out[-1].append(tag)
    return out


def handle(record):return next((v for c,v in record if c==5),'')
def value(record,code):return next((v for c,v in record if c==code),'')


def mutate(doc,markers,text,mode):
    data=records(text);modern=doc.dxfversion>'AC1009'
    # Layout name alone must not move an otherwise model-owned entity.
    for r in data:
        if modern and handle(r)==markers['base']:
            at=next((i for i,t in enumerate(r) if t==(100,'AcDbLine')),len(r));r.insert(at,(410,'Other Sheet'))
        if not modern and handle(r)==markers['paper-no-attrs']:
            r[2:2]=[(102,'{NATIVE_SPACE'),(100,'AcDbEntity'),(67,0),(66,1),(102,'}')]
        if not modern and handle(r)==markers['reactor-shadow']:
            r[2:2]=[(102,'{NATIVE_SPACE'),(100,'AcDbEntity'),(67,1),(66,1),(102,'}')]
    if mode=='moved-model':
        moving=next(r for r in data if handle(r)==markers['moved']);data.remove(moving)
        inside=False
        for i,r in enumerate(data):
            if r[0]==(0,'BLOCK') and handle(r)==markers['model-block-header']:inside=True
            elif inside and r[0]==(0,'ENDBLK'):data.insert(i,moving);break
        else:raise AssertionError('missing model BLOCK')
    elif mode=='late-tables':
        start=next(i for i,r in enumerate(data) if r[0]==(0,'SECTION') and value(r,2)=='TABLES')
        end=next(i+1 for i in range(start+1,len(data)) if data[i][0]==(0,'ENDSEC'))
        table=data[start:end];del data[start:end];at=next(i for i,r in enumerate(data) if r[0]==(0,'EOF'));data[at:at]=table
    return [t for r in data for t in r]


def write(path,tags,version,binary):
    if binary:
        out=BytesIO();out.write(b'AutoCAD Binary DXF\r\n\x1a\0');writer=BinaryTagWriter(out,dxfversion=version)
        for code,val in tags:
            if code==999:continue
            if code in range(310,320) or code==1004:val=bytes.fromhex(val)
            writer.write_tag2(code,val)
        path.write_bytes(out.getvalue())
    else:path.write_text(''.join(f'{code}\n{val}\n' for code,val in tags),encoding='utf-8')


def evaluate(path,ref=None):
    ref=ref or original();overlay=ref.load_dxf(path);doc=ezdxf.readfile(path)
    return dict(selected=[dict(handle=e.dxf.handle,kind=e.dxftype()) for e in doc.modelspace()],
        paths=[dict(handle=f.metadata['Handle'],kind=f.metadata['Type'],points=f.points) for f in overlay.features],
        bounds=[min(f.bounds.min_x for f in overlay.features),min(f.bounds.min_y for f in overlay.features),
                max(f.bounds.max_x for f in overlay.features),max(f.bounds.max_y for f in overlay.features)])


def main():
    assert ezdxf.__version__=='1.4.4';OUT.mkdir(parents=True,exist_ok=True);ezdxf.options.write_fixed_meta_data_for_testing=True
    cases=[];ref=original()
    for version in VERSIONS:
        doc,markers,text=build(version)
        for mode in ['normal','moved-model','late-tables']:
            tags=mutate(doc,markers,text,mode)
            for binary in [False,True]:
                name=f'{version}-{mode}-'+('binary' if binary else 'ascii')+'.dxf';path=OUT/name
                write(path,tags,doc.dxfversion,binary);cases.append(dict(file=name,**evaluate(path,ref)))
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original modelspace/layout selections / {sum(len(c["selected"]) for c in cases)} selected source entities')


if __name__=='__main__':main()
