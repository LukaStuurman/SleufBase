"""Read native DXF through original metadata inspectors and independent ezdxf."""
from pathlib import Path
import json,sys
from generate_template_variant_fixtures import snapshot
ROOT=Path(__file__).resolve().parents[1]
def main():
    out=Path(sys.argv[1]);fixture=json.loads((ROOT/'rust/tests/fixtures/template-variants/reference.json').read_text(encoding='utf-8'))
    count=0
    for c in fixture['cases']:
        actual=snapshot(out/(c['name']+'.dxf'),c['wrappers'])
        if actual!=c['expected']:
            for k in actual:
                if actual[k]!=c['expected'][k]:raise AssertionError((c['name'],k,str(actual[k])[:1800],str(c['expected'][k])[:1800]))
        count+=len(c['pairs'])
    print(f'{len(fixture["cases"])} native container drawings / {count} dynamic pairs: all donor tags/references, states, grips, child geometry/layers/attributes/images/annotations and repair-free DXF audits match original')
if __name__=='__main__':main()
