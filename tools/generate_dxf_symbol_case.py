"""Freeze Python 3.11/Unicode 14 lower(), including final-sigma context.

ezdxf table keys use lower(), which deliberately differs from casefold().
Infer the case-context properties directly from Python's own lower operation.
"""
from pathlib import Path
import unicodedata

ROOT=Path(__file__).resolve().parents[1]


def ranges(values):
    out=[]
    for value in values:
        if out and value==out[-1][1]+1:out[-1][1]=value
        else:out.append([value,value])
    return out


def main():
    assert unicodedata.unidata_version=="14.0.0"
    lower=[];cased=[];ignorable=[]
    for code in range(0x110000):
        if 0xD800<=code<=0xDFFF:continue
        ch=chr(code);text=ch.lower()
        if text!=ch:lower.append((code,text))
        is_cased=ch.isupper() or ch.islower() or unicodedata.category(ch)=="Lt"
        if is_cased:cased.append(code)
        probe=("." if is_cased else "A")+ch+"Σ"
        if probe.lower()[-1]==("σ" if is_cased else "ς"):ignorable.append(code)
    text="// Generated from Python 3.11 / Unicode 14; do not edit.\n"
    text+="const LOWER: &[(char,&str)] = &[\n"
    for code,value in lower:
        escaped="".join("\\u{"+format(ord(c),"x")+"}" for c in value)
        text+=f"    ('\\u{{{code:x}}}',\"{escaped}\"),\n"
    text+="];\n"
    for name,values in [("CASED",cased),("CASE_IGNORABLE",ignorable)]:
        text+=f"const {name}: &[(u32,u32)] = &[\n"
        for a,b in ranges(values):text+=f"    (0x{a:x},0x{b:x}),\n"
        text+="];\n"
    (ROOT / "rust/src/dxf_symbol_unicode.rs").write_text(text,encoding="utf-8",newline="\n")
    print(len(lower),"lower mappings;",len(ranges(cased)),"cased and",len(ranges(ignorable)),"case-ignorable ranges")


if __name__=="__main__":main()
