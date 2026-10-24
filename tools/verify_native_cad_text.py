"""Fresh native vector glyphs vs original ezdxf, with no saved expected output."""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import json
import subprocess
import sys

import ezdxf
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from fontTools.ttLib import TTFont
from generate_cad_text_fixtures import paths, spec, attributes
from generate_dxf_text_metrics_fixtures import build_font
from generate_stroke_font_fixtures import shx


def ink(path):
    out=[];current=None;at=0
    for kind in path['kinds']:
        n=max(1,kind)
        if kind==0:
            if current and current['commands']:out.append(current)
            current=dict(start=path['points'][at],commands=[])
        else:current['commands'].append(dict(kind=kind,points=path['points'][at:at+n]))
        at+=n
    if current and current['commands']:out.append(current)
    assert at==len(path['points'])
    return out


def compare(a,b,name):
    if isinstance(b,(float,int)):
        assert abs(a-b)<=1e-10+128*sys.float_info.epsilon*max(abs(a),abs(b)),(name,a,b)
    elif isinstance(b,list):
        assert len(a)==len(b),(name,len(a),len(b))
        for i,(x,y) in enumerate(zip(a,b)):compare(x,y,f'{name}[{i}]')
    elif isinstance(b,dict):
        assert a.keys()==b.keys(),name
        for key in b:compare(a[key],b[key],f'{name}.{key}')
    else:assert a==b,(name,a,b)


def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-CAD-text-runtime-') as temp:
        root=Path(temp);names=[]
        for i in range(3):
            name=f'fresh-vector-{i}.ttf';names.append(name);path=root/name;build_font(path)
            f=TTFont(path);f.recalcTimestamp=False
            for char in ['x','A','p','M']:
                glyph=f['glyf'][char]
                for j,(x,y) in enumerate(glyph.coordinates):glyph.coordinates[j]=(x+3*i,y+7*i)
                advance,_=f['hmtx'].metrics[char];f['hmtx'].metrics[char]=(advance+29*i,13-7*i)
            f.save(path)
            name=f'fresh-vector-{i}.shx';names.append(name)
            programs={32:[2,8,5+i,0,0],65:[10,12+i,4,2,8,41+3*i,0,0],
                      120:[2,8,0,-2,1,12,7+i,5,60+i,2,8,5,-3,0],112:[12,8+i,-7,-127+i,2,8,6,7,0],
                      0x431:[7,65,1,8,3,0,0]}
            (root/name).write_bytes(shx(programs,above=19+2*i,below=4+i,unicode=True))
            name=f'fresh-vector-{i}.lff';names.append(name)
            (root/name).write_text(f'''#Name: Own fresh vector LFF {i}
#LetterSpacing: {1.25+i*.3}
#WordSpacing: {7+i}
[0041]
0,0;{9+i},0;{9+i},22,A0.7;0,22;0,0
[0078]
0,-2;{7+i},-2,A-0.45;{7+i},11;0,11;0,-2
[0070]
0,-6;{8+i},-6,A1;{8+i},13;0,13;0,-6
[0431]
C0041
19,3;27,16,A-0.8
''',encoding='utf-8')
        manager=FontManager();manager.build([str(root)],support_dirs=False)
        fonts.TrueTypeFont._glyph_caches.clear();requests=[];expected=[]
        with patch.object(fonts,'font_manager',manager):
            for name in names:
                doc=ezdxf.new('R2018');doc.styles.get('Standard').dxf.font=name;font=fonts.make_font(name,1.)
                for content in ['Axp MØ','б ?Axp']:
                    for h,v in [(h,0) for h in range(6)]+[(h,v) for v in [1,2,3] for h in range(3)]:
                        e=doc.modelspace().add_text(content,dxfattribs=attributes(content,len(requests),h,v))
                        requests.append(dict(font=name,text=spec(e)));expected.append(paths(e,font))
        source=root/'input.json';target=root/'actual.json';source.write_text(json.dumps(dict(directories=[str(root)],texts=requests)),encoding='utf-8')
        subprocess.run([driver,str(source),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        for i,(a,b) in enumerate(zip(actual,expected)):compare(ink(a),b,f'fresh vector {i} {requests[i]["font"]}')
        print(f'{len(names)} fresh TTF/SHX/LFF fonts / {len(expected)} compiled vector cases match original, including LSB, baseline, curves, all alignments and OCS')


if __name__=='__main__':main()
