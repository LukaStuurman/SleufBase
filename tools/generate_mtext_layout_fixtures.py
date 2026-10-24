"""Measured-cell layout oracle from the original complete ezdxf layout engine.

No application initialization, preferences, accounts or installed font scan.
"""
from pathlib import Path
from unittest.mock import patch
import copy
import json
import math
import os
import random

import ezdxf
from ezdxf.tools import text_layout as tl

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_MTEXT_LAYOUT_FIXTURES',ROOT/'rust/tests/fixtures/mtext-layout'))


class Recorder(tl.ContentRenderer):
    def __init__(self,id,kind,ops):self.id=id;self.kind=kind;self.ops=ops
    def render(self,left,bottom,right,top,m=None):
        assert m is None
        self.ops.append(dict(kind=self.kind,id=self.id,rect=dict(left=left,bottom=bottom,right=right,top=top)))
    def line(self,x1,y1,x2,y2,m=None):
        assert m is None
        self.ops.append(dict(kind='line',id=self.id,start=[x1,y1],end=[x2,y2]))


def cell(spec,ops):
    kind=spec['kind']
    if kind in ['space','nbsp','tab']:
        cls={'space':tl.Space,'nbsp':tl.NonBreakingSpace,'tab':tl.Tabulator}[kind]
        return cls(spec['width'],spec.get('min'),spec.get('max'))
    if kind=='text':return tl.Text(spec['width'],spec['height'],valign=spec.get('valign',0),stroke=spec.get('stroke',0),renderer=Recorder(spec['id'],'text',ops))
    if kind=='fraction':return tl.Fraction(cell(spec['top'],ops),cell(spec['bottom'],ops),stacking=spec.get('stacking',0),valign=spec.get('valign',0),renderer=Recorder(spec['id'],'unused',ops))
    raise AssertionError(kind)


def rect(box):
    x,y=box.final_location()
    return dict(left=x,bottom=y-box.total_height,right=x+box.total_width,top=y)


def cell_info(c,locked=False,offset=0):
    if isinstance(c,tl.RigidConnection):kind='rigid';children=list(c)
    elif isinstance(c,tl.Fraction):kind='fraction';children=[c._top_content,c._bottom_content]
    else:
        kind={tl.Text:'text',tl.Space:'space',tl.NonBreakingSpace:'nbsp',tl.Tabulator:'tab'}[type(c)];children=[]
    return dict(kind=kind,rect=rect(c),locked=locked,offset=offset,children=[cell_info(c) for c in children])


def finite(v):
    if isinstance(v,dict):return all(finite(x) for x in v.values())
    if isinstance(v,(tuple,list)):return all(finite(x) for x in v)
    if isinstance(v,float):return math.isfinite(v)
    return True


def evaluate(request):
    ops=[]
    def background(spec):
        return None if spec.get('frame') is None else Recorder(spec['frame'],'background',ops)
    def keep_source(self,cells,first):
        result=old_split(self,cells,first);result._fixture_source=self._fixture_source;return result
    old_split=tl.Paragraph._new_paragraph
    try:
        layout=tl.Layout(request['width'],request.get('height'),request.get('margins'),background(request))
        for c in request['columns']:
            layout.append_column(c.get('width'),c.get('height'),gutter=c.get('gutter',0),margins=c.get('margins'),renderer=background(c))
        # Only carry a diagnostic source id into split paragraphs. No layout
        # arithmetic, cell choices or renderer methods are replaced.
        with patch.object(tl.Paragraph,'_new_paragraph',keep_source):
            for source,event in enumerate(request['events']):
                kind=event['kind']
                if kind=='next_column':layout.next_column();continue
                if kind=='empty':p=tl.EmptyParagraph(event['height'],event.get('spacing',1))
                else:
                    p=tl.Paragraph(align=event.get('align',1),indent=tuple(event.get('indent',[0,0,0])),line_spacing=event.get('spacing',1),margins=event.get('margins'),tab_stops=[tl.TabStop(t['pos'],t.get('kind',0)) for t in event.get('tabs',[])],renderer=background(event))
                    p.append_content([cell(s,ops) for s in event['cells']])
                p._fixture_source=source;layout.append_paragraphs([p])
        layout.place(*request.get('at',[0,0]),align=request.get('align',1));layout.render()
        columns=[]
        for col in layout:
            paragraphs=[]
            for p in col:
                lines=[dict(rect=rect(line),available_width=line.line_width,cells=[cell_info(e.cell,e.locked,e.offset) for e in line._cells]) for line in getattr(p,'_lines',[])]
                paragraphs.append(dict(source=p._fixture_source,empty=isinstance(p,tl.EmptyParagraph),rect=rect(p),distance=p.distance_to_next_paragraph,lines=lines))
            columns.append(dict(rect=rect(col),used_height=col.used_content_height(),paragraphs=paragraphs))
        result=dict(rect=rect(layout),current_column=layout.current_column_index,columns=columns,primitives=ops)
        return dict(output=result) if finite(result) else dict(guard='non_finite')
    except (tl.LayoutError,ValueError,ZeroDivisionError,OverflowError) as error:return dict(error=type(error).__name__)


