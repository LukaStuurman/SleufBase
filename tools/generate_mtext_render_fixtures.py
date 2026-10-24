"""Real original MTEXT renderer/fonts, isolated from app/settings/system fonts.

The corrected oracle changes only font selection: b1 requests Bold, and the
glyph backend uses the same face as measurement. Unmodified results are kept
alongside every changed case. Layout, glyph curves and transforms stay original.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import copy, json, os, random
with TemporaryDirectory(prefix='SleufBase-isolated-ezdxf-') as _import_home:
    _cache=Path(_import_home)/'ezdxf';_cache.mkdir()
    (_cache/'font_manager_cache.json').write_text('{"version":2,"font-faces":[]}',encoding='utf-8')
    # ezdxf itself loads a font cache during import. Supply an empty private
    # cache so even that import cannot scan installed fonts or write user data.
    with patch.dict(os.environ,{'XDG_CACHE_HOME':_import_home,'XDG_CONFIG_HOME':_import_home,
                               'EZDXF_CONFIG_FILE':str(Path(_import_home)/'empty.ini')}):
        import ezdxf
        from ezdxf.fonts import fonts
from ezdxf.fonts.font_manager import FontManager
from ezdxf.addons.drawing.mtext_complex import ComplexMTextRenderer
from ezdxf.addons.drawing.properties import Properties
from ezdxf.addons.drawing.unified_text_renderer import UnifiedTextRenderer
from ezdxf.addons.drawing.pipeline import prepare_string_for_rendering
from ezdxf.render.abstract_mtext_renderer import AbstractMTextRenderer
from ezdxf.tools import text_layout as tl
from ezdxf.math import Matrix44
from fontTools.ttLib import TTFont
from generate_mtext_fixtures import context,original
from generate_mtext_layout_fixtures import rect,cell_info,finite

ROOT=Path(__file__).resolve().parents[1]
ASSETS=ROOT/'rust/tests/fixtures'
OUT=Path(os.environ.get('SLEUFBASE_MTEXT_RENDER_FIXTURES',ASSETS/'mtext-render'))
IDENTITY=[[1.,0,0,0],[0,1.,0,0],[0,0,1.,0]]

def make_fonts(out):
    variants=[('Arial.ttf','Fixture Fallback','Regular',400,5,1),
        ('Regular.ttf','Fixture Styled','Regular',400,5,1),
        ('Bold.ttf','Fixture Styled','Bold',700,5,1.3),
        ('Italic.ttf','Fixture Styled','Italic',400,5,.8),
        ('BoldItalic.ttf','Fixture Styled','Bold Italic',700,5,1.5),
        ('Heavy.ttf','Fixture Styled','Heavy',900,5,1.7),
        ('Condensed.ttf','Fixture Styled','Condensed',400,3,.6),
        ('Expanded.ttf','Fixture Styled','Expanded',400,7,1.9),
        ('Oblique.ttf','Fixture Styled','Oblique',400,5,1.1),
        ('Book.ttf','Fixture Single','Book',400,5,.9),
        ('DejaVuSans.ttf','DejaVu Sans','Regular',400,5,1.2),
        ('DejaVuSerif.ttf','DejaVu Serif','Regular',400,5,1.4),
        ('DejaVuSansMono.ttf','DejaVu Sans Mono','Regular',400,5,1.6)]
    sources=[('own.shx',ASSETS/'stroke-font/fixture-1.0.shx'),
        ('own.shp',ASSETS/'stroke-font/fixture.shp'),('own.lff',ASSETS/'lff-font/polygons.lff'),
        ('curves.shx',ASSETS/'stroke-font/curves.shx'),('curves.lff',ASSETS/'lff-font/curves.lff')]
    for n,(name,family,style,weight,width,scale) in enumerate(variants):
        dest=out/'fonts'/f'{n:02}-{Path(name).stem}'/name;dest.parent.mkdir(parents=True,exist_ok=True)
        f=TTFont(ASSETS/'dxf-text-metrics/SleufBaseFixture.ttf');f.recalcTimestamp=False
        for record in f['name'].names:
            if record.nameID in [1,2,4,6]:
                s={1:family,2:style,4:family+' '+style,6:family.replace(' ','')+'-'+style.replace(' ','')}[record.nameID]
                record.string=s.encode(record.getEncoding())
        f['OS/2'].usWeightClass=weight;f['OS/2'].usWidthClass=width
        for glyph in f['glyf'].glyphs.values():
            glyph.expand(f['glyf'])
            if hasattr(glyph,'coordinates'):
                glyph.coordinates[:]=[(round(x*scale),y) for x,y in glyph.coordinates]
        for name_,(w,b) in list(f['hmtx'].metrics.items()):f['hmtx'].metrics[name_]=(round(w*scale),round(b*scale))
        f.save(dest)
    for n,(name,path) in enumerate(sources,len(variants)):
        dest=out/'fonts'/f'{n:02}-{name.replace(".","-")}'/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(path.read_bytes())

def manager_for(out):
    m=FontManager();m.build([str(p) for p in sorted((out/'fonts').iterdir())],support_dirs=False)
    fonts.TrueTypeFont._glyph_caches.clear()
    return m

def fixed_face(face):
    if not face.filename and face.weight>=700 and face.style.lower()=='regular':return face._replace(style='Bold')
    return face

def matrix(rows):
    return Matrix44([rows[r][c] if r<3 else (1 if c==3 else 0) for c in range(4) for r in range(4)])

def contours(path):
    """Combined original GlyphPath retains moves, curves and control vertices."""
    kinds=[];points=[]
    if len(path):
        kinds.append(0);points.append(list(path.start))
        for c in path:
            kinds.append(0 if c.type==4 else int(c.type))
            if c.type==2:points.append(list(c.ctrl))
            elif c.type==3:points.extend([list(c.ctrl1),list(c.ctrl2)])
            points.append(list(c.end))
    return dict(kinds=kinds,points=points)

class Backend:
    def __init__(self,corrected):self.draws=[];self.corrected=corrected;self.engine=UnifiedTextRenderer()
    def draw_text(self,text,transform,properties,cap_height):
        if not text.strip():return
        text=prepare_string_for_rendering(text,'TEXT')
        face=properties.font
        if self.corrected:face=fonts.font_manager.get_font_face(fonts.find_font_file_name(face))
        p=self.engine.get_text_path(text,face,cap_height).to_path().transform(transform)
        self.draws.append(dict(kind='text',id=properties._fixture_id,contours=contours(p)))
    def draw_line(self,start,end,properties):self.draws.append(dict(kind='line',id=properties._fixture_id,start=list(start),end=list(end)))
    def draw_filled_polygon(self,*args):raise AssertionError('background not enabled')

class Trace(tl.ContentRenderer):
    def __init__(self,renderer,id,ops):self.renderer=renderer;self.id=id;self.ops=ops
    def render(self,left,bottom,right,top,m=None):
        self.ops.append(dict(kind='text',id=self.id,rect=dict(left=left,bottom=bottom,right=right,top=top)))
        self.renderer.render(left,bottom,right,top,m)
    def line(self,x1,y1,x2,y2,m=None):
        self.ops.append(dict(kind='line',id=self.id,start=[x1,y1],end=[x2,y2]))
        self.renderer.line(x1,y1,x2,y2,m)

class Oracle(ComplexMTextRenderer):
    def __init__(self,request,corrected):
        AbstractMTextRenderer.__init__(self);self.request=request;self.ops=[];self.words=[]
        self._backend=Backend(corrected);self._properties=Properties();self._properties._fixture_id=0
    def make_mtext_context(self,e):return original(self.request['initial'])
    def make_bg_renderer(self,e):return None
    def resolve_aci_color(self,aci):return f'aci-{aci}'
    def word(self,text,ctx):
        result=super().word(text,ctx);id=len(self.words)+1;result.renderer.properties._fixture_id=id
        f=self.get_font(ctx)
        self.words.append(dict(text=text,context=context(ctx),selected=f.name,font_kind=type(f).__name__,width=f.text_width(text),space_width=f.space_width()))
        result.renderer=Trace(result.renderer,id,self.ops);return result
    def fraction(self,data,ctx):
        result=super().fraction(data,ctx)
        if isinstance(result,tl.Fraction):result.renderer=Trace(result.renderer,0,self.ops)
        return result

def evaluate(request,out,corrected=True):
    m=manager_for(out);raw=fonts.find_font_file_name
    selector=(lambda f:raw(fixed_face(f))) if corrected else raw
    r=copy.deepcopy(request)
    if r['initial']['cap_height']<=1e-9:r['initial']['cap_height']=2.5
    with patch.object(fonts,'font_manager',m),patch.object(fonts,'find_font_file_name',selector):
        try:
            oracle=Oracle(r,corrected);doc=ezdxf.new('R2018')
            e=doc.modelspace().add_mtext(r['content'],dxfattribs=dict(width=r['width'],char_height=r['initial']['cap_height'],line_spacing_factor=r['spacing']))
            cols=r.get('columns',[])
            if cols:
                # Only supply the already-resolved column specs. The complete
                # original adapter still parses and measures every token.
                # Use the real layout_engine for token/font->cell conversion;
                # column construction is captured and replaced with explicit
                # unequal/manual specs, already independently tested below.
                base=tl.Layout.append_column
                index=0
                def append(self,*args,**kwargs):
                    nonlocal index
                    if index==0:
                        for c in cols:base(self,c.get('width'),c.get('height'),gutter=c.get('gutter',0))
                    index+=1
                with patch.object(tl.Layout,'append_column',append):layout=oracle.layout_engine(e)
            else:layout=oracle.layout_engine(e)
            layout.place(*r['at'],align=r['align']);layout.render(matrix(r['matrix']))
            result=dict(words=oracle.words,draws=oracle._backend.draws,layout=dict(rect=rect(layout),current_column=layout.current_column_index,
                columns=[dict(rect=rect(c),used_height=c.used_content_height()) for c in layout],primitives=oracle.ops))
            return dict(output=result) if finite(result) else dict(error='non_finite')
        except (tl.LayoutError,ValueError,ZeroDivisionError,OverflowError) as exc:return dict(error=type(exc).__name__)

def requests(seed=4271,count=96):
    ctx=context(original(None));ctx['font_face']=dict(filename='Regular.ttf',family='Fixture Styled',style='Regular',weight=400,width=5)
    texts=['','  ',r'Axp MØ %%c ^J A',r'A\P\Px\P',r'\Lunder\l \Oover\o \Kstrike\k',
        r'\Llong word\Pnext\l',r'pre{\H.4x;\A2;small}post',r'\W.6;wide\Q17;slant\W1.7;both',
        r'\S1/2; \Supper#lower; \Sx^ y; \Splain;',r'a\~b c\tignored',
        'A\tx p\tA\tB',r'\pt2,c4,r6;A^Ix^Ip^IØ',r'\pi-1,l2,r.5,qj;many Axp words\Pnext paragraph',
        r'\pqc;center\P\pqr;right\P\pqd;distributed',r'\c197121;rgb\C5;aci \A2;\H2;big',
        r'\fFixture Styled|b0|i0;normal \fFixture Styled|b1|i0;bold \fFixture Styled|b0|i1;italic \fFixture Styled|b1|i1;both',
        r'\fown;Axp \fcurves;BC A',r'\H0;zero\H1;A',r'A\Nx\Nword',r'A\Xignored',
        r'\ptc2.50,r-1,3;first^Isecond^Ithird','A\x01x\x1fp',r'\L\Sx/y; next\l',
        r'\ptc٢.٥,r３;A^Ix^Ip',r'\ptc,r;bad',r'\fFixture Styled|b1|i0;Axp',
        r'\fFixture Styled|b0|i1;Axp']
    rng=random.Random(seed)
    fragments=['Axp','BC','MØ',r'\P',r'\~',r'\L',r'\l',r'\H.6x;',r'\A2;',r'\W.7;',r'\Q-13;',r'\S1/2;',r'\fFixture Styled|b1|i0;',r'\fFixture Styled|b0|i1;',' ','\t']
    texts += [''.join(rng.choice(fragments) for _ in range(rng.randrange(3,18))) for _ in range(count)]
    for i,text in enumerate(texts):
        initial=copy.deepcopy(ctx);initial['cap_height']=[.25,1.,2.5][i%3];initial['width_factor']=[.6,1.,1.4][i%3]
        initial['paragraph']['align']=[1,2,3,4,5][i%5]
        if i%7==0:initial['font_face']['filename']=['own.shp','own.shx','own.lff','curves.shx','curves.lff'][i%5]
        yield dict(content=text,initial=initial,width=[5.,13.,31.][i%3],spacing=[1.,.7,1.3][i%3],align=i%9+1,
            columns=([dict(width=9.,height=6.,gutter=1.5),dict(width=9.,height=None,gutter=1.5)] if i%4==3 else []),
            at=[i*.13,-i*.17],matrix=([[-.7,.3,0,168000.125],[.2,1.7,0,401000.375],[.1,-.2,1.,2.5]] if i%3 else IDENTITY))

def face_queries():
    queries=[]
    for family in ['Fixture Styled','Fixture Single','Fixture','missing','serif','sans-serif','monospace','own','curves']:
        for style in ['Regular','Bold','Italic','Oblique','no-match','']:
            for weight,width in [(400,5),(700,3),(900,7)]:queries.append(dict(filename='',family=family,style=style,weight=weight,width=width))
    queries.extend([dict(filename=n,family='missing',style='Wrong',weight=900,width=1) for n in ['BOLD.TTF','own.shp','own.shx','own.lff','absent.ttf']])
    return queries

def main():
    assert ezdxf.__version__=='1.4.4'
    make_fonts(OUT);m=manager_for(OUT)
    with patch.object(fonts,'font_manager',m):
        faces=[dict(input=f,selected=m.find_font_name(fonts.FontFace(**f))) for f in face_queries()]
    cases=[];changes=0
    for r in requests():
        fixed=evaluate(r,OUT);legacy=evaluate(r,OUT,False);case=dict(input=r,**fixed)
        if fixed!=legacy:case['legacy']=legacy;changes+=1
        cases.append(case)
    (OUT/'reference.json').write_text(json.dumps(dict(faces=faces,cases=cases),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(faces)} original face selections / {len(cases)} complex MTEXT cases / {changes} explicit font-selection corrections / {sum("error" in c for c in cases)} original errors')

if __name__=='__main__':main()
