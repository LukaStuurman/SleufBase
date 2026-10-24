"""Original contact/logo helpers on synthetic documents; no app/settings."""
from pathlib import Path
from dataclasses import asdict
import importlib,json,os,tempfile
import ezdxf
from PIL import Image
from generate_object_export_fixtures import reference
from generate_template_image_fixtures import signature
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_BRANDING_FIXTURES',ROOT/'rust/tests/fixtures/template-branding'))

def main():
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export');exp=module.CadastralDxfExporter(wfs_client=None)
    OUT.mkdir(parents=True,exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    doc=ezdxf.new('R2018');doc.header['$FINGERPRINTGUID']='{FB571C77-7225-4EA6-AB6C-4EA3B93AA862}';doc.header['$VERSIONGUID']='{FB571C77-7225-4EA6-AB6C-4EA3B93AA863}'
    title=doc.blocks.new('TITLE');title.add_text('De Diepteweg 16');title.add_text('5236 PV');title.add_text("'s-Hertogenbosch")
    doc.modelspace().add_text('De Diepteweg 16');doc.modelspace().add_text('5236 PV');doc.modelspace().add_text('keep De Diepteweg 16');doc.modelspace().add_mtext('5236 PV')
    layouts=[]
    for i,(sx,sy,angle) in enumerate([(1,1,0),(-2,.75,0),(.5,-1.5,37.5),(0,0,0),(1,1,180)],1):
        name='bLaD'+str(i);layouts.append(name);layout=doc.layouts.new(name)
        layout.add_blockref('TITLE',(i*7.2,i*-2.3),dxfattribs={'xscale':sx,'yscale':sy,'rotation':angle})
        layout.add_blockref('TITLE',(999,999)) # Original chooses the first INSERT.
    doc.layouts.new('BladZonderTitel');layouts+=['BladZonderTitel','VerdwenenBlad']
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/'source.dxf');(OUT/'source.dxf').write_bytes((OUT/'source.dxf').read_bytes().replace(b'\r\n',b'\n'))
    with tempfile.TemporaryDirectory() as tmp:
        tmp=Path(tmp);tech=Image.new('RGBA',(37,19));tech.putdata([(x*7%256,y*11%256,(x+y)*13%256,(x*3+y*5)%256) for y in range(19) for x in range(37)]);tech.save(OUT/'tech.png')
        client=Image.new('RGB',(21,39),(200,117,31));client.save(OUT/'client.jpg',quality=93)
        # Actual original preparation includes PNG conversion and layout ownership.
        exp._ensure_template_image_layer(doc);exp._restore_template_layout_logos(doc,tmp,layouts,OUT/'tech.png',OUT/'client.jpg')
        exp._restore_template_layout_contact_text(doc)
        expected=[]
        for name in layouts:
            if name not in doc.layouts:continue
            expected.extend(dict(layout=name,image=signature(doc,e)) for e in doc.layouts.get(name).query('IMAGE'))
        texts=[e.dxf.text for e in doc.entitydb.values() if e.dxftype()=='TEXT']
        # Repeated helper must not modify already repaired text.
        exp._restore_template_layout_contact_text(doc);assert texts==[e.dxf.text for e in doc.entitydb.values() if e.dxftype()=='TEXT']
    (OUT/'reference.json').write_text(json.dumps(dict(layouts=layouts,expected=expected,texts=texts,contact_changes=4),ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(len(expected),'original layout logos;',len(texts),'contact/untouched texts')
if __name__=='__main__':main()
