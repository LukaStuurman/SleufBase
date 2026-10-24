"""Compare equivalent nearest-neighbor TIFF previews in fresh native/Pillow processes.

Build: cargo build --release --manifest-path rust/Cargo.toml --example benchmark_raster
Run: python tools/benchmark_raster.py --native rust/target/release/examples/benchmark_raster.exe
Uses the synthetic >16M pixel fixture by default. Reports process peak resident
memory and decode time, not total application/GPU memory or end-to-end rendering.
"""
from __future__ import annotations
import argparse
import ctypes
import json
import os
from pathlib import Path
import subprocess
import statistics
import sys
import time
import zlib

ROOT=Path(__file__).resolve().parents[1]


def peak_bytes():
    if os.name!="nt":
        import resource
        value=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        return value if sys.platform=="darwin" else value*1024
    class Counters(ctypes.Structure):
        _fields_=[("cb",ctypes.c_uint32),("PageFaultCount",ctypes.c_uint32)]+[
            (key,ctypes.c_size_t) for key in ["PeakWorkingSetSize","WorkingSetSize","QuotaPeakPagedPoolUsage",
                "QuotaPagedPoolUsage","QuotaPeakNonPagedPoolUsage","QuotaNonPagedPoolUsage","PagefileUsage","PeakPagefileUsage"]]
    api=ctypes.WinDLL("kernel32",use_last_error=True)
    api.GetCurrentProcess.restype=ctypes.c_void_p
    api.K32GetProcessMemoryInfo.argtypes=[ctypes.c_void_p,ctypes.POINTER(Counters),ctypes.c_uint32]
    api.K32GetProcessMemoryInfo.restype=ctypes.c_int
    counters=Counters();counters.cb=ctypes.sizeof(counters)
    if not api.K32GetProcessMemoryInfo(api.GetCurrentProcess(),ctypes.byref(counters),counters.cb):
        raise ctypes.WinError(ctypes.get_last_error())
    return counters.PeakWorkingSetSize


def pillow(path):
    from PIL import Image, __version__
    start=time.perf_counter()
    with Image.open(path) as source:
        rgba=source.convert("RGBA")
        scale=min(1,2048/rgba.width,2048/rgba.height,((2048*2048)/(rgba.width*rgba.height))**0.5)
        dimensions=(max(1,int(rgba.width*scale)),max(1,int(rgba.height*scale)))
        preview=rgba.resize(dimensions,Image.Resampling.NEAREST)
        elapsed=(time.perf_counter()-start)*1000
        crc=zlib.crc32(preview.tobytes())
        report=dict(mode="Pillow-full-RGBA-preview",pillow_version=__version__,python_version=sys.version.split()[0],
            file=path.name,width=preview.width,height=preview.height,crc32=crc,elapsed_ms=elapsed,peak_working_set_bytes=peak_bytes())
        preview.close();rgba.close()
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native",type=Path)
    parser.add_argument("--raster",type=Path,default=ROOT/"rust/tests/fixtures/rasters/large-striped.tiff")
    parser.add_argument("--rounds",type=int,default=3)
    parser.add_argument("--pillow-child",action="store_true",help=argparse.SUPPRESS)
    args=parser.parse_args()
    if args.pillow_child:
        print(json.dumps(pillow(args.raster)));return
    if args.native is None:parser.error("--native is verplicht")
    def run(command):
        result=subprocess.run(command,check=True,capture_output=True,text=True,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0)
        return json.loads(result.stdout)
    if not 1<=args.rounds<=20:parser.error("--rounds moet tussen 1 en 20 liggen")
    results=[]
    for index in range(args.rounds):
        commands=dict(native=[str(args.native.resolve()),str(args.raster.resolve())],
            pillow=[sys.executable,str(Path(__file__).resolve()),"--pillow-child","--raster",str(args.raster.resolve())])
        order=["native","pillow"] if index%2==0 else ["pillow","native"]
        result={mode:run(commands[mode]) for mode in order}
        native,original=result["native"],result["pillow"]
        if (native["width"],native["height"],native["crc32"])!=(original["width"],original["height"],original["crc32"]):
            raise RuntimeError("Previewpixels/dimensions verschillen; prestatieresultaat niet geaccepteerd")
        results.append(result)
    summary={mode:dict(median_elapsed_ms=statistics.median(r[mode]["elapsed_ms"] for r in results),
        peak_working_set_bytes=max(r[mode]["peak_working_set_bytes"] for r in results)) for mode in ["native","pillow"]}
    print(json.dumps(dict(scope="synthetic decoder/preview only; excludes GUI/GPU and process startup; OS file cache not cleared",rounds=results,summary=summary),indent=2))


if __name__=="__main__":main()
