"""Compare raw native donor graphs with the original promoter and CAD readback.

The supplied executable must be a freshly built export_profile_leader example.
No ezdxf save is allowed after promotion: it drops native action objects.
"""
from pathlib import Path
import re
import io
import importlib
import json
import shutil
import subprocess
import sys
import tempfile
import ezdxf
from generate_profile_leader_fixtures import donor_module, ROOT, OUT
from verify_native_template_edits import records
from generate_template_geometry_fixtures import signature
from verify_native_template_images import compare, verify_preservation


def primary(record):
    if record[0][1] == "SECTION":
        return None
    for code, value in record:
        if code == 100:
            break
        if code in (5, 105):
            return value.strip().upper()
    return None


def normalized(path, source):
    result = records(path)
    original = records(source)
    old_handles = {primary(r) for r in original} - {None}
    handles = [primary(r) for r in result if primary(r)]
    assert len(handles) == len(set(handles)), "duplicate primary handle"
    new = {h: f"<new {i}>" for i, h in enumerate(h for h in handles if h not in old_handles)}
    assert len(new) == 15
    maximum = max(int(h, 16) for h in handles)
    seed = False
    for record in result:
        app = None
        header = record[0] == (0, "SECTION") and (2, "HEADER") in record
        target = record[0][1] == "BLOCK_RECORD" and (2, "SAL-VERWIJZING_LEIDING_BOVENKANT-SOD") in record
        for index, (code, value) in enumerate(record):
            if header:
                if code == 9:
                    seed = value == "$HANDSEED"
                elif seed and code == 5:
                    assert int(value, 16) > maximum
                    record[index] = (code, "<next free handle>")
            elif (code in (5, 105) or code in range(320, 370) or code in range(390, 400) or code in (480, 481, 1005)) and value.strip().upper() in new:
                record[index] = (code, new[value.strip().upper()])
            if code == 1001:
                app = value
            elif target and app == "AcDbDynamicBlockGUID" and code == 1000:
                assert re.fullmatch(r"\{[0-9A-F]{8}-[0-9A-F]{4}-4[0-9A-F]{3}-[89AB][0-9A-F]{3}-[0-9A-F]{12}\}", value)
                record[index] = (code, "<fresh UUID4>")
    return result


def verify(executable, source, output, module):
    oracle = output.with_name(output.stem + "-oracle.dxf")
    shutil.copyfile(source, oracle)
    assert module.promote_profile_leader_block(oracle) == 1
    subprocess.run([str(executable), str(source), str(output)], check=True)
    # Fresh handle numbers can differ: the Python promoter skips the old
    # HANDSEED; Rust uses the first free handle. Graph targets must be identical.
    assert normalized(output, source) == normalized(oracle, source), source.name
    # Actual original inspector: exact parameters, action targets and rep tags.
    info = module.inspect_profile_leader_block(output)
    assert info["donor_sha256"] == module.DONOR_SHA256 and info["is_dynamic"]
    assert info["polar_base"] == module.POLAR_BASE
    assert info["polar_top"] == info["polar_grip"] == module.POLAR_TOP
    handles = info["entity_handles"]
    assert info["stretch_refs"] == (handles["leader"],)
    assert set(info["move_refs"]) == {handles["description"], handles["depth"]}
    assert info["scale_refs"] == (handles["circle"],)
    assert info["block_rep_tag"][:2] == ((1070, "1"), (1071, "4"))
    for key, index in module.DONOR_REP_TAG_INDEX.items():
        assert info["entity_rep_tags"][key][:2] == ((1070, "1"), (1071, str(index)))
        assert info["entity_rep_tags"][key][2] == (1005, handles[key] if key in {"circle", "leader"} else "0")
    read = lambda path: ezdxf.read(io.StringIO(path.read_text(encoding="utf-8-sig")))
    actual_audit, baseline = read(output).audit(), read(source).audit()
    assert not actual_audit.errors
    assert [e.code for e in actual_audit.fixes] == [e.code for e in baseline.fixes]
    again = output.with_name(output.stem + "-again.dxf")
    subprocess.run([str(executable), str(output), str(again)], check=True)
    assert again.read_bytes() == output.read_bytes()
    print(f"{source.name}: all source tags and 15 native donor records match; Polar/stretch/move/scale checked")


def graph(path, module):
    pairs, _, _ = module.dv._read_pairs(path)
    all_records=module.dv._split_records(pairs)
    sections=module.dv._record_sections(all_records)
    _,block=module.dv._target_block_record(all_records,sections,module.BLOCK_NAME)
    handle=module.dv._record_handle(block)
    root=module._extension_dictionary_handle(block)
    descendants=module._metadata_descendant_handles(all_records,sections,root)
    geometry=[r for r,s in zip(all_records,sections) if s=="BLOCKS" and module.dv._record_owner(r)==handle]
    metadata=[r for r,s in zip(all_records,sections) if s=="OBJECTS" and module.dv._record_handle(r) in descendants]
    result=[block,*geometry,*metadata]
    assert len(result)==22
    aliases={module.dv._record_handle(r):f"<graph {i}>" for i,r in enumerate(result)}
    for record in result:
        app=None
        for i,(c,v) in enumerate(record):
            if (c==5 or c in module.dv._HANDLE_REFERENCE_CODES) and v.strip().upper() in aliases:
                record[i]=(c,aliases[v.strip().upper()])
            if c==1001:app=v
            elif app=="AcDbDynamicBlockGUID" and c==1000:record[i]=(c,"<fresh GUID>")
    return result


