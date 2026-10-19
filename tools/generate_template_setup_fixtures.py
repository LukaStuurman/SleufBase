"""Composed original template preparation: pages through title fields/viewports."""
from pathlib import Path
from types import SimpleNamespace
from datetime import datetime
import os,importlib,io,json
import ezdxf
from ezdxf.entities import factory
from ezdxf.lldxf.extendedtags import ExtendedTags
from generate_object_export_fixtures import reference
from generate_template_legend_fixtures import legend_snapshot
from generate_template_preparation_fixtures import snapshot as style_snapshot
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_TEMPLATE_SETUP_FIXTURES',ROOT/'rust/tests/fixtures/template-setup'))

def snapshot(doc,exporter):
    result=legend_snapshot(doc,exporter);result['symbols']=style_snapshot(doc,False);result['units']=doc.units
    variables=doc.rootdict.get('ACAD_IMAGE_VARS');result['raster_variables']=None if variables is None else {k:getattr(variables.dxf,k) for k in ('frame','quality','units')}
    scales=doc.rootdict.get('ACAD_SCALELIST');result['scale_keys']=[] if scales is None else list(scales.keys())
    result['scale_objects']={e.dxf.handle:[[t.code,t.value] for t in e.xtags.subclasses[1]] for e in doc.entitydb.values() if e.dxftype()=='SCALE'}
    return result

def main():
    assert ezdxf.__version__=='1.4.4';ezdxf.options.write_fixed_meta_data_for_testing=True
    OUT.mkdir(parents=True,exist_ok=True);ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    metadata=importlib.import_module(ref.__package__+'.template_metadata');Bounds=importlib.import_module(ref.__package__+'.models').Bounds
    sources=[]
    for i in range(3):
        doc=ezdxf.readfile(ROOT/'rust/tests/fixtures/template-legends/affine-2.dxf');doc.units=4
        if i==1:doc.set_raster_variables(frame=1,quality=0,units='mm')
        if i==2:
            # Exercise header insertion as well as replacement.
            doc.header.__delitem__('$INSUNITS')
        scale_dict=doc.rootdict.get('ACAD_SCALELIST')
        if scale_dict is None:scale_dict=doc.rootdict.add_new_dict('ACAD_SCALELIST')
        def scale(name,partial=False):
            h=doc.entitydb.next_handle();owner=scale_dict.dxf.handle
            tail='' if partial else '140\n7\n141\n9\n290\n1\n'
            text=f'0\nSCALE\n5\n{h}\n330\n{owner}\n100\nAcDbScale\n70\n0\n300\n{name}\n{tail}'
            e=factory.load(ExtendedTags.from_text(text),doc);doc.entitydb.add(e);doc.objects.add_object(e);return e
        for key in list(scale_dict.keys()):scale_dict.discard(key)
        a=scale('old A0');b=scale('alias');scale_dict['A0']=a;scale_dict['A1']=b;scale_dict['A2']=b
        scale_dict['A3']=scale('partial',True);scale_dict['A8']=scale('old A8');scale_dict['A9']=scale('old A9');scale_dict['DROP']=scale('untouched removed entry')
        doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
        p=OUT/f'setup-{i}.dxf';doc.saveas(p);p.write_bytes(p.read_bytes().replace(b'\r\n',b'\n'));sources.append((f'setup-{i}',p))
    sources.append(('shipped',ROOT/'assets/cadastral_template.dxf'))
    cases=[]
    def slots(n):
        return [None if j%5==1 else dict(min_x=120000+j*10,min_y=450000-j,max_x=120020+j*10,max_y=450020-j) for j in range(n)]
    for name,path in sources:
        doc=ezdxf.readfile(path);steps=[]
        counts=[0,9,17] if name!='shipped' else [40,49]
        for index,n in enumerate(counts):
            layers=None if index%2==0 else [dict(name='Water',color=3),dict(name='Data',color=5),dict(name='water',color=1)]
            step=dict(slots=slots(n),project=' Project Ø 漢 '+str(index)+' ',drawn_by=' User Ø ',label_gap=18.,profile_layers=layers)
            exporter._extend_template_for_slot_count(doc,n);exporter._ensure_template_image_layer(doc);exporter._setup_text_styles(doc)
            if layers is not None:exporter._ensure_template_profile_layers(doc,[SimpleNamespace(layer_name=l['name'],color=l['color']) for l in layers])
            names=exporter._used_template_layout_names(n);exporter._ensure_template_layout_legends(doc,names)
            doc.units=6;doc.set_raster_variables(frame=0,quality=1,units='m')
            exporter._update_template_scale_list(doc)
            metadata.update_title_blocks(doc,names,project=step['project'],drawn_by=step['drawn_by'],export_date=datetime(2026,10,19))
            exporter._update_template_layout_page_numbers(doc,names)
            layers_for_view=[None if b is None else SimpleNamespace(bounds=Bounds(**b)) for b in step['slots']]
            exporter._configure_template_wireframe_viewports(doc,layers_for_view,names,step['label_gap'])
            stream=io.StringIO();doc.write(stream);stream.seek(0);doc=ezdxf.read(stream)
            step['expected']=snapshot(doc,exporter);steps.append(step)
        cases.append(dict(name=name,path='assets/cadastral_template.dxf' if name=='shipped' else 'rust/tests/fixtures/template-setup/'+path.name,steps=steps))
    (OUT/'reference.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(cases),'complete preparation sequences;',sum(len(c['steps']) for c in cases),'states')
if __name__=='__main__':main()
