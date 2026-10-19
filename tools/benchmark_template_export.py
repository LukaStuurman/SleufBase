"""Native prepared export only; not a Python or full-app comparison."""
from pathlib import Path
import hashlib,json,platform,statistics,subprocess,sys
ROOT=Path(__file__).resolve().parents[1]
def main():
    exe=Path(sys.argv[1]).resolve();output=Path(sys.argv[2]);rounds=[]
    for _ in range(3):
        r=json.loads(subprocess.check_output([str(exe)],text=True,encoding='utf-8'));assert r['build_mode']=='release'
        assert (r['filled_slots'],r['profiles'],r['image_files'])==(3,2,6)
        if rounds:assert r['drawing_bytes']==rounds[0]['drawing_bytes']
        rounds.append(r)
    source=ROOT/'assets/cadastral_template.dxf';fixture=ROOT/'rust/tests/fixtures/template-export/reference.json'
    report=dict(rounds=rounds,platform=platform.platform(),source_bytes=source.stat().st_size,source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),scope='Three fresh native release processes. Timed: original real-template load/parse; complete template preparation; three filled slots and one blank; two complete active profiles; six already-prepared small RGBA PNG copies and full pixel-stream validation; one image graph batch; original per-slot drawing-order restoration; final DXF write, asset flush/fsync and atomic publication. Excludes process startup, input-manifest parsing/path resolution, raster/map rendering and fetch, network/BGT preparation, logos/contact restoration, GUI/GPU and output cleanup. Peak process working set includes startup/input parsing baselines and temporary documents. File-system cache is uncontrolled. This is a native prepared-input baseline, not a Python, raster-rendering or full-application comparison. Separate independent readback checks fields/order/geometry/ownership and PNG bytes/pixels.')
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print('Native prepared export median:',round(statistics.median(r['elapsed_ms'] for r in rounds),3),'ms;',statistics.median(r['peak_working_set_bytes'] for r in rounds),'bytes process peak working set')
if __name__=='__main__':main()
