"""Freeze original parser/coercion/rule behavior, without application startup."""
from pathlib import Path
import itertools
import json
import math
import unicodedata
import random
import struct
from generate_object_export_fixtures import reference

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/object-parser"
def main():
    assert unicodedata.unidata_version=="14.0.0"
    ref=reference();OUT.mkdir(parents=True,exist_ok=True)
    def feature(properties=None,coordinates=None,kind="Point"):
        return {"geometry":{"type":kind,"coordinates":[1,2,3] if coordinates is None else coordinates},"properties":properties or {}}
    anchor=feature({"groupType":"objects","idName":"anchor"},[10,20,4])
    cases=[]
    def add(name,candidate,correction=None):
        payload={"features":[candidate,anchor]}
        raw=json.dumps(payload,ensure_ascii=False,separators=(",",":"),allow_nan=False)
        try:
            points,polys=ref.parse_job_features_dataset_text(raw)
        except (AttributeError,TypeError):
            # Invalid truthy geometry containers crash before Python's type guard.
            points,polys=ref.parse_job_features_dataset_text(json.dumps({"features":[None,anchor]}))
            correction="Skip malformed geometry containers instead of crashing the complete job"
        expected=[]
        for f in [*points,*polys]:
            if isinstance(f,ref.KickTheMapObjectPoint):
                if not math.isfinite(f.x) or not math.isfinite(f.y):continue
                geometry={"Point":{"x":f.x,"y":f.y,"z":f.z if f.z is None or math.isfinite(f.z) else None}}
            else:
                vertices=[{"x":v.x,"y":v.y,"z":v.z if v.z is None or math.isfinite(v.z) else None} for v in f.vertices if math.isfinite(v.x) and math.isfinite(v.y)]
                if len(vertices)<2:continue
                geometry={"Polyline":vertices}
            expected.append({"object_name":f.object_name,"source_name":f.source_name,"geometry":geometry,"attributes":[f.attribute_1,f.attribute_2,f.attribute_3]})
        cases.append({"name":name,"input":raw,"expected":expected,"correction":correction})
    values=[None,False,True,0,0.0,-0.0,1,2.0,1e-5,1e16,1e20,"","  ","water","\u001cwater\u001f",[],{},["x",True,None,1.0],{"z":1,"a":["é","a'b",'a"b',"both'\"", "\x00\n\t\u200b"]}]
    for i,value in enumerate(values):
        for key in ("groupType","idName","name","properties1"):
            props={"groupType":"objects","name":"water","properties2":"x","properties3":"y"}
            if key=="name":props.pop("name")
            props[key]=value
            add(f"{key}-{i}",feature(props))
    for i,value in enumerate(["\u00a0١٢.٣_٤\u0085","1_000","1_0.0_5","+1.2e-3",".5","1.",True,False,"1__0","_1","1_","1_.0","1e_2","0x10","−1","\u001c1","1,2","nan","Infinity","1e9999"]):
        add(f"coordinate-{i}",feature({"idName":"water"},[value,2,value]),"Nonfinite XY is omitted and nonfinite optional Z becomes unknown" if str(value).lower()in("nan","infinity","1e9999") else None)
    add("nonfinite-z",feature({"idName":"water"},[1,2,"nan"]),"Nonfinite optional Z becomes unknown")
    add("mixed-polyline",feature({"idName":"LS"},[[1,2,3],["٣","4_0","bad"],None,[True,False],[],[5],["bad",2],["inf",0]]," LineString "),"Invalid/nonfinite XY vertices are omitted")
    for i,geometry in enumerate([None,[],[1],"point",True,0,{}]):add(f"geometry-{i}",{"geometry":geometry})
    negative={"features":[feature({"name":"RAW_NEGATIVE_ZERO","properties1":["RAW_NEGATIVE_ZERO",-0.0]}),anchor]}
    raw=json.dumps(negative,separators=(",",":")).replace('"RAW_NEGATIVE_ZERO"','-0')
    points,_=ref.parse_job_features_dataset_text(raw)
    cases.append({"name":"integer-negative-zero","input":raw,"expected":[{"object_name":f.object_name,"source_name":f.source_name,"geometry":{"Point":{"x":f.x,"y":f.y,"z":f.z}},"attributes":[f.attribute_1,f.attribute_2,f.attribute_3]} for f in points],"correction":None})
    rng=random.Random(71042);values=[]
    while len(values)<1024:
        value=struct.unpack('>d',rng.getrandbits(64).to_bytes(8,'big'))[0]
        if math.isfinite(value):values.append(value)
    payload={"features":[feature({"name":v,"properties1":[v,True,None]}) for v in values]}
    raw=json.dumps(payload,separators=(",",":"));points,_=ref.parse_job_features_dataset_text(raw)
    cases.append({"name":"float-property-representations","input":raw,"expected":[{"object_name":f.object_name,"source_name":f.source_name,"geometry":{"Point":{"x":f.x,"y":f.y,"z":f.z}},"attributes":[f.attribute_1,f.attribute_2,f.attribute_3]} for f in points],"correction":None})
    (OUT/"parser.json").write_text(json.dumps(cases,ensure_ascii=False,separators=(",",":"))+"\n",encoding="utf-8",newline="\n")
    names=["","a","b","-","!","[","]","aa","ab","a-b","é","中","a\nb"," mſ ","straße","SS","StraSSe","\u001cLS\u001f"]
    patterns=["".join(p) for n in range(1,5) for p in itertools.product("a?-![]*",repeat=n)]
    patterns += ["*["+"".join(p)+"]" for n in range(7) for p in itertools.product("ab-!",repeat=n)]
    patterns += ["*[z-a]","*[!z-a]","*[]a]","*[a--c]","*[z-a-b]","*[--a]","?[\\]","[*]","[?]","*[^a]","*[a&&b]","*[a~~b]","*[a||b]","?","??","*é*","*中?","stra?e","*ß*","mſ","*L[!A-Z]"," LS ","*"*1500+"é"]
    rules=[]
    for pattern in patterns:
        rule=ref.ObjectLayerRule((pattern,),"TARGET",5)
        rules.append({"pattern":pattern,"matches":[rule.matches(name) for name in names]})
    (OUT/"rules.json").write_text(json.dumps({"names":names,"rules":rules},ensure_ascii=False,separators=(",",":"))+"\n",encoding="utf-8",newline="\n")
    ranges=[]
    for n in range(0x110000):
        if chr(n).isprintable():
            if ranges and ranges[-1][1]==n-1:ranges[-1][1]=n
            else:ranges.append([n,n])
    text="// Python 3.11 / Unicode 14 str.isprintable for lossless JSON-property repr.\n// Generated by tools/generate_object_parser_fixtures.py.\n"
    text+="pub const PRINTABLE: &[(u32,u32)] = &[\n"+"".join(f"({hex(a)},{hex(b)}),\n" for a,b in ranges)+"];\n"
    (ROOT/"rust/src/python_repr_unicode.rs").write_text(text,encoding="utf-8",newline="\n")
    print(f"Generated {len(cases)} parser cases and {len(rules)*len(names)} original rule decisions")
if __name__=="__main__":main()
