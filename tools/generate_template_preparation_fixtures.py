"""Original style/layer preparation, including repeated pending-resource edits."""
from pathlib import Path
from types import SimpleNamespace
import importlib
import io
import json
import unicodedata
import ezdxf
from ezdxf.lldxf.validator import make_table_key
from generate_object_export_fixtures import reference
from generate_template_geometry_fixtures import signature

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT / "rust/tests/fixtures/template-preparation"


def snapshot(doc,entities=True):
    layers={}
    for e in doc.layers:
        d=e.dxf
        layers[d.name]=dict(color=d.color,lineweight=d.lineweight,flags=d.flags,linetype=d.linetype,plot=d.plot,true_color=d.get("true_color"),material=d.get("material_handle"),plotstyle=d.get("plotstyle_handle"))
    styles={e.dxf.name:{name:getattr(e.dxf,name) for name in ["flags","height","width","oblique","generation_flags","last_height","font","bigfont"]} for e in doc.styles}
    result=dict(layers=layers,styles=styles)
    if entities:result["entities"]=[signature(e) for e in doc.modelspace()]
    return result


def main():
    assert ezdxf.__version__=="1.4.4" and unicodedata.unidata_version=="14.0.0"
    ref=reference();module=importlib.import_module(ref.__package__+".cadastral_export");exporter=module.CadastralDxfExporter(wfs_client=None)
    active=importlib.import_module(ref.__package__+".template_fill_performance_patch")._configure_profile_layers_once
    ezdxf.options.write_fixed_meta_data_for_testing=True;OUT.mkdir(parents=True,exist_ok=True)
    def make(name,custom=False,missing=False):
        doc=ezdxf.new("R2018");doc.appids.add("KEEP")
        if custom:
            doc.styles.add("PROEFSLEUVEN_LIBMONO",font="Custom Font.ttf",dxfattribs=dict(height=1.8,width=.8,oblique=5,last_height=2.3,generation_flags=2))
            for layer,color in [("Water",3),("Straße",1),("STRASSE",2),("ΟΣ",4),("οσ",5),("X-XX-AL-PROFIELBALK-SD",-2)]:
                e=doc.layers.add(layer,color=abs(color),lineweight=35,true_color=0x123456,plot=False,dxfattribs=dict(flags=5));e.dxf.color=color;e.set_xdata("KEEP",[(1000,"Behouden Ø 漢😀"),(1070,3)])
        for block_name,tag,x in [("Straße","A",1),("STRASSE","B",2),("ΟΣ","C",3),("οσ","D",4)]:
            block=doc.blocks.new(block_name);block.add_attdef(tag,(x,2),text="default "+tag,dxfattribs={"height":.2*x})
        if missing:doc.layers.add("Tail",color=7)
        doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
        path=OUT / (name+".dxf");doc.saveas(path);path.write_bytes(path.read_bytes().replace(b"\r\n",b"\n"))
        if missing:
            from verify_native_template_edits import records
            out=[]
            for r in records(path):
                if r[0]==(0,"LAYER") and (2,"Tail") in r:r=[(c,v) for c,v in r if c not in {62,370}]
                out.extend(r)
            path.write_text("".join(f"{c}\n{v}\n" for c,v in out),encoding="utf-8",newline="\n")
        return path
    def prep(*pairs):return dict(kind="prepare",points=[dict(name=n,color=c) for n,c in pairs])
    def geometry(layer):
        return dict(kind="geometry",layers=[dict(name=layer,color=5,true_color=0x445566,lineweight=None)],entities=[dict(layer=layer,layout=None,paint={},shape=dict(Circle=dict(center=[0,1],radius=1)))])
    cases=[dict(name="custom",path=make("custom",True),steps=[prep(("Water",5),("Data",3),("Straße",4),("STRASSE",6),("ΟΣ",2),("οσ",1),("X-XX-AL-PROFIELBALK-SD",4),("0",255)),prep(("Water",3),("Data",5),("Another",8)),prep(("Data",3),("water",1)),prep()]),dict(name="missing",path=make("missing",missing=True),steps=[prep(("Tail",7),("Data",3)),prep(("Tail",3),("Data",5),("Later",2)),prep(("Tail",7),("Data",3))]),dict(name="staged",path=make("empty"),steps=[geometry("Early"),prep(("Early",3)),geometry("Late"),prep(("early",5),("Late",1)),prep(("Early",3)),dict(kind="geometry",layers=[],entities=[dict(layer="Early",layout=None,paint={},shape=dict(Text=dict(text="Voorbereid Ø 漢😀",insert=[3,4],height=.2,rotation=0,style="KADASTRALE_LABELS",halign=0,valign=0))),*[dict(layer="0",layout=None,paint={},shape=dict(Block=dict(name=n,insert=[10,20],scale=1,attributes={t:"Native "+t}))) for n,t in [("Straße","A"),("STRASSE","B"),("ΟΣ","C"),("οσ","D")]]])])]
    cases.append(dict(name="shipped",path=ROOT / "assets/cadastral_template.dxf",compare_entities=False,steps=[prep(("Native template layer",3),("X-XX-AL-PROFIELBALK-SD",2)),prep(("Native template layer",5))]))
    for case in cases:
        doc=ezdxf.readfile(case["path"])
        for step in case["steps"]:
            if step["kind"]=="prepare":
                exporter._setup_text_styles(doc);exporter._ensure_template_profile_layers(doc,[SimpleNamespace(layer_name=p["name"],color=p["color"]) for p in step["points"]])
                before=snapshot(doc,False)
                active(exporter,doc,tuple(SimpleNamespace(layer_name=p["name"],color=p["color"]) for p in step["points"]))
                assert snapshot(doc,False)==before,"Active cached helper differs from original layer semantics"
            else:
                for layer in step["layers"]:
                    if layer["name"] not in doc.layers:
                        attrs={k:v for k,v in layer.items() if k not in {"name","color"} and v is not None};e=doc.layers.add(layer["name"],color=layer["color"],dxfattribs=attrs)
                        # The generic primitive writer permits these optional
                        # refs to be absent; preparation must preserve that.
                        e.dxf.discard("material_handle");e.dxf.discard("plotstyle_handle")
                        if layer.get("lineweight") is None:e.dxf.discard("lineweight")
                for request in step["entities"]:
                    kind,shape=next(iter(request["shape"].items()));target=doc.modelspace();attrs=dict(layer=request["layer"],**request["paint"])
                    if kind=="Circle":target.add_circle(shape["center"],shape["radius"],dxfattribs=attrs)
                    elif kind=="Text":target.add_text(shape["text"],dxfattribs=dict(attrs,insert=shape["insert"],height=shape["height"],rotation=shape["rotation"],style=shape["style"],halign=shape["halign"],valign=shape["valign"],align_point=shape["insert"]))
                    else:exporter._insert_template_block(target,shape["name"],shape["insert"],request["layer"],attributes=shape["attributes"],scale=shape["scale"])
            # Compare real CAD readback: ezdxf fills absent optional material /
            # plotstyle refs on load. Do not mutate the ongoing reference doc.
            stream=io.StringIO();doc.write(stream)
            step["expected"]=snapshot(ezdxf.read(io.StringIO(stream.getvalue())),case.get("compare_entities",True))
        case["path"]=case["path"].relative_to(ROOT).as_posix()
    (OUT / "reference.json").write_text(json.dumps(cases,ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    h=0xCBF29CE484222325;seed=73665;count=0
    chars=[chr(i) for i in range(0x110000) if not 0xD800<=i<=0xDFFF and chr(i).lower()!=chr(i)]
    for _ in range(20000):
        seed=(seed*6364136223846793005+1)&0xFFFFFFFFFFFFFFFF;code=(seed>>32)%0x110000
        if not 0xD800<=code<=0xDFFF:chars.append(chr(code))
    for ch in chars:
        for text in [ch,"A"+ch+"Σ","."+ch+"Σ","AΣ"+ch,"AΣ"+ch+"A","ΟΣ"+ch]:
            for byte in make_table_key(text).encode("utf-8")+b"\xff":h=((h^byte)*0x100000001B3)&0xFFFFFFFFFFFFFFFF
            count+=1
    (OUT / "symbol-case.json").write_text(json.dumps(dict(hash=f"{h:016x}",count=count),indent=2)+"\n",encoding="utf-8",newline="\n")
    print(len(cases),"preparation sequences;",sum(len(c["steps"]) for c in cases),"states;",count,"official symbol-key probes")


if __name__=="__main__":main()
