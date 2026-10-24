"""Primary ezdxf 1.4.4 surface/control-cage oracles; no native geometry used."""
import json
from pathlib import Path
import ezdxf
from ezdxf.math import Vec3
from ezdxf.render import MeshBuilder

OUT = Path(__file__).resolve().parents[1] / "rust/tests/fixtures/meshes"

def main():
    if ezdxf.__version__ != "1.4.4":
        raise RuntimeError("These fixtures require the reviewed ezdxf 1.4.4 reference")
    OUT.mkdir(parents=True, exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    doc = ezdxf.new("R2018")
    doc.appids.new("SOURCE_ID")
    msp = doc.modelspace()
    def identify(entity, name):
        entity.set_xdata("SOURCE_ID", [(1000, name)])
        return entity
    for flags in range(16):
        identify(msp.add_3dface([(0,0,1),(5,0,2),(6,4,3),(0,3,4)],dxfattribs={"invisible_edges":flags}),f"quad-{flags}")
    identify(msp.add_3dface([(1,2,3),(5,2,4),(4,6,7)],dxfattribs={"invisible_edges":4}),"triangle-hidden-third")
    for kind in ("solid","trace"):
        for count in (3,4):
            identify(getattr(msp, "add_"+kind)([(0,0,2),(4,0,2),(0,3,2),(5,4,2)][:count],dxfattribs={"extrusion":(1,2,-3),"thickness":1.5}),f"{kind}-{count}")
    face=msp.add_polyface()
    face.append_faces([[(0,0,1),(4,0,2),(4,3,3),(0,3,4)],[(0,0,1),(4,3,3),(1,5,2)]])
    _, proxies=face.indexed_faces()
    proxy=list(proxies)[0]
    proxy.face_record.dxf.vtx2 *= -1
    proxy.face_record.dxf.color=3
    proxy.face_record.set_xdata("SOURCE_ID",[(1000,"face-metadata")])
    face.vertices[0].dxf.true_color=0
    # Declared header counts may be wrong; mixed ordering is legal to read.
    face.dxf.m_count=999;face.dxf.n_count=999
    face.vertices.insert(0,face.vertices.pop())
    identify(face,"polyface-mixed-order")
    for flags in (0,1,32,33):
        entity=msp.add_polymesh((3,4),dxfattribs={"flags":16|flags,"smooth_type":5 if flags==33 else 0})
        for a in range(3):
            for b in range(4):entity.set_mesh_vertex((a,b),(a*2,b*3,a*a-b))
        identify(entity,f"polygon-{flags}")
    for subdivision in (0,2):
        entity=msp.add_mesh(dxfattribs={"subdivision_levels":subdivision,"blend_crease":1})
        with entity.edit_data() as data:
            data.vertices=[(0,0,0),(4,0,1),(5,2,2),(2,4,3),(0,3,1),(7,6,4)]
            data.faces=[(0,1,2,3,4),(2,3,5)]
            data.edges=[(0,5),(2,3)]
            data.edge_crease_values=[0.5,-1.0]
        identify(entity,f"mesh-{subdivision}")
    block=doc.blocks.new("Surfaces",base_point=(1,2,3))
    for entity in list(msp):block.add_entity(entity.copy())
    nested=doc.blocks.new("Nested")
    nested.add_blockref("Surfaces",(8,-3,4),dxfattribs={"rotation":37,"xscale":2,"yscale":0.5,"zscale":-1})
    msp.add_blockref("Nested",(168000,401000,6),dxfattribs={"rotation":-29,"xscale":-1.2,"yscale":3,"zscale":2,"extrusion":(0.3,0.5,0.7)})
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/"meshes-ascii.dxf",fmt="asc")
    doc.saveas(OUT/"meshes-binary.dxf",fmt="bin")
    doc=ezdxf.readfile(OUT/"meshes-ascii.dxf")
    expected=[]
    def segments(entity):
        kind=entity.dxftype()
        if kind in ("SOLID","TRACE","3DFACE"):
            # Face3d.wcs_vertices() in 1.4.4 collapses the duplicated fourth
            # corner, moving the closing edge's bit from 8 to 4. Use raw WCS
            # corners for masks; DXF bits still refer to four source edges.
            vertices=([entity[i] for i in range(4)]+[entity[0]]) if kind=="3DFACE" else list(entity.wcs_vertices(close=True))
            return [(a,b) for i,(a,b) in enumerate(zip(vertices,vertices[1:])) if kind!="3DFACE" or not(entity.dxf.invisible_edges&(1<<i))]
        if kind=="POLYLINE" and entity.is_poly_face_mesh:
            _,proxies=entity.indexed_faces()
            return [(Vec3(face[i].dxf.location),Vec3(face[(i+1)%len(face)].dxf.location)) for face in proxies for i in range(len(face)) if face.is_edge_visible(i)]
        if kind=="POLYLINE":
            # Use ezdxf's public row/column access and documented M/N closure.
            m,n=entity.dxf.m_count,entity.dxf.n_count
            at=lambda a,b:Vec3(entity.get_mesh_vertex((a%m,b%n)).dxf.location)
            rows=m if entity.is_m_closed else m-1
            cols=n if entity.is_n_closed else n-1
            return [(a,b) for i in range(rows) for j in range(cols) for a,b in zip([at(i,j),at(i+1,j),at(i+1,j+1),at(i,j+1)],[at(i+1,j),at(i+1,j+1),at(i,j+1),at(i,j)])]
        vertices=entity.vertices
        return [(Vec3(vertices[a]),Vec3(vertices[b])) for face in entity.faces for a,b in zip(face,list(face[1:])+[face[0]])]+[(Vec3(vertices[a]),Vec3(vertices[b])) for a,b in entity.edges]
    def walk(entities, transform=lambda p:Vec3(p),path=()):
        for entity in entities:
            if entity.dxftype()=="INSERT":
                matrix=entity.matrix44()
                walk(entity.block(),lambda p,t=transform,m=matrix:t(m.transform(p)),path+(entity.dxf.handle,))
                continue
            edges=set()
            for a,b in segments(entity):
                if a.isclose(b,abs_tol=0,rel_tol=0):continue
                edge=tuple(sorted((tuple(transform(a)),tuple(transform(b)))))
                edges.add(edge)
            case={"id":entity.get_xdata("SOURCE_ID")[0].value,"nested":bool(path),"edges":sorted(edges)}
            if not path and entity.dxftype()=="POLYLINE" and entity.is_polygon_mesh:
                cage=MeshBuilder.from_polyface(entity)
                case["first_face"]=[list(cage.vertices[i]) for i in cage.faces[0]]
            expected.append(case)
    walk(doc.modelspace())
    (OUT/"expected.json").write_text(json.dumps(expected,separators=(",",":"))+"\n",encoding="utf-8")
    print(f"Generated {len(expected)} source/world mesh cases, {sum(len(v['edges']) for v in expected)} visible edges")

if __name__=="__main__":main()
