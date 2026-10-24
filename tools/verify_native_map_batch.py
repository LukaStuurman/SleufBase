"""Fresh original standalone pages through the production optimized batch."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json,subprocess,sys
from PIL import Image
import generate_map_batch_fixtures as oracle
def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-map-batch-runtime-') as temp:
        root=Path(temp);oracle.OUT=root/'reference';oracle.SEED=751713
        oracle.ANGLES=[0.,-12.7,90.,157.2,31.1,0.]
        cases=oracle.main()['cases'];pixels=0
        for i,c in enumerate(cases):
            source=dict(path=str(oracle.OUT/c['path']),width=53,height=37,transform=c['transform'],bounds=c['bounds'],epsg=28992,kickthemap_job_id=None,kickthemap_job_title='',kickthemap_job_sidecar=None)
            request=dict(rasters=[source],settings=dict(dpi=c['dpi'],scale=c['scale'],background='white'),fonts=dict(regular=str(oracle.FONT),bold=str(oracle.FONT)))
            manifest=root/f'input-{i}.json';manifest.write_text(json.dumps(request),encoding='utf-8')
            output=root/f'output-{i}';subprocess.run([driver,str(manifest),str(output)],check=True,timeout=120,capture_output=True)
            with Image.open(output/c['expected']['name']) as page:
                assert page.mode==c['expected']['mode'] and list(page.size)==c['expected']['size']
                assert all(abs(a-b)<1e-9 for a,b in zip(page.info['dpi'],c['expected']['dpi']))
                for x,y,color in c['expected']['samples']:assert list(page.getpixel((x,y)))==color,(c['path'],x,y,page.getpixel((x,y)),color)
                pixels+=len(c['expected']['samples'])
        print(f'{len(cases)} fresh standalone RGB/DPI pages / {pixels} original pixel samples match optimized native batches with altered TIFF pixels and affine angles')
if __name__=='__main__':main()
