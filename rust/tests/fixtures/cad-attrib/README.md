# Linked CAD attributes

The native overlay preserves ordered ATTRIB source entities on their INSERT
owner, alongside the old last-value-per-tag metadata map. Duplicate tags are
not collapsed for drawing. R12 and modern AcDbText/AcDbAttribute fields are
separated: 73 is field length, 74 is vertical alignment, and R2018's repeated
71/72/11 are not text generation/alignment. Last-280 lock position and raw
XRECORD/embedded-MTEXT tags are retained. See the primary [ATTRIB group-code
reference](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-DXF/files/GUID-7DD8B495-C3F8-48CD-A766-14F9D7D0DD9B.htm).

ATTRIB geometry already lives outside its owning INSERT's local block space.
It receives only outer transforms plus its rotated, unscaled MINSERT offset
in its own OCS. Per-instance metadata is borrowed through the shared source
block. Attributes draw after block contents; flags and generic invisibility,
parent/layer visibility, layer-0 inheritance, by-block color and layer-name
case aliases apply. A missing block still displays its independently located
attributes and reports its missing definition.

The original ezdxf 1.4.4 `Insert.multi_insert()` changes ATTRIB.insert but leaves
align_point unchanged. Native code translates the complete contour, keeping
both anchors together. The reference keeps **240 materially different old
results** next to corrected copies made with the original text renderer and
both OCS anchors translated. The [Autodesk attribute alignment rules](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-DXF/files/GUID-7DD8B495-C3F8-48CD-A766-14F9D7D0DD9B.htm)
and [MINSERT grid rules](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-Core/files/GUID-A780A2FA-4A2E-4574-950F-E788AB71F527.htm)
support this rigid translation. Arithmetic-order noise is not counted as a bug.

ASCII and binary fixtures contain **774 scene entities / 471 ATTRIB instances**:
all 15 alignments, TTF/SHX/LFF, signed nonuniform scales, reflection/rotation,
different owner/attribute OCS, nested INSERT/MINSERT, flags, duplicate tags,
source metadata and hidden distant labels. Curve samples check at most 0.005
drawing units of WCS chord error; anchor/endpoints allow `1e-8 + 128*EPS*scale`.
Projects keep the geometry and unsupported raw data without font files.
Malformed linked structures and non-finite points cannot replace a valid
project. Actual egui painting and map export check attached attribute ink.
Zero-spacing 65535x65535 MINSERT collapses to one instance. A 10,000-instance
probe borrows one 1 MB XDATA source by address and serializes under 1.1 MB;
this proves source sharing, not total app RSS or speed.

`generate_cad_attrib_fixtures.py` regenerates all three files byte-for-byte
under different hashseeds; use `SLEUFBASE_CAD_ATTRIB_FIXTURES` for another
directory. Its original-font oracle is shared with `verify_native_cad_attrib.py`,
which generates **four fresh drawings / 3096 entities / 1884 ATTRIB instances**
and compares the optimized `render_cad_scene` reader/scene, metadata and
bidirectional curve/chord distances without saved expected output.

This implements single-line linked ATTRIB contours. Embedded MTEXT is preserved
with an explicit unsupported-layout warning and no fabricated fallback text.
ATTDEF display, complete MTEXT, TrueType fill, space/layout selection, further
CAD/vendor variants and real AutoCAD/project acceptance remain open. The
original global PARITY.md release gates still apply. MIT notices for the
attribute/insert and text2path contracts ship in `--licenses` and artifacts.
