"""Supported original paper-layout underlay copies, shared definitions/reactors."""
from pathlib import Path
import importlib,json,os
import ezdxf
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_UNDERLAY_FIXTURES',ROOT/'rust/tests/fixtures/template-underlays'))
def snapshot(doc):
    result=[]
    for layout in doc.layouts:
        if layout.name=='Model':continue
        entities=[]
        for e in layout:
            if e.dxftype() not in ('PDFUNDERLAY','DGNUNDERLAY','DWFUNDERLAY'):continue
            attrs={k:(list(v) if hasattr(v,'x') else v) for k,v in e.dxf.all_existing_dxf_attribs().items() if k not in ('handle','owner','underlay_def_handle')}
            definition=doc.entitydb[e.dxf.underlay_def_handle]
            entities.append(dict(kind=e.dxftype(),attrs=attrs,boundary=[list(p) for p in e.boundary_path],definition=dict(kind=definition.dxftype(),filename=definition.dxf.filename,name=definition.dxf.name)))
        result.append(dict(layout=layout.name,entities=entities))
    return result
def main():
    ref=reference();exp=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    ezdxf.options.write_fixed_meta_data_for_testing=True;OUT.mkdir(parents=True,exist_ok=True)
    doc=ezdxf.readfile(ROOT/'rust/tests/fixtures/template-pages/one-page.dxf');doc.header['$FINGERPRINTGUID']='{FB571C77-7225-4EA6-AB6C-4EA3B93AA872}';doc.header['$VERSIONGUID']='{FB571C77-7225-4EA6-AB6C-4EA3B93AA873}'
    layout=doc.layouts.get('bLaD1')
    for i,fmt in enumerate(['pdf','dgn','dwf']):
        definition=doc.add_underlay_def('document.'+fmt,fmt,str(i+1))
        for j in range(2):
            e=layout.add_underlay(definition,(10+i*13,20+j*7,3),scale=1.25+j,rotation=37.5+i*45,dxfattribs={'layer':'KADER','contrast':40+i*7,'fade':10+i*3,'flags':1+2*j})
            e.dxf.scale_y=-.75-j;e.dxf.extrusion=(.3,.4,1.)
            e.set_boundary_path([(0,0),(10,0),(10,5+j),(0,5+j)])
            e.set_xdata('PAGE_TEST',[(1000,'retain underlay metadata'),(1011,(1,2,3))])
            e.new_extension_dict().add_xrecord('UNDERLAY').reset([(1,'independent owned data'),(90,17+i)])
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/'source.dxf');(OUT/'source.dxf').write_bytes((OUT/'source.dxf').read_bytes().replace(b'\r\n',b'\n'))
    steps=[]
    for required in (9,17,33):
        exp._extend_template_for_slot_count(doc,required);steps.append(dict(required=required,expected=snapshot(doc)))
        for definition in (e for e in doc.objects if e.dxftype() in ('PDFDEFINITION','DGNDEFINITION','DWFDEFINITION')):
            for layout in doc.layouts:
                for e in layout:
                    if e.dxftype().endswith('UNDERLAY') and e.dxf.underlay_def_handle==definition.dxf.handle:assert e.dxf.handle in definition.get_reactors()
    (OUT/'reference.json').write_text(json.dumps(steps,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('3 original extension states;',sum(len(l['entities']) for s in steps for l in s['expected']),'underlay instances')
if __name__=='__main__':main()
