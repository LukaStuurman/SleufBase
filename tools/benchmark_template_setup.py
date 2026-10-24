"""Fresh-process comparison of composed template preparation, without output I/O."""
from pathlib import Path
from datetime import datetime
from types import SimpleNamespace
import hashlib,importlib,json,platform,statistics,subprocess,sys,time
from benchmark_template_pages import peak_bytes
ROOT=Path(__file__).resolve().parents[1]
def original():
    import ezdxf
    from generate_object_export_fixtures import reference
    ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    metadata=importlib.import_module(ref.__package__+'.template_metadata');Bounds=importlib.import_module(ref.__package__+'.models').Bounds
    layers=[SimpleNamespace(bounds=Bounds(120000,450000,120030,450040)) for _ in range(64)]
    start=time.perf_counter();doc=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf');before=len(exporter._detect_template_slots(doc));exporter._extend_template_for_slot_count(doc,64)
    names=exporter._used_template_layout_names(64);exporter._ensure_template_image_layer(doc);exporter._setup_text_styles(doc)
    previous=sum(bool(list(doc.layouts.get(n).query('INSERT'))) for n in names[1:]);exporter._ensure_template_layout_legends(doc,names)
    doc.units=6;doc.set_raster_variables(frame=0,quality=1,units='m');exporter._update_template_scale_list(doc)
    metadata.update_title_blocks(doc,names,project='Benchmark project',drawn_by='User',export_date=datetime(2026,10,19))
    exporter._update_template_layout_page_numbers(doc,names);exporter._configure_template_wireframe_viewports(doc,layers,names,18.)
    slots=len(exporter._detect_template_slots(doc));layouts=len(doc.layouts);attributes=sum(e.dxftype()=='ATTRIB' for e in doc.entitydb.values())
    restored=sum(bool(list(doc.layouts.get(n).query('INSERT'))) for n in names[1:])-previous
    elapsed_ms=(time.perf_counter()-start)*1000
    print(json.dumps(dict(elapsed_ms=elapsed_ms,peak_working_set_bytes=peak_bytes(),slots=slots,added_pages=(slots-before)//8,restored_legends=restored,layouts=layouts,attributes=attributes)))
def main():
    if '--original-child' in sys.argv:original();return
    exe=Path(sys.argv[1]).resolve();output=Path(sys.argv[2]);path=ROOT/'assets/cadastral_template.dxf';rounds=[]
    for _ in range(3):
        native=json.loads(subprocess.check_output([str(exe),str(path)],text=True,encoding='utf-8'))
        previous=json.loads(subprocess.check_output([sys.executable,__file__,'--original-child'],text=True,encoding='utf-8'))
        assert native['build_mode']=='release'
        for key in ('slots','added_pages','restored_legends','layouts','attributes'):assert native[key]==previous[key],(key,native,previous)
        rounds.append(dict(native=native,original=previous))
    report=dict(rounds=rounds,platform=platform.platform(),python=sys.version.split()[0],source_bytes=path.stat().st_size,source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),scope='Three fresh processes per implementation. Timed: local real-template load/parse, extension to 64 slots, text/image-layer preparation, legend/affine attribute regeneration, drawing/raster units, scale-list cleanup, project/date/page fields, overview fitting and final counts. Native final record reindexing is included. Excludes process/interpreter/import startup time, final DXF output, rasters/photos, BGT/network, profile/object drawing and GUI/GPU. Windows process peak working set includes runtime/import baselines and temporary preparation documents; it is not isolated allocation or a full-app peak. File-system cache is uncontrolled. Geometry and graphs are verified separately against original readback.')
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print('Median load + composed preparation: native',round(statistics.median(r['native']['elapsed_ms'] for r in rounds),3),'ms; original',round(statistics.median(r['original']['elapsed_ms'] for r in rounds),3),'ms')
    print('Median process peak working set: native',statistics.median(r['native']['peak_working_set_bytes'] for r in rounds),'bytes; original',statistics.median(r['original']['peak_working_set_bytes'] for r in rounds),'bytes')
if __name__=='__main__':main()
