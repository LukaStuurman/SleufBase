# Legenda- en titelblokherstel

Negen vaste referenties uit de actieve Python-helper met ezdxf 1.4.4:
het echte sjabloon, zeven affine varianten en een R2007-document.
De bron is Blad1; een doelblad met enig INSERT krijgt geen nieuwe legenda.
Blad2 is actieve papierruimte; dubbele namen, ontbrekende layouts, gedeelde
afbeeldingsdefinities en nieuwe afbeeldingsreactoren zijn opgenomen.

433 nieuw opgebouwde attributen controleren signed/nonuniform XYZ-schalen,
rotatie, ATTDEF-basis, OCS, spiegeling, schuinte, breedte/hoogte, uitlijning,
veldflags en tagwaarden. Handmatig verschoven bronattributen worden opnieuw
vanuit ATTDEF opgebouwd. Dubbele tags volgen de laatste waarde; ontbrekende
waarden gebruiken de definitietekst. APPDATA met geometrieachtige codes mag
de transformatie niet beïnvloeden. Multiline ATTDEF-regeneratie is nog niet
ondersteund en geeft een fout vóór wijziging van het oorspronkelijke document.

De onafhankelijke readback vergelijkt alle niet-default DXF-velden, geometrie,
objectgrafen en titelmetadata; audit mag geen aanvullende reparaties vereisen.
Hoeken worden modulo 360 vergeleken; weggelaten schema-defaults en numerieke
defaultverschillen tot 1e-12 worden gelijk behandeld. Overige getallen hebben
een tolerantie van 1e-9. Ongewijzigde bronrecords worden afzonderlijk vergeleken.
Nieuwe SEQEND-eigenaars worden ook in de ruwe DXF gecontroleerd, omdat ezdxf
bij het koppelen van actieve papierruimte eigenaars kan normaliseren.

Regenereren en onafhankelijk controleren vanuit de repository:

    python tools/generate_template_legend_fixtures.py
    cargo run --release --manifest-path rust/Cargo.toml --example export_template_legends -- NIEUWE_MAP
    python tools/verify_native_template_legends.py NIEUWE_MAP

Dit bewijst deze teken-/metadatafunctie; de volledige native exportbediening
en CAD-praktijkacceptatie blijven afzonderlijke voorwaarden.
