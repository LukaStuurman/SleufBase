from pathlib import Path
import json,sys
from PIL import Image
from generate_template_raster_fixtures import OUT
from verify_native_template_images import compare
def main():
    output=Path(sys.argv[1]);count=0;pixels=0
    for c in json.loads((OUT/'reference.json').read_text(encoding='utf-8')):
        if c['intentional_difference'] is not None:
            assert c['input'] in {f'orientation-{i}.tiff' for i in (5,6,7,8)}
            difference=c['intentional_difference'];assert difference['original_details']['source_size']==c['details']['source_size'][::-1]
            assert difference['original_expected']!=c['expected'] and (OUT/difference['original_expected']).is_file()
        compare(json.loads((output/(c['name']+'.json')).read_text(encoding='utf-8')),c['details'],c['name'])
        with Image.open(output/(c['name']+'.png')) as native,Image.open(OUT/c['expected']) as reference:
            assert native.size==reference.size,(c['name'],native.size,reference.size)
            a=native.convert('RGBA').tobytes();b=reference.convert('RGBA').tobytes()
            if a!=b:
                i=next(i for i,(x,y) in enumerate(zip(a,b)) if x!=y);raise AssertionError((c['name'],'pixel',i//4,'channel',i%4,a[i//4*4:i//4*4+4],b[i//4*4:i//4*4+4]))
            pixels+=native.width*native.height
        count+=1
    corrected=sum(c['intentional_difference'] is not None for c in json.loads((OUT/'reference.json').read_text(encoding='utf-8')))
    assert corrected==4
    print(count,'raster preparations:',pixels,'exact RGBA pixels, orientation, dimensions and crop verified;',corrected,'documented TIFF orientation corrections')
if __name__=='__main__':main()
