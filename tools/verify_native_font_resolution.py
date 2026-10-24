"""Compare compiled factory routes and full text extents with original goldens."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json, subprocess, sys
from verify_native_template_extents import compare

ROOT=Path(__file__).resolve().parents[1]
def main():
    reference=json.loads((ROOT/'rust/tests/fixtures/font-resolution/reference.json').read_text(encoding='utf-8'))
    with TemporaryDirectory(prefix='SleufBase-font-factory-compiled-') as temp:
        output=Path(temp)/'actual.json'
        subprocess.run([str(Path(sys.argv[1]).resolve()),str(output)],check=True,timeout=120)
        actual=json.loads(output.read_text(encoding='utf-8'))
        assert len(actual)==len(reference['cases'])
        queries=0
        for case,result in zip(reference['cases'],actual):
            assert case['name']==result['name']
            assert len(case['expected'])==len(result['actual'])
            for i,(expected,found) in enumerate(zip(case['expected'],result['actual'])):
                if expected['status']=='error':assert found['status']=='error',(case['name'],i,found)
                else:compare(found,expected,f'factory {case["name"]} query {i}')
                queries+=1
        print(f'{len(actual)} original/native factory scenarios / {queries} queries: actual font files, metrics and corners match, including BIGFONT, unsupported glyphs, missing files, cache and fatal errors')

if __name__=='__main__':main()
