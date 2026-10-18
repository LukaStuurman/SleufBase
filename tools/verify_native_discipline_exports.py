"""Read actual XLSX ZIP/XML without calling native report code."""
from pathlib import Path
import json
import sys
from xml.etree import ElementTree as ET
from zipfile import ZipFile

NS={"s":"http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
def signature(path):
    with ZipFile(path) as z:
        assert z.testzip() is None
        sheet=ET.fromstring(z.read("xl/worksheets/sheet1.xml"))
        styles=ET.fromstring(z.read("xl/styles.xml"))
        workbook=ET.fromstring(z.read("xl/workbook.xml"))
        strings=[]
        if "xl/sharedStrings.xml" in z.namelist():
            strings=["".join(si.itertext()) for si in ET.fromstring(z.read("xl/sharedStrings.xml"))]
        xfs=list(styles.find("s:cellXfs",NS));fonts=list(styles.find("s:fonts",NS));fills=list(styles.find("s:fills",NS))
        def appearance(index):
            xf=xfs[index];font=fonts[int(xf.get("fontId","0"))];fill=fills[int(xf.get("fillId","0"))]
            color=font.find("s:color",NS);pattern=fill.find("s:patternFill",NS)
            fg=pattern.find("s:fgColor",NS) if pattern is not None else None
            align=xf.find("s:alignment",NS)
            return {"bold":font.find("s:b",NS)is not None,"font":font.find("s:name",NS).get("val"),"size":float(font.find("s:sz",NS).get("val")),"font_rgb":color.get("rgb") if color is not None else None,"fill_rgb":fg.get("rgb") if fg is not None else None,"align":dict(align.attrib) if align is not None else {}}
        rows=[]
        for row in sheet.findall("s:sheetData/s:row",NS):
            cells=[]
            for cell in row:
                assert cell.find("s:f",NS)is None,"Report cells must be literal, not formulas"
                kind=cell.get("t","n")
                if kind=="s":value=strings[int(cell.find("s:v",NS).text)]
                elif kind=="inlineStr":value="".join(cell.find("s:is",NS).itertext())
                else:value=int(cell.find("s:v",NS).text)
                cells.append({"ref":cell.get("r"),"value":value,"style":appearance(int(cell.get("s","0")))})
            rows.append({"height":float(row.get("ht","15")),"cells":cells})
        widths=[]
        for col in sheet.findall("s:cols/s:col",NS):
            widths.extend([float(col.get("width"))]*(int(col.get("max"))-int(col.get("min"))+1))
        pane=sheet.find("s:sheetViews/s:sheetView/s:pane",NS)
        return {"name":workbook.find("s:sheets/s:sheet",NS).get("name"),"dimension":sheet.find("s:dimension",NS).get("ref"),"pane":dict(pane.attrib) if pane is not None else None,"filter":sheet.find("s:autoFilter",NS).get("ref"),"widths":widths,"rows":rows}

def main(directory):
    import ezdxf
    cases=json.loads((Path(__file__).resolve().parents[1]/"rust/tests/fixtures/disciplines/workbooks.json").read_text(encoding="utf-8"))
    for case in cases:
        actual=signature(Path(directory)/(case["name"]+".xlsx"))
        if actual!=case["expected"]:
            # Small structured diagnostics instead of a whole workbook dump.
            for key in actual:
                if actual[key]!=case["expected"][key]:
                    raise AssertionError(f"{case['name']}: {key}: {str(actual[key])[:1500]} != {str(case['expected'][key])[:1500]}")
        drawing=ezdxf.readfile(Path(directory)/(case["name"]+".dxf"))
        audit=drawing.audit()
        assert not audit.errors and not audit.fixes,(case["name"],audit.errors,audit.fixes)
        objects=[entity for entity in drawing.modelspace() if entity.dxftype()=="LINE"]
        assert [{"layer":e.dxf.layer,"color":e.dxf.color} for e in objects]==case["object_styles"],case["name"]
        assert all(len(e.get_xdata("KTM_DXF_EXPORT"))==5 for e in objects)
        labels=[list(e.dxf.insert) for e in drawing.modelspace() if e.dxftype()=="TEXT"]
        assert len(labels)==len(case["label_positions"]),case["name"]
        assert all(abs(a-b)<1e-9 for actual,expected in zip(labels,case["label_positions"]) for a,b in zip(actual,expected)),case["name"]
    print(f"All {len(cases)} native workbooks match original values, styles, dimensions, widths, panes, filters and totals (with recorded count correction); custom-rule DXF layers/colors/label positions and repair-free audits pass.")
if __name__=="__main__":main(sys.argv[1])
