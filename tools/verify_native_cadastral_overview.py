"""Read real overview DXFs against the original full writer and TIFF copies."""
from pathlib import Path
from types import SimpleNamespace
import importlib,json,sys,tempfile
import ezdxf
from PIL import Image
from verify_native_profile_leaders import full_signature
from generate_template_image_fixtures import signature as image_signature
from verify_native_template_images import compare
from generate_object_export_fixtures import reference
ROOT=Path(__file__).resolve().parents[1]
FIXTURE=ROOT/'rust/tests/fixtures/cadastral-overview'

def ring(points):
    points=[list(p) for p in points]
    if len(points)>1 and points[0]==points[-1]:points.pop()
    if not points:return points
    variants=[]
    for values in [points,list(reversed(points))]:
        start=min(range(len(values)),key=lambda i:values[i]);variants.append(values[start:]+values[:start])
    return min(variants)

def signature(doc,entity):
    if entity.dxftype()=='IMAGE':
        result=image_signature(doc,entity);result.pop('filename')
        definition=doc.entitydb[entity.dxf.image_def_handle];reactor=doc.entitydb[entity.dxf.image_def_reactor_handle]
        assert reactor.dxf.owner==entity.dxf.handle and reactor.dxf.image_handle==entity.dxf.handle
        assert reactor.dxf.handle in definition.get_reactors()
        path=Path(definition.dxf.filename)
        return dict(kind='IMAGE',image=result,source=path.name)
    result=full_signature(entity);d=entity.dxf
    if entity.dxftype()=='LWPOLYLINE':
        result.update(const_width=d.const_width,elevation=d.elevation,extrusion=list(d.extrusion),thickness=d.thickness)
        if d.layer=='PROEFSLEUVEN_OMTREK':result['vertices']=ring(result['vertices'])
        elif d.layer=='PROEFSLEUVEN_HARTLIJN':
            # An undirected centerline's first SVD eigenvector may have either
            # sign. Compare every endpoint, width and paint, irrespective of sign.
            result['vertices']=min(result['vertices'],list(reversed(result['vertices'])))
    elif entity.dxftype()=='HATCH':
        result.update(pattern=d.pattern_name,solid=d.solid_fill,associative=d.associative,style=d.hatch_style,pattern_type=d.pattern_type,elevation=list(d.elevation),extrusion=list(d.extrusion),paths=[dict(flags=p.path_type_flags,closed=p.is_closed,vertices=ring(p.vertices)) for p in entity.paths])
    return result

def template_overview(case,output,exporter,models):
    """Call the original complete template overview writer on its real source.

    Read back its actual serialized entities, then compare the RD overview in
    the production paired DXF. The source's existing records are excluded by
    identity; no native output is used to construct expected geometry.
    """
    name=case['name'];request=case['request'];template=ROOT/'assets/cadastral_template.dxf'
    original=ezdxf.readfile(template);baseline={e.dxf.handle for e in original.modelspace()}
    settings=request['settings'];exporter._setup_layers(original,tuple(settings['centerline_color']),tuple(settings['label_color']));exporter._setup_text_styles(original)
    layers=[models.GeoTiffLayer(FIXTURE/Path(s['path']).name,Image.open(FIXTURE/Path(s['path']).name),models.GeoTransform(**s['transform']),models.Bounds(**s['bounds']),28992,metadata=s['metadata']) for s in request['layers']]
    combined=exporter._combined_bounds(layers);bounds=combined.padded(exporter._overview_padding(combined))
    try:
        with tempfile.TemporaryDirectory(prefix='SleufBase-original-template-overview-') as temp:
            path=Path(temp)/'original.dxf'
            exporter._populate_overview_modelspace(original,original.modelspace(),path,layers,[SimpleNamespace(**g) for g in request['linework']],[SimpleNamespace(**g) for g in request['labels']],bounds,settings['trench_mode'],settings['include_tiff_images'],settings['label_gap'],tuple(settings['centerline_color']),tuple(settings['label_color']))
            original.saveas(path);written=ezdxf.readfile(path)
            wanted_entities=[e for e in written.modelspace() if e.dxf.handle not in baseline]
            wanted=[signature(written,e) for e in wanted_entities]
        actual=ezdxf.readfile(output/('template-'+name+'.dxf'))
        wanted_layers={e.dxf.layer for e in wanted_entities}
        found=[e for e in actual.modelspace() if e.dxf.handle not in baseline and e.dxf.layer in wanted_layers]
        compare([signature(actual,e) for e in found],wanted,name+' original complete written template overview')
        for layer_name in ['BGT_VECTOR_OMTREK','BGT_WEGDEEL','KAD_PERCEEL','KAD_GRENS','KAD_BEBOUWING','KAD_STRAATNAAM','KAD_HUISNUMMER','PROEFSLEUVEN_VLAK','PROEFSLEUVEN_OMTREK','PROEFSLEUVEN_HARTLIJN','PROEFSLEUVEN_TIFF','PROEFSLEUVEN_LABEL']:
            expected_layer=written.layers.get(layer_name);actual_layer=actual.layers.get(layer_name)
            for key in ['color','true_color','lineweight','linetype','flags','plot']:
                compare(getattr(actual_layer.dxf,key,None),getattr(expected_layer.dxf,key,None),name+'/template-layer/'+layer_name+'/'+key)
        for style_name in ['PROEFSLEUVEN_LIBMONO','KADASTRALE_LABELS']:
            expected_style=written.styles.get(style_name);actual_style=actual.styles.get(style_name)
            # Compare font inputs and every declared metric from either style.
            # Explicit defaults and omitted optional default-valued tags have
            # identical DXF meaning; handles/owners identify separate documents.
            keys=(set(actual_style.dxf.all_existing_dxf_attribs())|set(expected_style.dxf.all_existing_dxf_attribs()))-{'handle','owner'}
            attributes=lambda s:{k:getattr(s.dxf,k,None) for k in keys}
            compare(attributes(actual_style),attributes(expected_style),name+'/template-style/'+style_name)
        image_entities=[e for e in found if e.dxftype()=='IMAGE']
        assert len(image_entities)==(len(layers) if settings['include_tiff_images'] else 0)
        for index,e in enumerate(image_entities):
            path=Path(actual.entitydb[e.dxf.image_def_handle].dxf.filename)
            assert path.is_absolute() and path.is_file()
            assert path.read_bytes()==(FIXTURE/Path(request['layers'][index]['path']).name).read_bytes()
        # Overview TIFFs/geometry stay outside both dynamic slot states.
        for block in actual.blocks:
            if block.name.endswith(('_NORMAAL_CONTENT','_REVERSE_CONTENT')):
                assert not any(e.dxf.layer in wanted_layers for e in block),(name,'overview captured by a profile state',block.name)
        source=ezdxf.readfile(template);baseline_audit=source.audit();report=actual.audit()
        assert not baseline_audit.has_errors and not report.has_errors
        assert [(f.code,f.message) for f in report.fixes]==[(f.code,f.message) for f in baseline_audit.fixes],(name,'new template audit repairs',report.fixes)
        return len(wanted),len(image_entities)
    finally:
        for layer in layers:layer.image.close()

