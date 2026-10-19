# Native samenstelling en publicatie van voorbereide sjabloonexport

Vier referenties roepen de oorspronkelijke bladhulpfuncties, afbeeldingsinvoeging
en actieve volledige profieltekenaar aan. Zij bevatten tien gevulde vakken,
lege posities, dubbele zichtbare PS2-labels, twee sjabloonbronnen, uitbreiding
naar een extra blad, zes complete profielen en twintig blijvende RGBA-PNGs.
Automatische/handmatige schalen, omkering, referentiepunten, BGT-oppervlakken,
handmatige maaiveldtekst/kleur, markerbegrenzing en beide botsingsmodi komen voor.
De referentie gebruikt ezdxf 1.4.4 en de actieve V9-patch; geen Rust-uitvoer.

Onafhankelijke readback vergelijkt alle niet-default DXF-velden, geometrie,
attributen, oorspronkelijke objectvolgorde, blad-/titel-/vensterinstellingen,
stijlen/lagen en eenheden. LEADER-verwijzingen worden naar de daadwerkelijke
annotatie-index opgelost. Een bronbehoudcontrole beschermt overige ruwe tags
en headervariabelen. Audit mag de bestaande bronbaseline niet vermeerderen.
Nieuwe SEQEND-eigenaars worden tevens in de ruwe DXF gecontroleerd.

Iedere nieuwe IMAGEDEF verwijst naar een bestaande PNG in de blijvende map van
die exportpoging. Zowel exacte PNG-bytes als alle RGBA-pixels worden vergeleken.
Willekeurige absolute bundlepaden worden alleen voor de geometrievergelijking
vervangen door hun indexgebonden bestandsnaam; hun echte locatie wordt apart
geverifieerd. Oude bronafbeeldingspaden worden niet genormaliseerd.

Rust-tests controleren behoud van vorige tekeningen/assets na ontbrekende of
beschadigde PNG, ongeldige datum, late profiel-/tekstfout, annulering en panic
na het definitieve schrijven. Een geslaagde vervanging bewaart eerdere
bundles; opgeslagen tekeningkopieën werken nog als invoer-PNGs verwijderd zijn.
Een uitvoerpad naar het invoersjabloon of een invoer-PNG wordt vóór wijziging
geweigerd. De echte CLI wordt gecontroleerd met relatieve invoerpaden en bestaande uitvoer.

    python tools/generate_template_export_fixtures.py
    cargo build --release --manifest-path rust/Cargo.toml --example export_template_assemblies
    rust/target/release/examples/export_template_assemblies.exe NIEUWE_MAP
    python tools/verify_native_template_assemblies.py NIEUWE_MAP rust/target/release/sleufbase-rust.exe

Dit contract begint **na** raster-/serverdatavoorbereiding. GeoTIFF-/kaart-/
fotorendering, automatisch ophalen en overzichtsgeometrie afleiden, logo-/
contactherstel en de complete native exportdialoog blijven migratiewerk.
De API accepteert voorbereide overzichtsgeometrie; zij verzint geen ontbrekende
data en vervangt geen exportafbeelding door een kaartpreview.
