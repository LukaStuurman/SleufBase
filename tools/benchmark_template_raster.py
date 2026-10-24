"""Fresh-process native/Pillow preparation from the same resident RGBA input."""
from pathlib import Path
import importlib,json,platform,statistics,subprocess,sys,tempfile,time,zlib
from types import SimpleNamespace
from benchmark_template_pages import peak_bytes
ROOT=Path(__file__).resolve().parents[1]
def original(mode,raw_output):
    import numpy as np
    from PIL import Image
    from generate_object_export_fixtures import reference
    ref=reference();module=importlib.import_module(ref.__package__+'.cadastral_export');models=importlib.import_module(ref.__package__+'.models')
    exporter=module.CadastralDxfExporter(wfs_client=None);w,h=1400,1000
    pixels=np.empty((h,w,4),dtype=np.uint8);x=np.arange(w,dtype=np.uint32);alpha=np.array([0,1,15,16,127,254,255],dtype=np.uint8)
    for y in range(h):
        pixels[y,:,0]=(x*71+y*13)%256;pixels[y,:,1]=(x*37+y*61)%256;pixels[y,:,2]=(x*19+y*43)%256;pixels[y,:,3]=alpha[(x+y*3)%7]
        if y<h//8 or y>=h*7//8:pixels[y,:,:]=[250,250,250,255]
        else:pixels[y,:w//8,:]=[250,250,250,255];pixels[y,w*7//8:,:]=[250,250,250,255]
    image=Image.fromarray(pixels);layer=SimpleNamespace(image=image,transform=models.GeoTransform(1,0,0,0,-1,0))
    profile=SimpleNamespace(start_point=SimpleNamespace(x=0.,y=0.),end_point=SimpleNamespace(x=10.,y=3.)) if mode=='rotate' else None
    start=time.perf_counter();result=image.convert('RGBA');vector=exporter._template_tiff_orientation_pixel_vector(layer,[],[],profile)
    if vector is not None and sum(v*v for v in vector)>1e-6:
        angle=exporter._normalize_rotation_degrees(270.-exporter._template_bearing_from_pixel_vector(*vector))
        result=result.rotate(angle,resample=Image.Resampling.BICUBIC,expand=True,fillcolor=(255,255,255,0))
    result=exporter._normalize_template_tiff_raster_alpha(exporter._crop_template_tiff_to_visible_bounds(result))
    elapsed_ms=(time.perf_counter()-start)*1000;peak=peak_bytes()
    rgba=result.tobytes()
    with raw_output.open('xb') as f:f.write(rgba)
    print(json.dumps(dict(mode=mode,elapsed_ms=elapsed_ms,peak_working_set_bytes=peak,width=result.width,height=result.height,crc32=zlib.crc32(rgba))))
def main():
    if '--original-child' in sys.argv:original(sys.argv[-2],Path(sys.argv[-1]));return
    exe=Path(sys.argv[1]).resolve();output=Path(sys.argv[2]);modes={}
    for mode in ('alpha','rotate'):
        rounds=[]
        for _ in range(3):
            with tempfile.TemporaryDirectory(prefix='sleufbase-raster-benchmark-') as td:
                native_raw=Path(td)/'native.rgba';original_raw=Path(td)/'original.rgba'
                native=json.loads(subprocess.check_output([str(exe),mode,str(native_raw)],text=True,encoding='utf-8'))
                previous=json.loads(subprocess.check_output([sys.executable,__file__,'--original-child',mode,str(original_raw)],text=True,encoding='utf-8'))
                assert native_raw.read_bytes()==original_raw.read_bytes(),(mode,'exact RGBA bytes differ')
            assert native['build_mode']=='release'
            for key in ('width','height','crc32'):assert native[key]==previous[key],(mode,key,native,previous)
            rounds.append(dict(native=native,original=previous))
        modes[mode]=rounds
        print(mode,'median native/original ms:',*[round(statistics.median(r[k]['elapsed_ms'] for r in rounds),3) for k in ('native','original')])
    report=dict(modes=modes,platform=platform.platform(),python=sys.version.split()[0],input=[1400,1000],scope='Three fresh processes per mode/implementation, same resident 1,400 x 1,000 RGBA pattern. Timed: original orientation decision, optional expanded premultiplied bicubic rotation, visible-alpha crop and edge-connected near-white alpha normalization. Rust consumes its owned input; Python follows original convert/crop/NumPy helper copies and any available original acceleration. Source/import/input-construction startup, TIFF decode, PNG encoding, DXF/network/GUI and pixel checksums/raw-output comparisons are excluded from timing. Peak working set is captured immediately after preparation and includes runtime/import/input-construction baselines; it is not isolated allocations. Native/Pillow dimensions, every raw RGBA byte and CRC32 agree in every round; byte comparison occurs after timing and peak capture. File-system cache is not controlled; this is an image-stage comparison, not a full export/app benchmark.')
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