def main():
    output=Path(sys.argv[1]);entities=images=template_entities=template_images=0
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export');models=importlib.import_module(ref.__package__+'.models');exporter=module.CadastralDxfExporter(None)
    for case in json.loads((FIXTURE/'reference.json').read_text(encoding='utf8')):
        name=case['name'];original=ezdxf.readfile(FIXTURE/(name+'.dxf'));actual=ezdxf.readfile(output/(name+'.dxf'))
        compare([signature(actual,e) for e in actual.modelspace()],[signature(original,e) for e in original.modelspace()],name+' complete actual entities')
        compare(actual.header['$INSUNITS'],original.header['$INSUNITS'],name+'$INSUNITS')
        # Original saveas silently overwrites its explicitly assigned overview
        # extents with the empty layout's +/-1e20 sentinel. Preserve the intended
        # finite bounds, checked through the original bounds/padding helpers.
        combined=exporter._combined_bounds([SimpleNamespace(bounds=models.Bounds(**s['bounds'])) for s in case['request']['layers']]);bounds=combined.padded(exporter._overview_padding(combined))
        for key,expected in [('$EXTMIN',[bounds.min_x,bounds.min_y,0.]),('$EXTMAX',[bounds.max_x,bounds.max_y,0.])]:
            assert abs(original.header[key][0])==1e20
            compare(list(actual.header[key]),expected,name+key+' original intended bounds')
        for layer in original.layers:
            found=actual.layers.get(layer.dxf.name)
            for key in ['color','true_color','lineweight','linetype']:compare(found.dxf.get(key),layer.dxf.get(key),name+'/layer/'+layer.dxf.name+'/'+key)
        for style in ['PROEFSLEUVEN_LIBMONO','KADASTRALE_LABELS']:
            assert actual.styles.get(style).dxf.font==original.styles.get(style).dxf.font
        for index,entity in enumerate(actual.modelspace().query('IMAGE')):
            path=Path(actual.entitydb[entity.dxf.image_def_handle].dxf.filename)
            source=case['request']['layers'][index]['path']
            source=FIXTURE/Path(source).name
            assert path.is_absolute() and path.is_file()
            assert path.read_bytes()==source.read_bytes(),(name,'TIFF bytes changed',path)
            with Image.open(path) as a,Image.open(source) as b:assert a.size==b.size and a.convert('RGBA').tobytes()==b.convert('RGBA').tobytes()
            images+=1
        report=actual.audit();assert not report.has_errors and not report.fixes,(name,report.errors,report.fixes)
        entities+=len(actual.modelspace())
        count,rasters=template_overview(case,output,exporter,models);template_entities+=count;template_images+=rasters
    print(f'{len(json.loads((FIXTURE/"reference.json").read_text(encoding="utf8")))} real standalone cadastral overview exports / {entities} original-matched complete entities / {images} affine IMAGEs and byte-identical TIFFs; all three trench modes, labels, colors, fills, headers and reciprocal image graphs passed')
    print(f'8 actual paired-template exports / {template_entities} independently written original overview entities / {template_images} affine TIFFs; original draw order, CAD/profile separation and no new audit repairs passed')
if __name__=='__main__':main()
