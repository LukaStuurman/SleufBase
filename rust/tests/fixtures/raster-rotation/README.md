# TIFF rotation and original pointer interaction

The actual original `geotiff.set_geotiff_layer_rotation` and hash-validated app
ring/press/drag/release methods run on synthetic layers, with no application,
settings, accounts, network or clipboard. The 36 chains contain 252 absolute
transforms, 1,764 real-pixel ring decisions/screen angles and 144 original pointer
drags. Seven additional exact ring-boundary queries check inclusive tolerances.
Inputs cover affine shear, both directions, repeated absolute changes, reset,
large angles, -180/+180 and the original tiny-angle normalization.

The native GeoTIFF controls supply 90° left/right, decimal-comma/custom angles,
reset and a draggable ring. Rotation is about the original georeferenced image
centre; the original basis is saved once in metadata. No pixel decoding or
texture replacement is needed. Start/reference measurements and unrelated
metadata stay unchanged. Native projects persist the basis; Python imports now
retain it and normalize full revolutions. Earlier native imports lacking a
basis recover it from the actual transform/current angle, avoiding double rotation.

Interactive mode centres the selected TIFF and fits its ring into the viewport,
then holds a fixed camera while rotating. Left dragging changes the angle without
panning; middle dragging still pans. Preset/custom actions retain the current
camera. Escape/right-click/closing the tool ends interaction and keeps the angle,
as in the original mode. Project changes discard stale tools and camera state.
The original 28 px ring padding, 42 px minimum radius, 18 px ring tolerance and
10 px handle/6 px margin are compared. Integer mouse coordinates match Tk input;
exact floating boundary tests use identical explicit geometry. RD transforms and
angles compare within 0.00000002 m/degrees; screen geometry allows 0.000002 px
for NumPy versus native affine arithmetic. The pure angle helper uses identical
input centres, including its otherwise undefined zero-distance pointer case.

Real egui pointer events exercise rotation, release, Escape and middle-button
panning, with unchanged texture/source identities and stored points. Repeated
2,000-cycle changes do not accumulate drift. Atomic errors, old-basis repair and
native/Python project round trips are checked. Actual complete headers are
rendered at 900/1200/1500 px and all named action text checked against its clip;
the two toolbar rows now wrap instead of hiding actions beyond the window.

```
python tools/generate_raster_rotation_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test geotiff_rotation
cargo build --locked --release --manifest-path rust/Cargo.toml --example resolve_geotiff_rotation
python tools/verify_native_geotiff_rotation.py rust/target/release/examples/resolve_geotiff_rotation.exe
```

Set `SLEUFBASE_RASTER_ROTATION_FIXTURES` to regenerate elsewhere. The fresh
runtime audit uses a new seed and 53 chains/371 transforms/2,597 ring decisions
and screen angles/212 actual original pointer-handler moves. CI regenerates the
reference and runs the optimized audit. The full worker readback adds a genuinely
rotated TIFF with selected start/reference points and compares written profile
objects and all prepared TIFF pixels with the original exporter. This is physical
TIFF rotation; full virtual-layer map tools, original UI/settings migration,
raster variants/performance and real-project/release acceptance remain in PARITY.md.
