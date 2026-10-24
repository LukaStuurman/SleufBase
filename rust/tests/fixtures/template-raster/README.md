# GeoTIFF-beelden voor sjabloonvakken

140 voorbereidingen vergelijken 42.716 volledige RGBA-pixels zonder
pixeltolerantie, inclusief verborgen RGB bij alpha nul. Tien bronpatronen
bestrijken smalle/kleine beelden, gedeeltelijke alpha, grenswaarden 15/16,
witte eilanden, schaakborden en geheel transparante/witte beelden. Twaalf
profielhoeken per patroon, RD/shear-transformaties, omgekeerde richting,
wegen/terrein en prioriteit/ties controleren de bronsemantiek. Acht echte
RGBA-TIFFs controleren alle Orientation-tags in de volledige decoder.

136 resultaten komen rechtstreeks van de oorspronkelijke V9-helper met
Pillow 11.3.0. Vier resultaten hebben een expliciete correctie: Pillow levert
bij deze RGBA-TIFFs met Orientation 5..8 verkeerde post-load afmetingen.
`tifffile==2025.5.10` controleert de ruwe numerieke RGBA-pixels tegen de bron;
daarna wordt de oriëntatie expliciet toegepast en volgen de oorspronkelijke
rotatie-/crop-/alphahelpers. Zowel de foutieve oorspronkelijke PNG/details als
de gecorrigeerde referentie blijven in de fixtures bewaard. Het verschil staat
per geval in `intentional_difference`; de native decoder blijft correct omkeren
en breedte/hoogte verwisselen. Geen algemene tolerantie maskeert dit verschil.

Rotatie volgt de oorspronkelijke uitgebreide bicubische RGBA-transformatie
met premultiplicatie, 15-decimale matrixafronding, randclamping en byteconversie.
De zichtbare alpha (>15) bepaalt eerst de crop met twee pixels marge. Daarna
verdwijnen alleen randverbonden transparante/bijna-witte achtergronden, met
vier-connectiviteit en behoud van ingesloten wit. Native gebruikt vier staten
in de eigen alphabuffer; volledige RGB-/booleaanse maskerkopieën zijn niet nodig.
Een scanline-stack markeert runs direct, zodat zij niet dubbel in de wachtrij
komen. PNG-opslag is annuleerbaar en atomair; bestaande bestanden blijven staan.

De volledige TIFF-decoder deelt kleur-/strook-/tegelbehandeling met de preview,
maar verkleint niet. 45 bestaande ongeschaalde kleur-/oriëntatiereferenties zijn
ook voor deze route getest. Een 4097×4097-strokenbestand controleert daarnaast
iedere oorspronkelijke pixel, inclusief de rij/kolom die de preview overslaat. De bronlimiet is 150 miljoen pixels, het chunkbudget
128 MiB; rotatie weigert expansie boven 300 miljoen pixels vóór allocatie.
Full decoding maakt geen tijdelijke lijst met iedere bronkolom/-rij.
Overige TIFF-modi/overviews, kaart-/foto-opbouw, virtuele rasters en de volledige
native exportbediening blijven afzonderlijke migratie-eisen.

    python tools/generate_template_raster_fixtures.py
    cargo run --release --manifest-path rust/Cargo.toml --example export_template_rasters -- NIEUWE_MAP
    python tools/verify_native_template_rasters.py NIEUWE_MAP

De affine/bicubische implementatie is aangepast uit
[Pillow 11.3.0](https://github.com/python-pillow/Pillow/tree/11.3.0/src/libImaging)
en [Image.py](https://github.com/python-pillow/Pillow/blob/11.3.0/src/PIL/Image.py).
De [MIT-CMU-notice](../../../third_party/Pillow-LICENSE.txt) is opgenomen in de
Windows-artifact en is ook opvraagbaar met `sleufbase-rust.exe --licenses`.
