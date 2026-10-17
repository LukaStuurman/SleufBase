"""Generate a synthetic .klicproject with the verified original serializer.

Run with `py -3.11 tools/generate_legacy_project_fixture.py`. Only two validated
code objects are invoked on a synthetic workspace; the application/GUI is not
started and no account or user project is read.
"""
from __future__ import annotations
import dis
import importlib.util
import json
from datetime import datetime, timezone
from pathlib import Path
from types import CodeType, FunctionType, SimpleNamespace

ROOT=Path(__file__).resolve().parents[1]


def main():
    spec=importlib.util.spec_from_file_location("legacy_helper",ROOT/"legacy_bytecode.py")
    helper=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    module=helper.read_validated_code("app",ROOT/"legacy_bytecode.py")
    cls=next(c for c in module.co_consts if isinstance(c,CodeType) and c.co_name=="KlicViewerApp")
    instructions=list(dis.get_instructions(cls))
    version=next(instructions[i-1].argval for i,c in enumerate(instructions) if c.opname=="STORE_NAME" and c.argval=="PROJECT_FILE_VERSION")
    class Clock:
        @staticmethod
        def now():
            return datetime(2026,10,17,12,0,tzinfo=timezone.utc)
    env={"Path":Path,"datetime":Clock}
    methods={c.co_name:FunctionType(c,env) for c in cls.co_consts if isinstance(c,CodeType) and c.co_name in {"_current_project_payload","_json_safe_project_value"}}
    class Workspace:
        PROJECT_FILE_VERSION=version
        _json_safe_project_value=staticmethod(methods["_json_safe_project_value"])
        _current_project_payload=methods["_current_project_payload"]
        _is_virtual_trench_layer=staticmethod(lambda layer: "virtual_trench_payload" in layer.metadata)
        _current_template_export_settings_payload=staticmethod(lambda:{"custom_unknown_setting":{"preserve":[1,2,"é"]}})
    env["KlicViewerApp"]=Workspace
    w=Workspace()
    w.current_project_path=Path("PS42.klicproject")
    w.center_x=168002.0;w.center_y=401001.0;w.meters_per_pixel=0.05
    w.opacity_var=SimpleNamespace(get=lambda:80)
    w.use_background_var=SimpleNamespace(get=lambda:True)
    w.background_source_var=SimpleNamespace(get=lambda:"PDOK luchtfoto")
    w.selected_tiff_index=0;w.selected_dxf_index=0
    w.template_export_slot_order=[{"custom_slot":"PS42"}]
    point=lambda role,x,y,z,**extra:dict(role=role,x=x,y=y,z=z,**extra)
    metadata={"virtual_trench_payload":{"width_meters":2.0,"source":"marxact",
        "points":[point("start",168000,401001,11),
            point("object",168002,401001,3.12,object_name="water detail",source_name="Waterleiding",
                attribute_1="water detail",attribute_2="Water",attribute_3="marxact_point",custom_object_metadata={"keep":True}),
            point("end",168004,401001,16)]},
        "marxact_boundary_3d":[[168000,401000,10],[168004,401000,14],[168004,401002,18],[168000,401002,12]],
        "marxact_source_path":"assets/PS42.dxf","marxact_alignment_rotation_degrees":0,
        "marxact_alignment_auto_start":[168000,401001,11],"marxact_alignment_auto_end":[168004,401001,16],
        "marxact_alignment_auto_width":2,"kickthemap_job_id":123,
        "custom_metadata":{"unknown":["preserve",{"also":True}]}}
    def path(stem,relative):
        return SimpleNamespace(stem=stem,resolve=lambda:relative)
    w.tiff_layers=[SimpleNamespace(path=path("PS42",None),metadata=metadata),
        SimpleNamespace(path=path("reference","assets/reference.tiff"),metadata={"rotation_degrees":15,"kickthemap_job_id":456})]
    w.dxf_overlays=[SimpleNamespace(path=path("context","assets/context.dxf"),visible=False)]
    w.map_comments=[SimpleNamespace(x=168001.5,y=401001.5,text="Controleer diepte – café")]
    payload=w._current_project_payload()
    # Normalize only the timestamp to keep generation deterministic in all zones.
    payload["saved_at"]="2026-10-17T12:00:00+00:00"
    output=ROOT/"rust/tests/fixtures/python-v1.klicproject"
    output.write_text(json.dumps(payload,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(f"Generated Python v{version} project using the original serializer: {output}")


if __name__=="__main__":
    main()
