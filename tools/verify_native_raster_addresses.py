"""Fresh original complete TIFF-address batches against the optimized worker core."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,subprocess,sys
import generate_raster_address_fixtures as oracle
def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-raster-address-runtime-') as temp:
        root=Path(temp);oracle.OUT=root/'original';oracle.SEED=91451;oracle.COUNT=81
        expected=oracle.main()['cases']
        sources=[c['source'] for c in expected]
        replies=[dict(kind='error',value=c['error']) if c['error'] else dict(kind='rows',value=c['response']) for c in expected]
        source=root/'input.json';target=root/'actual.json'
        source.write_text(json.dumps([dict(kind='raster_addresses',sources=sources,replies=replies)],ensure_ascii=False),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert actual==[[c['record'] for c in expected]]
        print(f'{len(expected)} fresh complete-batch rows / {len(expected)*3} original clipboard contracts match optimized native code, including failed/empty requests, duplicates, long Unicode names and distance rounding')
if __name__=='__main__':main()