def verify_install(executable,source,output,module):
    fixture=json.loads((OUT / "references.json").read_text(encoding="utf-8"))
    request_path=output.with_suffix(".json")
    request_path.write_text(json.dumps(fixture["requests"],ensure_ascii=False),encoding="utf-8")
    subprocess.run([str(executable),str(source),str(output),"--install",str(request_path)],check=True)
    patch=importlib.import_module(module.__package__+".template_dynamic_profile_leader_patch")
    exporter=patch.CadastralDxfExporter(wfs_client=None)
    original=ezdxf.readfile(source)
    doc=ezdxf.readfile(source)
    existing=module.BLOCK_NAME in doc.blocks
    assert patch._find_template_profile_leader_reference(doc)==(None,None), "native prototype-clone path is a separate requirement"
    if not existing:module.ensure_profile_leader_geometry(doc)
    if module.LAYER_NAME not in doc.layers:doc.layers.add(module.LAYER_NAME,color=7,dxfattribs={"lineweight":0})
    expected_original=[]
    for request in fixture["requests"]:
        patch._add_dynamic_template_profile_leader(exporter,doc.modelspace(),request["description"],request["depth"],request["start"],(999,999),request["scale"],request["color"])
        expected_original.append(signature(list(doc.modelspace())[-1]))
    oracle=output.with_name(output.stem+"-oracle.dxf")
    doc.saveas(oracle)
    module.promote_profile_leader_block(oracle)
    # Existing AutoCAD graphs are retained directly from the source. Saving
    # through ezdxf would discard native action objects and rewrite BLKREFS.
    assert graph(output,module)==graph(source if existing else oracle,module),source.name
    new_types={r[0][1] for r in module._donor_metadata_records()}|{"BLOCK_RECORD","BLOCK","ENDBLK","CIRCLE","ATTDEF","LWPOLYLINE","INSERT","ATTRIB","SEQEND","APPID"}
    verify_preservation(source,output,original,allowed_extra=new_types,allowed_tables=("LAYER","BLOCK_RECORD","APPID"))
    native=ezdxf.readfile(output)
    inserts=[e for e in native.modelspace().query("INSERT") if e.dxf.name==module.BLOCK_NAME]
    assert len(inserts)==len(fixture["expected"])
    if not existing:compare(expected_original,fixture["expected"],"frozen-original-placements")
    for i,(entity,expected) in enumerate(zip(inserts,expected_original)):compare(signature(entity),expected,f"{source.name}.reference[{i}]")
    raw=records(output)
    handles=[int(h,16) for r in raw if (h:=primary(r)) is not None]
    assert len(handles)==len(set(handles)) and int(native.header["$HANDSEED"],16)>max(handles)
    assert all(name in native.appids for name in ["AcDbBlockRepETag","AcDbDynamicBlockTrueName","AcDbDynamicBlockGUID"])
    baseline=original.audit();audit=native.audit()
    assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes]
    again=output.with_name(output.stem+"-again.dxf")
    subprocess.run([str(executable),str(output),str(again),"--install"],check=True)
    assert output.read_bytes()==again.read_bytes()
    print(f"{source.name}: complete installed/retained graph, 40 native INSERTs/80 attributes and all source records verified")


def main():
    executable = Path(sys.argv[1]).resolve(strict=True)
    module = donor_module()
    with tempfile.TemporaryDirectory(prefix="sleufbase-native-donor-") as temporary:
        directory = Path(temporary)
        for name in ["leader", "tagged"]:
            verify(executable, OUT / (name + ".dxf"), directory / (name + ".dxf"), module)
        # High HANDSEED and lower-case pointers exercise allocation/remapping.
        source = directory / "high-seed-input.dxf"
        pairs, newline, bom = module.dv._read_pairs(OUT / "leader.dxf")
        module.dv._set_header_handseed(pairs, 0xFFFF0000)
        pairs = [(c, v.lower() if c in module.dv._HANDLE_REFERENCE_CODES or c == 5 else v) for c, v in pairs]
        module.dv._write_pairs(source, pairs, newline=newline, had_bom=bom)
        verify(executable, source, directory / "high-seed.dxf", module)
        # Real 18 MiB template: install only original approved geometry before
        # testing native finalization. Native geometry installation is separate.
        source = directory / "shipped-prepared-input.dxf"
        doc = ezdxf.readfile(ROOT / "assets/cadastral_template.dxf")
        module.ensure_profile_leader_geometry(doc)
        doc.saveas(source)
        verify(executable, source, directory / "shipped.dxf", module)
        verify_install(executable,OUT / "empty.dxf",directory / "installed-empty.dxf",module)
        verify_install(executable,ROOT / "assets/cadastral_template.dxf",directory / "installed-shipped.dxf",module)
        source=directory / "missing-appids-input.dxf"
        doc=ezdxf.readfile(OUT / "empty.dxf")
        for name in ["AcDbBlockRepETag","AcDbDynamicBlockTrueName","AcDbDynamicBlockGUID"]:doc.appids.remove(name)
        doc.saveas(source)
        verify_install(executable,source,directory / "installed-appids.dxf",module)
    print("Native donor: four promotion audits and three geometry/attribute/resource audits passed")


if __name__ == "__main__":
    main()
