"""Original MTEXT parser/context oracle, without the application or font scan."""
from pathlib import Path
import json
import math
import os
import random
import unicodedata

import ezdxf
from ezdxf.fonts.font_face import FontFace
from ezdxf.tools.text import MTextContext,MTextParser,ParagraphProperties

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_MTEXT_FIXTURES',ROOT/'rust/tests/fixtures/mtext'))


def context(ctx):
    return dict(stroke=ctx._stroke,continue_stroke=ctx.continue_stroke,aci=ctx.aci,
        rgb=list(ctx.rgb) if ctx.rgb is not None else None,align=int(ctx.align),
        font_face=dict(ctx.font_face._asdict()),cap_height=ctx.cap_height,width_factor=ctx.width_factor,
        char_tracking_factor=ctx.char_tracking_factor,oblique=ctx.oblique,
        paragraph=dict(indent=ctx.paragraph.indent,left=ctx.paragraph.left,right=ctx.paragraph.right,
            align=int(ctx.paragraph.align),tab_stops=list(ctx.paragraph.tab_stops)))


def original(initial):
    ctx=MTextContext()
    if initial is None:return ctx
    for name,value in initial.items():
        if name=='stroke':ctx._stroke=value
        elif name=='font_face':ctx.font_face=FontFace(**value)
        elif name=='paragraph':ctx.paragraph=ParagraphProperties(**{**value,'tab_stops':tuple(value['tab_stops'])})
        else:setattr(ctx,name,value)
    return ctx


def evaluate(request):
    parser=MTextParser(request['content'],original(request.get('initial')),yield_property_commands=request['properties'])
    try:
        result=[]
        for t in parser:
            data=dict(kind=t.type.name.lower())
            if t.data is not None:data['data']=t.data
            result.append(dict(content=data,context=context(t.ctx)))
        # The native parser explicitly rejects overflow, including transitions
        # that would leave no emitted token. Preserve the original evidence.
        states=[context(parser.ctx)]+[t['context'] for t in result]
        if any(not math.isfinite(s[k]) for s in states for k in ['cap_height','width_factor','char_tracking_factor','oblique']):
            return dict(guard='non_finite',original='non-finite context produced')
        return dict(tokens=result)
    except (TypeError,ValueError,OverflowError) as error:
        return dict(error=type(error).__name__)


def requests(seed=8713,random_count=128):
    values=['','hello world','  spaces  ','one\n\ntwo\tthree\rnext','a\x00b\x1fc',
        r'\P\N\X\~',r'a\P\Pb\Nc\Xd\~e',r'\\\{literal\} {nested{deep}} tail',
        '}',r'{outer{inner}outer}','abc'+chr(92),chr(92),r'\{x\}',r'\Zword\z;unknown\U+00D8',
        '%%c %%C %%d %%D %%p %%P %%u %%o %%k %%z %%',
        '^I^J^M^@^ ^a^é^^^', '^\n^J',r'\Lone\ltwo\Othree\ofour\Kfive\ksix',
        r'{\Linside}\H2;outside\l end',r'\L{\l inside}outside\H2;\Knext\k',
        r'\fArial|b1|i1|c0|p34;ABC\FArial|b0|i0;XYZ',r'\f|b1;not changed',
        r'\fA|b10|i10;prefix\fB|i0|b1;bold\fC;reset',r'\funterminated',
        r'\S1/2; \Supper#lower; \S+1^ -2; \Splain;',r'\Sone\/two/three;\Send\;still/next;',
        r'\S; \S/; \S#/; \S1/2/3;', '\\Strailling\\',r'\S1/2',r'\Sx^Jz/y^Iz;',
        r'\pi-2,l3,r.5,qc,t1,c2.50,r-3,4;first\Psecond',r'\pxi*,l*,r*,q*,t;reset',
        r'\pt2,c3,r4,i5,l6;tail',r'\ptc,r,,..5,z5;bad stops',r'\pi1e2,l-.2,r+3,qj;justify',
        r'\pqr;right\P\pqd;distributed\P\pql;left\P\pqz;default',r'\pt1E-2,c.5,r+02.0;tabs',
        r'\c255;red\C7;reset\c197121;rgb\C999;unchanged',r'\C256;block\C0;layer',
        r'\c16777216;wrap\c4294967295;white',r'\C-2negative\c+3plus',
        r'\H1e309;x',r'\H1e308;\H1e308x;x',r'\Q-1e999;x',
        '\\C'+'9'*4301+';x','\\c'+'1'*4301+';x']
    for code in ['A','C','c','H','W','T','Q']:
        for expr in ['',';','0;','1','2;','3;','-2.5x;','+.5;','.5x;','1e-2;','1e;','1E+2X;','bad;','000007;']:
            values.append(f'pre\\{code}{expr}post')
    values.extend(chr(92)+code for code in ['A','C','c','H','W','T','Q','p','f','F','S','L','l','O','o','K','k','Z'])
    zeros=[n for n in range(0x110000) if unicodedata.category(chr(n))=='Nd' and unicodedata.decimal(chr(n))==0]
    for zero in zeros:
        ds=''.join(chr(zero+n) for n in [1,2,3])
        values.extend([f'\\H{ds}.5x;Unicode',f'\\C{ds};color\\c{ds};rgb',f'\\pi-{ds},tc{ds}.2;tabs'])
    rng=random.Random(seed)
    fragments=[r'\P',r'\~',r'\N',r'\X',r'\L',r'\l',r'\O',r'\o',r'\K',r'\k',
        r'\H2.5x;',r'\W.7;',r'\T1.2x;',r'\Q-17;',r'\A2;',r'\A0;',r'\C5;',r'\c197121;',
        r'\pql;',r'\pxi-1,l1,r0,t2,c4;',r'\fFixture|b1|i1;',r'\S1/2;',r'\Sx^ y;',
        r'\Zx',r'\{',r'\}',r'\\','%%c','AØx','word',' ','\t','\n','{','}']
    values.extend(''.join(rng.choice(fragments) for _ in range(rng.randint(1,20))) for _ in range(random_count))
    custom=MTextContext();custom.cap_height=2.5;custom.width_factor=.8;custom.char_tracking_factor=1.25
    custom.rgb=(17,34,51);custom.align=1;custom._stroke=3;custom.font_face=FontFace('fixture.ttf','Fixture','Italic',700,3)
    custom.paragraph=ParagraphProperties(1,-2,.5,3,(1,'c2.5','r3'))
    for n,text in enumerate(values):
        for properties in [False,True]:
            yield dict(content=text,initial=context(custom) if n%3==0 else None,properties=properties)


def main():
    assert ezdxf.__version__=='1.4.4' and unicodedata.unidata_version=='14.0.0'
    OUT.mkdir(parents=True,exist_ok=True)
    cases=[dict(input=r,**evaluate(r)) for r in requests()]
    (OUT/'parser.json').write_text(json.dumps(dict(cases=cases),ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    print(f'{len(cases)} original MTEXT parser cases / {sum(len(c.get("tokens",[])) for c in cases)} tokens / {sum("error" in c or "guard" in c for c in cases)} explicit errors/guards')


if __name__=='__main__':main()
