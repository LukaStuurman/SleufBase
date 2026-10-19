"""Fresh-process original/native load + expansion of the real shipped template."""
from pathlib import Path
import ctypes
import hashlib
import importlib
import json
import platform
import statistics
import subprocess
import sys
import time
ROOT=Path(__file__).resolve().parents[1]

def peak_bytes():
    if sys.platform!='win32':return None
    from ctypes import wintypes
    class Counters(ctypes.Structure):
        _fields_=[('cb',wintypes.DWORD),('PageFaultCount',wintypes.DWORD)]+[(name,ctypes.c_size_t) for name in ('PeakWorkingSetSize','WorkingSetSize','QuotaPeakPagedPoolUsage','QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage','QuotaNonPagedPoolUsage','PagefileUsage','PeakPagefileUsage')]
    kernel=ctypes.WinDLL('kernel32',use_last_error=True);psapi=ctypes.WinDLL('psapi',use_last_error=True)
    kernel.GetCurrentProcess.restype=wintypes.HANDLE
    psapi.GetProcessMemoryInfo.argtypes=[wintypes.HANDLE,ctypes.POINTER(Counters),wintypes.DWORD]
    counters=Counters();counters.cb=ctypes.sizeof(counters)
    assert psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(),ctypes.byref(counters),counters.cb)
    return counters.PeakWorkingSetSize

def original():
    import ezdxf
    from generate_object_export_fixtures import reference
    ref=reference();exporter=importlib.import_module(ref.__package__+'.cadastral_export').CadastralDxfExporter(wfs_client=None)
    start=time.perf_counter();doc=ezdxf.readfile(ROOT/'assets/cadastral_template.dxf')
    before=len(exporter._detect_template_slots(doc));exporter._extend_template_for_slot_count(doc,64)
    slots=len(exporter._detect_template_slots(doc));layouts=len(doc.layouts);entities=len(doc.modelspace())
    elapsed_ms=(time.perf_counter()-start)*1000
    print(json.dumps(dict(elapsed_ms=elapsed_ms,peak_working_set_bytes=peak_bytes(),slots=slots,added_pages=(slots-before)//8,layouts=layouts,model_entities=entities)))

def main():
    if '--original-child' in sys.argv:original();return
    exe=Path(sys.argv[1]).resolve();output=Path(sys.argv[2]);path=ROOT/'assets/cadastral_template.dxf';rounds=[]
    for _ in range(3):
        native=json.loads(subprocess.check_output([str(exe),str(path)],text=True,encoding='utf-8'))
        previous=json.loads(subprocess.check_output([sys.executable,__file__,'--original-child'],text=True,encoding='utf-8'))
        assert native['build_mode']=='release'
        for key in ('slots','added_pages','layouts','model_entities'):assert native[key]==previous[key],(key,native,previous)
        rounds.append(dict(native=native,original=previous))
    result=dict(rounds=rounds,platform=platform.platform(),python=sys.version.split()[0],file=path.name,source_bytes=path.stat().st_size,source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),scope='Three fresh processes per implementation. Timed: local real-template read/parse, extension from 32 to 64 slots, final slot/layout/model-entity counts. Excludes interpreter/import/process startup time, output DXF serialization, metadata/viewport fitting, rasters, network, GUI/GPU and the complete export workflow. Windows process peak working set includes startup/import baselines and all temporary expansion documents; it is not isolated allocation or full-app peak memory. File-system cache state is uncontrolled. Geometry and object graphs are checked separately by independent release readback.')
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('Median load + 64-slot expansion: native',round(statistics.median(r['native']['elapsed_ms'] for r in rounds),3),'ms; original',round(statistics.median(r['original']['elapsed_ms'] for r in rounds),3),'ms')
    print('Median process peak working set: native',statistics.median(r['native']['peak_working_set_bytes'] for r in rounds),'bytes; original',statistics.median(r['original']['peak_working_set_bytes'] for r in rounds),'bytes')

if __name__=='__main__':main()
