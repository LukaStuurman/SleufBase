"""Compare raw native donor graphs with the original promoter and CAD readback.

The supplied executable must be a freshly built export_profile_leader example.
No ezdxf save is allowed after promotion: it drops native action objects.
"""
from pathlib import Path
import re
import io
import shutil
import subprocess
import sys
import tempfile
import ezdxf
from generate_profile_leader_fixtures import donor_module, ROOT, OUT
from verify_native_template_edits import records


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
    print("Native donor promotion independently verified in four drawings")


if __name__ == "__main__":
    main()