def text(id,w,h=1,**kw):return dict(kind='text',id=id,width=w,height=h,**kw)
def glue(kind='space',width=1,**kw):return dict(kind=kind,width=width,**kw)
def paragraph(cells,**kw):return dict(kind='paragraph',cells=cells,**kw)
def request(events,**kw):return dict(width=10,columns=[{}],events=events,**kw)
def repeated(cells,count):
    result=[]
    for n in range(count):
        if n:result.append(glue(width=.25))
        result.extend(copy.deepcopy(cells))
    return result


def requests(seed=73241,random_count=200):
    body=[text(1,3,1,stroke=9),glue(width=.75),text(2,4,2,valign=1,stroke=15),glue(width=.5),text(3,5,.5,valign=2)]
    for align in range(1,5):
        for anchor in range(1,10):
            yield request([paragraph(copy.deepcopy(body),align=align,indent=[-.5,1,.25],spacing=1.3,margins=[.1,.2,.3,.4],frame=200),dict(kind='empty',height=1.2,spacing=.8),paragraph(copy.deepcopy(body),align=align)],at=[168000.25,401000.75],align=anchor,frame=100)
    for align in range(1,5):
        yield request([paragraph([text(1,3),glue(width=1),text(2,6)],align=align)])
        for width in [0,.25,1,3,10,10.00001,50]:
            r=request([paragraph([text(1,width),glue(width=.5),text(2,1)],align=align)]);yield r
    for stacking in [0,1,2,3]:
        for valign in [0,1,2]:
            fraction=dict(kind='fraction',id=11,top=text(12,2,.7,stroke=15),bottom=text(13,3,1.3),stacking=stacking,valign=valign)
            nested=dict(kind='fraction',id=20,top=copy.deepcopy(fraction),bottom=text(21,.5,.25),stacking=2,valign=valign)
            yield request([paragraph([text(1,1,.5,stroke=9),glue('nbsp',.25),fraction,glue(width=.5),nested])])
    for align in range(1,5):
        for tabs in [[],[dict(pos=3,kind=0),dict(pos=6,kind=1),dict(pos=8,kind=2)],[dict(pos=8,kind=2),dict(pos=3,kind=0),dict(pos=6,kind=1)], [dict(pos=3,kind=0),dict(pos=3,kind=1),dict(pos=3,kind=2)],[dict(pos=-1),dict(pos=0),dict(pos=10),dict(pos=12)]]:
            yield request([paragraph([text(1,1,stroke=9),glue('tab',.75),text(2,2,stroke=15),glue('tab',.5),text(3,3),glue(width=1),text(4,2)],align=align,tabs=tabs)])
            yield request([paragraph([glue('tab',2),glue('tab',2),text(1,12),glue(width=1),text(2,1)],align=align,tabs=tabs)])
    for cells in [[],[glue('nbsp',2)],[glue('tab',2)],[glue(width=1),text(1,20)], [text(1,2),text(2,3)], [text(1,2),glue('nbsp',1),glue('nbsp',2),text(2,3)], [glue('nbsp',1),text(1,2),glue('nbsp',1),glue('space',1),text(2,3)], [text(1,2),glue(width=1),glue(width=2)]]:
        yield request([paragraph(cells,align=4)])
    for height in [None,0,.5,1,2,3.334,8]:
        for align in range(1,5):
            r=request([paragraph(repeated(body,3),align=align,margins=[.1,.2],spacing=.8),dict(kind='empty',height=.75),paragraph(copy.deepcopy(body),align=align)])
            r['columns']=[dict(width=7,height=height,gutter=1,margins=[.2,.3],frame=300),dict(width=11,height=height,gutter=2,frame=301)];yield r
    yield request([paragraph(copy.deepcopy(body)),dict(kind='next_column'),paragraph(repeated(body,2)),dict(kind='next_column'),paragraph([text(100,2,.5)])])
    yield request([dict(kind='next_column'),paragraph([text(1,1)])])
    r=request([dict(kind='empty',height=2),dict(kind='empty',height=2),paragraph([text(1,1,.5)])]);r['columns']=[dict(height=1,frame=301)];yield r
    for n in [0,1,2,3,4,5]:yield request([paragraph(copy.deepcopy(body),margins=[.1,.2,.3,.4,.5][:n])],margins=[.2]*n)
    for align in range(1,5):
        yield request([paragraph([text(1,1),glue(width=1,min=.5,max=1.25),text(2,1),glue('nbsp',.1,min=.1,max=.5),text(3,1),glue(width=1,min=0,max=3),text(4,1),glue(width=.2,max=.8),text(5,6)],align=align)])
    rng=random.Random(seed)
    for case in range(random_count):
        next_id=1
        def word(depth=0):
            nonlocal next_id
            id=next_id;next_id+=1
            if depth<2 and rng.random()<.15:return dict(kind='fraction',id=id,top=word(depth+1),bottom=word(depth+1),stacking=rng.randrange(3),valign=rng.randrange(3))
            return text(id,rng.choice([0,.25,1,2,3.5,6,12]),rng.choice([.5,1,1.5,2.5]),stroke=rng.choice([0,1,2,4,9,15]),valign=rng.randrange(3))
        events=[]
        for n in range(rng.randint(1,5)):
            cells=[]
            for i in range(rng.randint(1,12)):
                if cells:cells.append(glue(rng.choice(['space','space','nbsp','tab']),rng.choice([0,.25,.5,1,2])))
                cells.append(word())
            if rng.random()<.25:cells.insert(0,glue(width=rng.choice([.5,2,20])))
            events.append(paragraph(cells,align=rng.randint(1,4),indent=[rng.choice([-1,0,1]),rng.choice([0,.5]),rng.choice([0,.5,1])],spacing=rng.choice([.5,1,1.2,2]),tabs=[dict(pos=p,kind=rng.randrange(3)) for p in rng.sample([1,2,4,6,8],3)],frame=500+n))
            if rng.random()<.2:events.append(dict(kind='empty',height=.75,spacing=1.3))
        r=request(events,align=rng.randint(1,9),at=[rng.uniform(-3,3),rng.uniform(-3,3)])
        r['columns']=[dict(width=rng.choice([5,8,10,15]),height=rng.choice([None,3,6,15]),gutter=rng.choice([0,.5,1]),margins=[.1,.2],frame=400)]
        yield r


def main():
    assert ezdxf.__version__=='1.4.4'
    OUT.mkdir(parents=True,exist_ok=True);cases=[dict(input=r,**evaluate(r)) for r in requests()]
    (OUT/'layout.json').write_text(json.dumps(dict(cases=cases),ensure_ascii=False,separators=(',',':'),allow_nan=False)+'\n',encoding='utf-8')
    print(f'{len(cases)} original layout cases / {sum(len(c.get("output",{}).get("primitives",[])) for c in cases)} primitives / {sum("error" in c or "guard" in c for c in cases)} errors or guards')


if __name__=='__main__':main()
