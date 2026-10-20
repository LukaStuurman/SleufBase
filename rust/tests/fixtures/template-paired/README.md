# Complete native Normaal / Reverse exports

`export_paired_templates` runs the real template exporter on five scenarios with
fourteen selected slots in total. Each scenario writes a forward-only export,
a reverse-only export and a paired export from the same inputs. Cases include
drawn/hidden profiles, photos on both panels with quarter-turn rotation, a native
virtual raster, image-only slots, duplicate labels, reserved empty slots and an
extra sheet. Actual sources and map preparation are used, not reversed screenshots
or preview rasters. Network backgrounds are disabled for reproducibility.

`verify_native_paired_templates.py` checks every state's complete ordered content,
attributes, leader links and PNG pixels against its separate single-direction
export. It also reconstructs the visibility graph and automatic grip with the
original production promotion/finalization functions. Donor tags, references,
UUID4 shape, initial visibility, raw child owners and model-wrapper ordering
must agree. This integration proof complements the existing independent Python
profile/raster/map references; matching two native calls alone would not prove
full Python rendering parity.

The approved CAD template contains one existing owner-0 proxy that ezdxf's audit
removes. Its source tags are required to survive unchanged; audit repairs must
exactly match the original template's baseline. No new repair or error is
allowed. The small dynamic-visibility golden fixtures still require zero fixes.
Original unknown proxy data is not deleted to make a verifier green.

The companion XLSX is compared member by member with both single-direction
workbooks. Cell order, counts, styles and package structure remain equal. Valid
creation/modification timestamps are normalized; no other worksheet/package
differences are ignored. Every selected slot is counted once.

Windows-font discovery is independently checked against a freshly built original
font catalog for fifteen filename/family/case/fallback requests. Native indexing
reads bounded SFNT name/OS2 tables, not all glyph data. Actual glyph bytes are
shared and loaded on demand. The original fallback priority and directory
precedence are retained. Known SHX/SHP/LFF fonts fail explicitly until their
parsers are implemented; support/configuration directories beyond the Windows
defaults still need the remaining settings migration.

Both native dialogs default to paired output and save this preference. Real egui
worker regressions verify both directions, project name/local date, input-owner
restoration and save/export exclusion. Additional tests cancel or panic late,
remove the donor, reject ambiguous pre-oriented PNG input and verify unchanged
previous drawings/bundles. Existing single-direction oracle cases stay explicit.

Further release work includes SHX/SHP/LFF, other CAD/font types, original photo
annotations/virtual masks, complete settings migration and interactive AutoCAD
acceptance. All global gates in `rust/PARITY.md` remain mandatory.
