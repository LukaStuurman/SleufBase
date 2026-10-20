"""Original ezdxf fast TEXT extents with a redistributable synthetic font.

No installed fonts or application/settings/account initialization are needed.
"""
from pathlib import Path
from unittest.mock import patch
import json,os
import ezdxf
from ezdxf.disassemble import make_primitive
from ezdxf.fonts import fonts
from ezdxf.tools.text import plain_text
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_DXF_TEXT_METRICS_FIXTURES',ROOT/'rust/tests/fixtures/dxf-text-metrics'))

def build_font(path):
    fb=FontBuilder(2048,isTTF=True)
    boxes={'.notdef':(0,-110,500,1000),'x':(10,-35,600,980),'A':(0,-20,1000,1500),'p':(0,-475,600,1100),'space':None,'M':(-20,0,1200,1500),'Oslash':(0,-20,1100,1520)}
    fb.setupGlyphOrder(list(boxes))
    fb.setupCharacterMap({32:'space',120:'x',65:'A',112:'p',77:'M',216:'Oslash'})
    glyphs={}
    for name,box in boxes.items():
        pen=TTGlyphPen(None)
        if name=='x':
            pen.moveTo((10,0));pen.qCurveTo((300,-100),(600,0));pen.lineTo((600,980));pen.lineTo((10,980));pen.closePath()
        elif name=='A':
            pen.moveTo((0,-20));pen.lineTo((1000,-20));pen.lineTo((1000,1500));pen.qCurveTo((500,1700),(0,1500));pen.closePath()
        elif name=='p':
            pen.moveTo((0,-475));pen.qCurveTo((300,-600),(600,-475));pen.lineTo((600,1100));pen.lineTo((0,1100));pen.closePath()
        elif box:
            x0,y0,x1,y1=box;pen.moveTo((x0,y0));pen.lineTo((x1,y0));pen.lineTo((x1,y1));pen.lineTo((x0,y1));pen.closePath()
        glyphs[name]=pen.glyph()
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics({n:(w,0) for n,w in zip(boxes,[999,700,1100,720,300,1250,1150])})
    fb.setupHorizontalHeader(ascent=1600,descent=-500)
    fb.setupOS2(sTypoAscender=1600,sTypoDescender=-500,usWinAscent=1600,usWinDescent=500)
    fb.setupNameTable({'familyName':'SleufBase Extent Fixture','styleName':'Regular','uniqueFontIdentifier':'SleufBaseFixture1','fullName':'SleufBase Extent Fixture Regular','psName':'SleufBaseFixture-Regular'})
    fb.setupPost();fb.setupMaxp();fb.setupHead(created=3406620153,modified=3406620153)
    fb.save(path)

def main():
    assert ezdxf.__version__=='1.4.4'
    OUT.mkdir(parents=True,exist_ok=True);path=OUT/'SleufBaseFixture.ttf';build_font(path)
    doc=ezdxf.new('R2018');ms=doc.modelspace()
    texts=['','   ','Axp MØ','%%c110 %%D %%p','%%uA%%u %%kM%%K','A^Ip^Jx^MØ','Straße 漢🛠','AV To 12345','^^^','A%%Kx','^é^漢^^','A\r\np^^']
    alignments=[(h,0) for h in range(6)]+[(h,v) for v in [1,2,3] for h in range(3)]
    normals=[(0,0,1),(0,0,-1),(1,2,3),(0.001,0.002,5)]
    cases=[]
    with patch.object(fonts.font_manager,'get_ttf_font',side_effect=lambda *a,**k:TTFont(path)):
        for font_name in ['SleufBaseFixture.ttf',fonts.MONOSPACE]:
            doc.styles.get('Standard').dxf.font=font_name
            for i,content in enumerate(texts):
                for j,(h,v) in enumerate(alignments):
                    n=len(cases);a=(10.25,-3.75,2.);b=a if n%11==0 else (22.,-1.5,4.)
                    e=ms.add_text(content,dxfattribs=dict(insert=a,align_point=b,height=[.25,1.,2.5][n%3],width=[.7,1.,1.4][(n//3)%3],rotation=[0,27,90,180,-23][n%5],oblique=[0,17,-12][n%3],halign=h,valign=v,text_generation_flag=[0,2,4,6][n%4],extrusion=normals[n%4]))
                    d=e.dxf
                    spec=dict(content=d.text,insert=list(d.insert),align_point=list(d.align_point),height=d.height,width=d.width,rotation=d.rotation,oblique=d.oblique,halign=d.halign,valign=d.valign,generation=d.text_generation_flag,normal=list(d.extrusion))
                    font=fonts.make_font(font_name,d.height,d.width);fm=font.measurements
                    primitive=make_primitive(e);vertices=list(primitive.path.control_vertices()) if primitive.path else []
                    expected=dict(width=font.text_width(plain_text(d.text)),measurements=dict(zip(fm._fields,fm)),corners=[list(v) for v in vertices[:4]] if vertices else None)
                    cases.append(dict(font='mono' if font_name==fonts.MONOSPACE else 'ttf',text=spec,expected=expected));ms.delete_entity(e)
    plain=[dict(value=t,expected=plain_text(t)) for t in texts+['%','%%','%%?','^\n','^','^A^Z^@^?','%%C%%D%%P%%O%%U%%K','%%123','a\u001cb','^^^\n','A^\rB']]
    (OUT/'reference.json').write_text(json.dumps(dict(cases=cases,plain=plain),ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'{len(cases)} original font/alignment/OCS text extents and {len(plain)} decoding cases')

if __name__=='__main__':main()
