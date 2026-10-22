"""Original separate simple-MTEXT path: decoding, wrapping, matrices and glyphs."""
from pathlib import Path
from unittest.mock import patch
import copy,json,os,random,math
from generate_mtext_render_fixtures import fonts,ezdxf,manager_for,contours,ASSETS
from ezdxf.addons.drawing.text import simplified_text_chunks
from ezdxf.addons.drawing.unified_text_renderer import UnifiedTextRenderer
from ezdxf.addons.drawing.pipeline import prepare_string_for_rendering
from ezdxf.tools.text import fast_plain_mtext,estimate_mtext_content_extents
from ezdxf.fonts.shapefile import GlyphCache
from generate_mtext_fixtures import requests as parser_requests

OUT=Path(os.environ.get('SLEUFBASE_MTEXT_SIMPLE_FIXTURES',ASSETS/'mtext-simple'))
FONTS=ASSETS/'mtext-render'

def _evaluate(request,root=FONTS):
    m=manager_for(root)
    with patch.object(fonts,'font_manager',m):
        engine=UnifiedTextRenderer();face=fonts.FontFace(**request['face'])
        doc=ezdxf.new('R2018')
        a=dict(insert=request['insert'],char_height=request['height'],width=request['width'],line_spacing_factor=request['spacing'],
               attachment_point=request['align'],rotation=request['rotation'],extrusion=request['normal'])
        if request.get('direction') is not None:a['text_direction']=request['direction']
        e=doc.modelspace().add_mtext(request['content'],dxfattribs=a)
        chunks=list(simplified_text_chunks(e,engine,font_face=face))
        height=e.dxf.char_height if e.dxf.char_height>1e-9 else 2.5
        fm=engine.get_font_measurements(face,height);f=engine.get_font(face)
        lines=[]
        for line,t,h in chunks:
            ink=dict(kinds=[],points=[])
            if line.strip():ink=contours(engine.get_text_path(prepare_string_for_rendering(line,'MTEXT'),face,h).to_path().transform(t))
            rows=[[t[c,r] for c in range(4)] for r in range(3)]
            lines.append(dict(text=line,width=engine.get_text_line_width(line,face,h),matrix=rows,contours=ink))
        return dict(plain=fast_plain_mtext(e.text),measurements=dict(zip(fm._fields,fm)),selected=f.name,font_kind=type(f).__name__,lines=lines)

def evaluate(request,root=FONTS,corrected=True):
    raw=GlyphCache.get_advance_width
    def cached_control_advance(self,number):
        try:return raw(self,number)
        except IndexError:
            # Original get_shape() already stored the correct space fallback
            # for an empty control glyph, then get_advance_width reads .end
            # once more and crashes on the first occurrence. Retain that
            # cached fallback; no shape geometry or other widths are changed.
            assert number<32
            return self._advance_width_cache[number]
    try:
        with patch.object(GlyphCache,'get_advance_width',cached_control_advance if corrected else raw):
            return dict(output=_evaluate(request,root))
    except (IndexError,ValueError,OverflowError) as error:return dict(error=type(error).__name__)

def requests(seed=15735,count=96):
    names=['Regular.ttf','Bold.ttf','Italic.ttf','own.shp','own.shx','own.lff','curves.shx','curves.lff']
    texts=['',' ',r'Axp MØ %%c %%d %%p','A\\Px\\P\\Pp',r'{A} {x} {p}',
        '\nA\n\nB\n',' A  x\t p  ','A\x01B\x1fC\x00p','A\u00a0x\u2003p',
        r'\~A\~x',r'\\ \{literal\} %%z %%',r'\fX;A\P\H2;x',r'\S1/2; \Sx#y;',
        r'\C1unterminated',r'\Sunterminated',r'\Zunknown;Axp',r'^^^I^J^M^@^é^\nA',
        '   A verylongword x p','A \n\n\nx','\n\nA\n','\t\tA\t\tx\t']
    rng=random.Random(seed)
    pieces=['A','x','p','MØ','B',' ','  ','\t','\n',r'\P','\u001f','\u2003',r'%%c','word','longword']
    texts += [''.join(rng.choice(pieces) for _ in range(rng.randrange(1,25))) for _ in range(count)]
    for i,text in enumerate(texts):
        yield dict(content=text,face=dict(filename=names[i%len(names)],family='Fixture Styled',style='Regular',weight=400,width=5),
            height=[.25,1.,2.5,0.,1e-10][i%5],width=[0.,.2,2.5,5.,13.][i%5],spacing=[.25,1.,1.37,4.][i%4],
            align=i%9+1,insert=[168000.125+i*.013,401000.375-i*.017,1.5],rotation=[0.,27.,-90.,180.][i%4],
            normal=[[0,0,1],[0,0,-1],[1,2,3],[0,0,0],[1,0,0]][i%5],direction=([1.2,-.7,5.] if i%7==0 else None))
    m=manager_for(FONTS)
    with patch.object(fonts,'font_manager',m):
        for name in names:
            f=fonts.make_font(name,1.)
            for text in ['A x',' A x','B  x']:
                width=f.text_width(text)
                for w in [math.nextafter(width,-math.inf),width,math.nextafter(width,math.inf)]:
                    yield dict(content=text,face=dict(filename=name,family='Fixture Styled',style='Regular',weight=400,width=5),
                        height=1.,width=w,spacing=1.,align=1,insert=[0,0,0],rotation=0.,normal=[0,0,1],direction=None)

def estimate_requests():
    names=['Regular.ttf','Bold.ttf','Italic.ttf','own.shp','own.shx','own.lff','curves.shx','curves.lff','*monospace']
    texts=['',' ',r'\P\P',r'Axp\P\P MØ','A x p',r'\S1/2; \H2;A',r'\~A x','  Axp  ','\t\t']
    for i,name in enumerate(names):
        for n,text in enumerate(texts):
            for k,height in enumerate([.25,1.,2.5,0.]):
                yield dict(content=text,font=name,height=height,width=[0.,.37,3.,11.][(n+k)%4],spacing=[.25,1.,1.37,4.][(i+k)%4])

def estimate(request,root=FONTS):
    m=manager_for(root)
    with patch.object(fonts,'font_manager',m):
        f=fonts.make_font(request['font'],request['height'])
        w,h=estimate_mtext_content_extents(request['content'],f,request['width'],request['spacing'])
        return dict(width=w,height=h)

def main():
    OUT.mkdir(parents=True,exist_ok=True)
    plain=[]
    for r in parser_requests(random_count=256):
        if r['properties']:continue
        # Include every parser-shaped input, but use the actual fast decoder;
        # its deliberate permissive/unterminated behavior is a separate API.
        text=r['content'];plain.append(dict(input=text,output=fast_plain_mtext(text)))
    cases=[]
    for r in requests():
        result=evaluate(r);legacy=evaluate(r,corrected=False);case=dict(input=r,**result)
        if result!=legacy:case['legacy']=legacy
        cases.append(case)
    estimates=[dict(input=r,output=estimate(r)) for r in estimate_requests()]
    (OUT/'reference.json').write_text(json.dumps(dict(plain=plain,cases=cases,estimates=estimates),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(plain)} fast-decoder cases / {len(cases)} simple-MTEXT documents / {sum(len(c.get("output",{}).get("lines",[])) for c in cases)} wrapped lines / {sum("legacy" in c for c in cases)} explicit original first-control SHX crash corrections')
    print(f'{len(estimates)} original content extent estimates, including whitespace, zero height and monospace')

if __name__=='__main__':main()
