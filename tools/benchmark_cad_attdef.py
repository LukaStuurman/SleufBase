"""Compare two optimized scene readers on definition-heavy repeated blocks.

Usage: python tools/benchmark_cad_attdef.py before.exe after.exe
Both drivers must include ATTDEF support. Only the block-index optimization
should differ. Output is a component comparison, never an application claim.
The same source and byte-identical scene are required for every sample.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
import hashlib
import json
import statistics
import subprocess
import sys
import time


def main():
    drivers=[str(Path(p).resolve()) for p in sys.argv[1:]]
    if len(drivers)!=2:raise SystemExit('Supply before.exe and after.exe scene readers')
    with TemporaryDirectory(prefix='SleufBase-attdef-benchmark-') as temp:
        root=Path(temp)
        tags=['0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nDefinitions\n']
        tags.extend(f'0\nATTDEF\n5\nD{i}\n2\nFIELD{i}\n3\nPrompt\n1\nDefault\n10\n1000000\n20\n1000000\n' for i in range(10000))
        tags.append('0\nLINE\n5\nABCD\n10\n0\n20\n0\n11\n1\n21\n2\n0\nENDBLK\n0\nBLOCK\n2\nOwner\n0\nINSERT\n5\nABCE\n2\nDefinitions\n10\n0\n20\n0\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nABCF\n2\nOwner\n10\n0\n20\n0\n70\n20000\n44\n2\n0\nENDSEC\n0\nEOF\n')
        drawing=root/'source.dxf';drawing.write_text(''.join(tags),encoding='ascii')
        request=root/'input.json';request.write_text(json.dumps(dict(directories=[],drawings=[str(drawing)])),encoding='utf-8')
        samples=[[],[]];expected=None
        # Warm both processes once; alternate order to reduce order/cache bias.
        for iteration in range(4):
            for variant in ([0,1] if iteration%2==0 else [1,0]):
                target=root/f'output-{iteration}-{variant}.json'
                start=time.perf_counter()
                subprocess.run([drivers[variant],str(request),str(target)],check=True,timeout=120)
                elapsed=time.perf_counter()-start
                raw=target.read_bytes();digest=hashlib.sha256(raw).hexdigest()
                scene=json.loads(raw)
                assert len(scene)==1 and len(scene[0])==20000 and all(e['kind']=='LINE' for e in scene[0])
                if expected is None:expected=digest
                assert digest==expected,'Variants or repeated samples changed the scene'
                if iteration:samples[variant].append(elapsed)
        medians=[statistics.median(s) for s in samples]
        print(json.dumps(dict(definitions=10000,grid_cells=20000,scene_nodes=20000,
            scene_sha256=expected,before_samples_seconds=samples[0],after_samples_seconds=samples[1],
            before_median_seconds=medians[0],after_median_seconds=medians[1],
            before_over_after=medians[0]/medians[1],
            driver_sha256=[hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in drivers]),indent=2))


if __name__=='__main__':main()
