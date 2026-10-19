# Sjabloonvoorbereiding: fonts, lagen en Unicode-symbolen

`tools/generate_template_preparation_fixtures.py` roept de oorspronkelijke
`_setup_text_styles` en `_ensure_template_profile_layers` aan via een geïsoleerd
bronpakket, zonder app-opstart, accountgegevens of netwerk. Python 3.11 /
Unicode 14 en ezdxf 1.4.4 zijn het referentiecontract.

Vier documentreeksen leveren 15 opeenvolgende toestanden: bestaande aangepaste
fonts/lagen; ontbrekende kleur-/lijngewichttags op de laatste laag; lagen en
blokken uit eerdere tekenbatches; het echte meegeleverde sjabloon. De native
reeks houdt hetzelfde document in geheugen tussen alle stappen. Herhaalde
voorbereiding is byte-identiek, ook na kleurwijzigingen en terugkeer naar de
bronwaarde. Een aparte regressie wijzigt een door de rasterbatch gemaakte laag.

De referentie bevat alle laag-/stijlvelden en nieuwe geometrie/attributen.
Serialisatie/readback in Python voegt geldige standaard materiaal-/plotstijl-
verwijzingen toe; de referentiesnapshot ondergaat dezelfde stap. Bestaande
records worden daarnaast tag voor tag gecontroleerd: alleen aangevraagde
kleur/lijngewicht, tabeltellingen en HANDSEED mogen veranderen. Auditfouten en
extra reparaties zijn niet toegestaan; de bestaande proxyreparatie 202 van het
echte sjabloon blijft de bronbaseline.

DXF-symboolnamen gebruiken ezdxf `make_table_key()` / Python `lower()`, inclusief
contextafhankelijke Griekse sigma. `Straße` en `STRASSE` zijn verschillende
symbolen, evenals finale/mediale sigma. Unicode-blokken met afzonderlijke
ATTDEFs controleren dat de goede definitie wordt geplaatst. `symbol-case.json`
bevat aantal en hash van 128.358 officiële teken-/contextprobes; de Rust-test
bouwt dezelfde invoer onafhankelijk op. `tools/generate_dxf_symbol_case.py`
bevriest de Unicode-14-lower-/contexttabellen, inclusief niet-BMP-tekens.

De oorspronkelijke aanroep voor nieuwe puntlagen is gecorrigeerd: expliciete
`color`/`lineweight`-argumenten voorkomen dat ezdxf zijn standaardwaarden over
gelijknamige dxfattribs schrijft. Twee Python-regressies bewaken die fout.

Uitvoeren op een verse uitvoermap:

```powershell
python tools/generate_dxf_symbol_case.py
python tools/generate_template_preparation_fixtures.py
cargo run --manifest-path rust/Cargo.toml --example export_template_preparation -- C:/temp/preparation-audit
python tools/verify_native_template_preparation.py C:/temp/preparation-audit
```

Deze controles bewijzen de voorbereidende documentbewerkingen. BGT-resolutie,
pagina-/rastervoorbereiding, de complete native exportdialoog en praktijkacceptatie
blijven afzonderlijke voorwaarden voor volledige functiepariteit.
