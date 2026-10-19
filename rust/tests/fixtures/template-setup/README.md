# Samengestelde sjabloonvoorbereiding

Vier oorspronkelijke reeksen met elf opeenvolgende toestanden combineren
bladuitbreiding, fonts/lagen, legendaherbouw, tekening- en rastereenheden,
schaallijst, projectnaam, tekenaar, datum, bladnummering en overzichtsvensters.
De bron bevat het echte sjabloon en drie affine varianten met lege vakken,
nieuwe/bestaande rasterinstellingen en een ontbrekende INSUNITS-headervariabele.
Schaalobjecten met ontbrekende velden, gedeelde aliases en verwijderde entries
controleren het oorspronkelijke behoud van overige objectdata.

Readback met ezdxf 1.4.4 vergelijkt alle blad-/objectvelden, geometrie,
afbeeldingsgrafen, styles/lagen, eenheden en alle schaalobjecten. Onverwante
ruwe bronrecords en headervariabelen worden afzonderlijk op behoud getoetst. Nieuwe
SEQEND-eigenaars worden tevens vóór lezernormalisatie in de ruwe DXF getoetst.
De bestaande auditbaseline van het echte sjabloon mag niet toenemen.

De Rust-regressies controleren idempotentie, dubbele bladnamen, annuleren en
panic na de voorbereidende stappen, ongeldige opties en opnieuw proberen.
Een vervolgtest voegt tekenwerk en een PNG toe aan een nieuw aangemaakt blad,
met hergebruik van stijlen, lagen en rasterinstellingen. Op Windows wordt de
automatische lokale datum zonder expliciete datumoptie gecontroleerd.

Regenereren en controleren vanuit de repository:

    python tools/generate_template_legend_fixtures.py
    python tools/generate_template_setup_fixtures.py
    cargo run --release --manifest-path rust/Cargo.toml --example export_template_setup -- NIEUWE_MAP
    python tools/verify_native_template_setup.py NIEUWE_MAP

De voorbereiding vervangt het document uitsluitend als alle stappen slagen.
De volledige native exportbediening, raster-/foto-opbouw en overige
CAD-kopieertypen zijn nog niet gereed.
