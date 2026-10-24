"""Independent real-DXF/logo-pixel/contact readback against original helpers."""
from pathlib import Path
import importlib,json,sys,tempfile
import ezdxf
from PIL import Image
from generate_template_branding_fixtures import OUT
from generate_template_image_fixtures import signature
from verify_native_template_images import compare
from generate_object_export_fixtures import reference
from generate_template_branding_fixtures import ROOT
def main():
    output=Path(sys.argv[1]);ref=json.loads((OUT/'reference.json').read_text(encoding='utf-8'));doc=ezdxf.readfile(output/'branding.dxf')
    actual=[];sources=set()
    for name in ref['layouts']:
        if name not in doc.layouts:continue
        for e in doc.layouts.get(name).query('IMAGE'):
            assert e.dxf.owner==doc.layouts.get(name).layout_key
            d=doc.entitydb[e.dxf.image_def_handle];reactor=doc.entitydb[e.dxf.image_def_reactor_handle]
            assert reactor.dxf.owner==e.dxf.handle and reactor.dxf.image_handle==e.dxf.handle
            assert reactor.dxf.handle in d.get_reactors()
            a=signature(doc,e);a.pop('filename');actual.append(dict(layout=name,image=a));sources.add(Path(d.dxf.filename))
    expected=ref['expected']
    for e in expected:e['image'].pop('filename')
    compare(actual,expected,'branding')
    assert len(sources)==2 # sources normalized once, referenced by all sheets
    for source,prepared in [('tech.png','tech.png'),('client.jpg','client.png')]:
        with Image.open(OUT/source) as a,Image.open(output/prepared) as b:
            assert a.size==b.size and a.convert('RGBA').tobytes()==b.convert('RGBA').tobytes(),source
    # entitydb insertion order changes on DXF serialization (modelspace is
    # read before block entities). Compare original text entities by handle.
    module=importlib.import_module(reference().__package__+'.cadastral_export');exp=module.CadastralDxfExporter(wfs_client=None)
    contact_doc=ezdxf.readfile(OUT/'source.dxf');exp._restore_template_layout_contact_text(contact_doc)
    text_by_handle=lambda d:{e.dxf.handle:e.dxf.text for e in d.entitydb.values() if e.dxftype()=='TEXT'}
    assert text_by_handle(doc)==text_by_handle(contact_doc)
    assert doc.modelspace().query('MTEXT')[0].text=='5236 PV'
    audit=doc.audit();assert not audit.has_errors,audit.errors
    # Independently exercise the full production pipeline, which inserts the
    # same branding into its single raster batch for two used paper layouts.
    expected_doc=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf');exp._ensure_template_image_layer(expected_doc)
    exp._ensure_template_layout_legends(expected_doc,['Blad1','Blad2'])
    actual_doc=ezdxf.readfile(output/'complete.dxf');expected_logos=[];actual_logos=[]
    with tempfile.TemporaryDirectory() as tmp:
        exp._restore_template_layout_logos(expected_doc,Path(tmp),['Blad1','Blad2'],OUT/'tech.png',OUT/'client.jpg')
        for name in ['Blad1','Blad2']:
            for target,items in [(expected_doc,expected_logos),(actual_doc,actual_logos)]:
                for e in target.layouts.get(name).query('IMAGE'):
                    a=signature(target,e)
                    if not any(key in a['name'] for key in ['TECHBASE_LOGO','OPDRACHTGEVER_LOGO']):continue
                    a.pop('filename');items.append(dict(layout=name,image=a))
        compare(actual_logos,expected_logos,'complete pipeline logos');assert len(actual_logos)==4
    assert not any(e.dxftype()=='TEXT' and e.dxf.text in ['De Diepteweg 16','5236 PV'] for e in actual_doc.entitydb.values())
    for name in ['Blad1','Blad2']:
        insert=exp._find_template_layout_insert(actual_doc.layouts.get(name));values={a.dxf.tag:a.dxf.text for a in insert.attribs}
        assert values['GEM-PROJECT']=="Logo's Ø" and values['DATUM1']=='23-10-2026'
        assert values['TEKENAAR1']=='Technicus' and values['BLADEN']=='2'
    audit=actual_doc.audit();assert not audit.has_errors,audit.errors
    print('14 actual native paper-layout logos across helper/full-production exports, all PNG/JPEG pixels, contact/title text and image/reactor ownership match original')
if __name__=='__main__':main()
