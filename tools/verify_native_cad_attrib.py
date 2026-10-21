"""Fresh ASCII/binary drawings audit the optimized reader/scene, not goldens."""
from pathlib import Path
from tempfile import TemporaryDirectory
import json
import shutil
import subprocess
import sys

import ezdxf
import numpy as np
from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from ezdxf.math import Vec3
from ezdxf.path import Path as GlyphPath
from generate_cad_attrib_fixtures import build,evaluate,FILES,ASSETS


def chord_error(points,polyline):
    a=np.asarray(polyline,dtype=np.float64);p=np.asarray(points,dtype=np.float64)
    start=a[:-1];delta=a[1:]-start;v=p[:,None,:]-start[None,:,:]
    norm=np.einsum('ij,ij->i',delta,delta);norm=np.where(norm==0,1,norm)
    t=np.clip(np.einsum('pij,ij->pi',v,delta)/norm[None,:],0,1)
    diff=v-t[:,:,None]*delta[None,:,:]
    return float(np.sqrt(np.einsum('pij,pij->pi',diff,diff).min(axis=1)).max())


def compare_paths(actual,expected,name):
    assert len(actual)==len(expected),(name,'contours',len(actual),len(expected))
    for a,w in zip(actual,expected):
        p=GlyphPath(w['start'])
        for c in w['commands']:
            points=c['points'];kind=c['kind']
            if kind==1:p.line_to(points[0])
            elif kind==2:p.curve3_to(points[1],ctrl=points[0])
            elif kind==3:p.curve4_to(points[2],ctrl1=points[0],ctrl2=points[1])
            else:raise AssertionError(kind)
        original=[list(v) for v in p.flattening(.00025)]
        assert chord_error(original,a)<=.0050001,(name,'original curve vs native chords')
        assert chord_error(a,original)<=.0052501,(name,'native vs original dense chords')
        assert np.max(np.abs(np.asarray(a[0])-w['start']))<1e-8,name
        assert np.max(np.abs(np.asarray(a[-1])-original[-1]))<1e-8,name


def main():
    driver=str(Path(sys.argv[1]).resolve())
    with TemporaryDirectory(prefix='SleufBase-attrib-runtime-') as temp:
        root=Path(temp)
        for file in FILES:shutil.copyfile(ASSETS/file,root/Path(file).name)
        manager=FontManager();manager.build([str(root)],support_dirs=False);fonts.TrueTypeFont._glyph_caches.clear()
        drawings=[];expected=[]
        for n in range(4):
            doc=build();doc.layers.get('Root').dxf.true_color=0x315abc+17*n
            for e in list(doc.entitydb.values()):
                if e.dxftype()=='INSERT':
                    e.dxf.rotation+=13*n;e.dxf.xscale*=(-.8 if n%2 else 1.1);e.dxf.yscale*=1.+n*.15
                    e.dxf.column_spacing*=1.+n*.13;e.dxf.row_spacing*=1.+n*.17
                elif e.dxftype()=='ATTRIB' and not e.has_embedded_mtext_entity:
                    shift=Vec3(n*.75,-n*.5,n*.1);e.dxf.insert+=shift
                    if e.dxf.hasattr('align_point'):e.dxf.align_point+=shift
                    e.dxf.height*=1.+n*.11;e.dxf.width*=1.+n*.07;e.dxf.rotation-=9*n;e.dxf.oblique-=2*n
                    e.dxf.text=e.dxf.text.replace('Axp BC','A%%cx^Jp MØ')
            path=root/f'fresh-{n}.dxf';doc.saveas(path,fmt='bin' if n%2 else 'asc')
            loaded=ezdxf.readfile(path);drawings.append(str(path));expected.append(evaluate(loaded,manager)['scene'])
        manifest=root/'input.json';target=root/'actual.json'
        manifest.write_text(json.dumps(dict(directories=[str(root)],drawings=drawings)),encoding='utf-8')
        subprocess.run([driver,str(manifest),str(target)],check=True,timeout=120)
        actual=json.loads(target.read_text(encoding='utf-8'));assert len(actual)==len(expected)
        entities=0;attributes=0
        for scene,wanted in zip(actual,expected):
            lookup={(e['handle'],e['blockpath']):e for e in wanted};assert len(scene)==len(lookup)
            for node in scene:
                name=(node['handle'],node['blockpath']);want=lookup.pop(name)
                for key in ['kind','layer','rgb','visible']:assert node[key]==want[key],(name,key,node[key],want[key])
                if node['kind']=='ATTRIB':
                    for key in ['tag','flags','field_length','lock_position']:assert node['attribute'][key]==want[key],(name,key)
                    assert node['content']==want['content'],name
                    compare_paths(node['paths'],want['paths'],name);attributes+=1
                else:
                    assert len(node['paths'])==1
                    assert np.max(np.abs(np.asarray(node['paths'][0])-want['points']))<1e-8,name
                entities+=1
            assert not lookup
        print(f'{len(drawings)} fresh ASCII/binary drawings / {entities} scene entities / {attributes} compiled ATTRIB instances match independent geometry, visibility, metadata and corrected MINSERT anchors')


if __name__=='__main__':main()
