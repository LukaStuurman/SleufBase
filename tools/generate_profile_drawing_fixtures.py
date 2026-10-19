"""Trace frames/bands from the complete original profile drawing method."""
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
import importlib
import json
import random
import ezdxf
from ezdxf.addons import Importer
from generate_object_export_fixtures import reference
from generate_template_geometry_fixtures import signature

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT / "rust/tests/fixtures/profile-drawing"
LAYER="X-XX-AL-PROFIELBALK-SD"
PREFIX="SAL-VERWIJZING_PROFIELRASTER_"


def main():
    assert ezdxf.__version__=="1.4.4"
    ref=reference()
    module=importlib.import_module(ref.__package__+".cadastral_export")
    importlib.import_module(ref.__package__+".dxf_template_pipeline_v9_patch").install_dxf_template_pipeline_v9_patch()

    class Capture(module.CadastralDxfExporter):
        def _distribute_template_band_labels(self,*args,**kwargs):
            result=super()._distribute_template_band_labels(*args,**kwargs)
            entries=args[1] if len(args)>1 else kwargs["entries"]
            self.bands=[dict(anchor_x=e["anchor_x"],placed_x=e["placed_x"],pinned=e["pin_to_anchor"]) for e in entries]
            return result

        def _add_template_profile_reference_indicator(self,*args,**kwargs):
            model=args[0];before={e.dxf.handle for e in model};self.reference_at=len(before)
            result=self.trace(super()._add_template_profile_reference_indicator,*args,**kwargs)
            self.reference_entities=[signature(e) for e in model if e.dxf.handle not in before]
            self.reference_boxes=result[1]
            return result

        def _distribute_template_leader_labels(self,*args,**kwargs):
            self.obstacles=dict(hard=kwargs.get("hard_static_line_segments",[]),soft=kwargs.get("soft_static_line_segments",[]),text=kwargs.get("static_text_boxes",[]))
            return self.trace(super()._distribute_template_leader_labels,*args,**kwargs)

        def trace(self,method,*args,**kwargs):
            model=args[0] if args and hasattr(args[0],"add_line") else args[1]
            before={e.dxf.handle for e in model}
            result=method(*args,**kwargs)
            self.excluded.update(e.dxf.handle for e in model if e.dxf.handle not in before)
            return result

    exporter=Capture(wfs_client=None)
    OUT.mkdir(parents=True,exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    source=ezdxf.readfile(ROOT / "assets/cadastral_template.dxf")
    doc=ezdxf.new("R2018")
    importer=Importer(source,doc)
    importer.import_blocks([b.name for b in source.blocks if PREFIX in b.name])
    importer.finalize()
    exporter._ensure_template_profile_layers(doc,())
    exporter._setup_text_styles(doc)
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    path=OUT / "blocks.dxf"
    doc.saveas(path)
    path.write_bytes(path.read_bytes().replace(b"\r\n",b"\n"))
    profiles=json.loads((ROOT / "rust/tests/fixtures/profiles/reference.json").read_text(encoding="utf-8"))
    selected=["basic-normal","basic-reverse","polyline-heights-normal","all-matched-normal","named-unmatched-normal","random-00-normal","random-01-reverse","custom-label-unicode-normal"]
    cases=[];reference_cases=[]
    for name in selected:
        case=next(c for c in profiles["cases"] if c["name"]==name)
        features=[]
        for f in case["features"]:
            geom=f["geometry"]
            args=(f["object_name"],f["source_name"])
            if "Point" in geom:
                p=geom["Point"];feature=ref.KickTheMapObjectPoint(*args,p["x"],p["y"],p["z"],*f["attributes"])
            else:
                feature=ref.KickTheMapObjectPolyline(*args,tuple(ref.KickTheMapPolylineVertex(v["x"],v["y"],v["z"]) for v in geom["Polyline"]),*f["attributes"])
            features.append(feature)
        dataset=ref.KickTheMapObjectDataset(1,name,Path("fixture.json"),tuple(f for f in features if isinstance(f,ref.KickTheMapObjectPoint)),tuple(f for f in features if isinstance(f,ref.KickTheMapObjectPolyline)),case["forced_start"])
        rule_data=case["rules"] or profiles["rules"]
        rules=tuple(ref.ObjectLayerRule(tuple(r["keywords"]),r["target_layer"],r["color_aci"],r["label"]) for r in rule_data)
        profile=exporter._build_template_cross_section_profile(dataset,rules,case["roads"],case["terrain"],case["marker_scale"],case["reverse"])
        assert profile is not None
        frozen=case["expected"]
        assert abs(profile.axis_length-frozen["axis_length"])<1e-8
        assert abs(profile.reference_level-frozen["reference_level"])<1e-8
        assert len(profile.points)==len(frozen["points"])
        for point,expected in zip(profile.points,frozen["points"]):
            assert abs(point.chainage-expected["chainage"])<1e-8
            assert (point.layer_name,point.color,point.description,point.is_endpoint)==(expected["layer_name"],expected["color"],expected["description"],expected["is_endpoint"])
        for manual in [None,20,250]:
            doc=ezdxf.readfile(path)
            exporter._setup_text_styles(doc)
            slot=module.TemplateSlot("","",None,module.Bounds(0,-1,30,3),module.Bounds(25,0,27,2),module.Bounds(28,0,30,2),0,0)
            layer=SimpleNamespace(metadata={},bounds=module.Bounds(0,0,10,2))
            show_reference=manual==250
            if show_reference:
                layer.metadata[exporter.TEMPLATE_REFERENCE_POINT_METADATA_KEY]=dict(x=profile.start_point.x+profile.axis_dx*0.37,y=profile.start_point.y+profile.axis_dy*0.37)
            automatic={"start":"Asfalt","middle":"Klinkers","end":"Gras"} if manual==250 else {}
            if manual is not None:
                layer.metadata[exporter.TEMPLATE_MAAIVELD_METADATA_KEY]={"start":{"text":" Asfalt ","color":"#aBcDeF"},"middle":{"text":"invullen","color":"bad"},"end":{"text":"Gras 漢😀","color":"#123456"}}
            fills=exporter._template_maaiveld_fill_segments(layer,use_custom_values=manual is not None,automatic_values=automatic)
            # Supply pre-resolved BGT texts at the drawing-stage boundary.
            exporter._template_bgt_surface_texts=lambda *args: automatic
            clip=manual!=20
            exporter.bands=[];exporter.excluded=set();exporter.reference_boxes=[];exporter.reference_entities=[];exporter.reference_at=0
            exporter._add_template_cross_section(doc,doc.modelspace(),slot,layer,"PS007B",profile,show_reference,manual is not None,[],False,clip,case["marker_scale"],manual,"Ø")
            expected=[signature(e) for e in doc.modelspace() if e.dxf.handle not in exporter.excluded and ((e.dxftype()=="INSERT" and e.dxf.name.startswith(PREFIX)) or (e.dxftype()=="LWPOLYLINE" and e.dxf.layer==LAYER))]
            complete=[signature(e) for e in doc.modelspace() if e.dxf.handle not in exporter.excluded]
            obstacle=dict(hard=exporter.obstacles["hard"],soft=exporter.obstacles["soft"][:3],text=exporter.obstacles["text"][:len(exporter.obstacles["text"])-len(exporter.reference_boxes)])
            marker_name="SAL-VERWIJZING_PROFIELPUNT-MARKER-SOD"
            marker_definition=[signature(e) for e in doc.blocks[marker_name]] if marker_name in doc.blocks else None
            cases.append(dict(name=name+"-"+str(manual),profile_case=name,row=asdict(slot.row_box),tiff=asdict(slot.tiff_box),manual_scale=manual,label="PS007B",reference_chainage=exporter._template_reference_chainage(layer,profile) if show_reference else 0,distance_description=exporter._template_profile_reference_rd_text(layer,profile) if show_reference else "",expected=expected,bands=exporter.bands,body_expected=complete,obstacles=obstacle,metadata=layer.metadata,automatic=automatic,use_custom=manual is not None,clip_markers=clip,fills=[dict(text=fills[k]["text"],rgb=int.from_bytes(bytes(fills[k]["rgb"]),"big")) for k in ["start","middle","end"]],marker_definition=marker_definition,show_reference=show_reference,reference_at=exporter.reference_at,reference_entities=exporter.reference_entities,reference_obstacles=exporter.obstacles))
            denominator=int(expected[0]["attributes"][3]["text"].split(":")[-1])
            start_x=slot.tiff_box.min_x-6.7895;scale=20/denominator;end_x=start_x+profile.axis_length*scale
            layout=dict(start_x=start_x,end_x=end_x,reference_y=slot.tiff_box.min_y-.4283,title_y=slot.tiff_box.min_y-.4683,top_band_y=slot.tiff_box.min_y-.4883,bottom_band_y=slot.tiff_box.min_y-.6883,bottom_y=slot.tiff_box.min_y-.8883,denominator=denominator,scale=scale)
            low=-2/scale;high=profile.axis_length+(slot.tiff_box.min_x-end_x)/scale
            for target in [low-1,low,-.5,0,profile.axis_length*.25,profile.axis_length*.75,profile.axis_length,high,high+1]:
                metadata={"template_reference_point":dict(x=profile.start_point.x+target*profile.axis_dx/profile.axis_length,y=profile.start_point.y+target*profile.axis_dy/profile.axis_length),"rotation_degrees":37.5}
                reference_layer=SimpleNamespace(metadata=metadata,bounds=layer.bounds)
                reference_doc=ezdxf.readfile(path)
                model=reference_doc.modelspace()
                sy=layout["reference_y"]+(float(profile.start_point.z or 0)-profile.reference_level)*scale;ey=layout["reference_y"]+(float(profile.end_point.z or 0)-profile.reference_level)*scale
                segments,boxes=module.CadastralDxfExporter._add_template_profile_reference_indicator(exporter,model,reference_layer,profile,start_x=start_x,left_edge_x=start_x-2,right_edge_x=slot.tiff_box.min_x,profile_end_x=end_x,horizontal_scale=scale,terrain_start_y=sy,terrain_end_y=ey)
                raw=exporter._template_reference_raw_point(reference_layer,profile);center=[raw[0]+3,raw[1]-4]
                display=exporter._rotate_template_map_point(*raw,*center,37.5)
                reference_cases.append(dict(profile_case=name,layout=layout,right_edge=slot.tiff_box.min_x,metadata=metadata,raw=raw,chainage=exporter._template_reference_chainage(reference_layer,profile),description=exporter._template_profile_reference_rd_text(reference_layer,profile),center=center,display=display,expected=[signature(e) for e in model],segments=segments,boxes=boxes))
    labels=["PS007B","sleuf ٠٠١٢ſİıK","PS A","ps geen","x 0z","PS1a2b","nr １２３XYZ"]
    rng=random.Random(73318)
    band_cases=[]
    for index in range(100):
        doc=ezdxf.readfile(path)
        count=index%31
        low=rng.uniform(-30,30);high=low+rng.choice([0.01,0.1,1,5])
        height=rng.choice([0.02,0.036,0.08])
        anchors=[rng.uniform(low-1,high+1) for _ in range(count)]
        if count>3:anchors[1]=anchors[2]
        pinned=[rng.random()<0.2 for _ in range(count)]
        entries=[]
        for anchor,pin in zip(anchors,pinned):
            refs=[exporter._insert_template_block(doc.modelspace(),PREFIX+"BAND_WAARDE-SOD",(anchor,y),LAYER,{"WAARDE":"0.00"}) for y in [0,-0.2]]
            entries.append(dict(anchor_x=anchor,pin_to_anchor=pin,top_ref=refs[0],top_attr=refs[0].attribs[0],top_y=0,top_offset_x=0,top_offset_y=0,top_height=height,bottom_ref=refs[1],bottom_attr=refs[1].attribs[0],bottom_y=-0.2,bottom_offset_x=0,bottom_offset_y=0,bottom_height=height))
        exporter._distribute_template_band_labels(doc.modelspace(),entries,low,high)
        band_cases.append(dict(min_x=low,max_x=high,height=height,anchors=anchors,pinned=pinned,expected=[e["placed_x"] for e in entries]))
    distances=[-0.0001,-0.0049,-0.005,-0.0,0.0,2.125,2.675,1.005,12345.675]
    clip_cases=[]
    for _ in range(150):
        a=[rng.uniform(-3,3),rng.uniform(-3,3)];b=[rng.uniform(-3,3),rng.uniform(-3,3)]
        clip_cases.append(dict(a=a,b=b,left=-1,right=1,bottom=0,expected=exporter._clip_profile_marker_segment(a,b,-1,1,0)))
    fill_cases=[]
    for item in [{},{"text":None,"color":"bad"},{"text":"invullen","color":"#abcdef"},{"text":" Tegels 漢😀 ","color":"#١٢٣٤٥٦"},{"text":True,"color":False},{"text":0,"color":"#123456"}]:
        metadata={"template_maaiveld_segments":{"middle":item}}
        for use_custom in [False,True]:
            automatic={"middle":"Asfalt"};resolved=exporter._template_maaiveld_fill_segments(SimpleNamespace(metadata=metadata),use_custom_values=use_custom,automatic_values=automatic)
            fill_cases.append(dict(metadata=metadata,automatic=automatic,use_custom=use_custom,expected=[dict(text=resolved[k]["text"],rgb=int.from_bytes(bytes(resolved[k]["rgb"]),"big")) for k in ["start","middle","end"]]))
    result=dict(cases=cases,band_cases=band_cases,clip_cases=clip_cases,fill_cases=fill_cases,reference_cases=reference_cases,rd_values=[dict(value=v,text=exporter._format_template_rd_coordinate(v)) for v in [-2.5,-1.5,-.5,0,.5,1.5,2.5,1e20,1.234e100]],labels=[dict(label=s,number=exporter._profile_identifier(s),description=exporter._profile_description(s)) for s in labels],distances=[dict(value=v,text=exporter._format_template_profile_distance(v)) for v in distances])
    (OUT / "reference.json").write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(len(cases),"original profile frame/band drawings",sum(len(c["expected"]) for c in cases),"entities")


if __name__=="__main__":main()
