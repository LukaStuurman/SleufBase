# Bladnummering en overzichtsviewports

`tools/generate_template_layout_fixtures.py` roept de oorspronkelijke
`_used_template_layout_names`, `_update_template_layout_page_numbers` en
`_configure_template_wireframe_viewports` aan via een geïsoleerd bronpakket.
Geen app-opstart, accountgegevens, instellingen of netwerk zijn nodig.
Referentie: Python 3.11 / Unicode 14 en ezdxf 1.4.4.

Twee documentreeksen leveren 20 opeenvolgende toestanden. Hetzelfde native
Document blijft in geheugen tussen alle wijzigingen en herhalingen. Acht
vakken vormen een blad; lege vakken behouden hun positie. De voorbeelden
omvatten nul/één/meerdere gebruikte bladen, leeggehouden bladen, een ontbrekend
blad, eerste/tweede INSERTs, ontbrekende titelblokken/viewports, meerdere KADER-
viewports en een lagere viewport op een andere laag. Schaalgrenzen omvatten
exacte passing en net te grote begrenzingen voor de zes oorspronkelijke
standaardschalen. Ook het echte meegeleverde sjabloon is onderdeel van de reeks.

De verwachte snapshot bevat alle bron-VIEWPORTvelden en alle ATTRIB-teksten.
`tools/verify_native_template_layout.py` vergelijkt onafhankelijke CAD-readback,
alle overige oorspronkelijke tags en de auditbaseline. Alleen bedoelde
viewportvelden en titeltekst mogen veranderen. Ontbrekende view-tags worden
vóór XDATA ingevoegd; herhaalde bewerkingen behouden alle andere recordbytes.
Annulering, niet-eindige waarden, omgekeerde bounds en numerieke overloop in
een later blad mogen geen gedeeltelijke eerdere bladwijzigingen publiceren.

`unicode-reference.json` controleert vijf papierlayouts voor geometrie,
IMAGE-plaatsing en titelvelden via andere schrijfwijzen: Straße/STRASSE,
Griekse sigma, dotless ı/I en afzonderlijke K-/Kelvin-tekenlayouts. ezdxf gebruikt
`upper()` voor layoutnamen, terwijl lagen en blokken `lower()` gebruiken.
De juiste ruimte-eigenaar, beeldgeometrie, titelvelden, handles en CAD-audit
worden onafhankelijk gecontroleerd.

Uitvoeren naar een verse uitvoermap:

```powershell
python tools/generate_template_layout_fixtures.py
cargo run --manifest-path rust/Cargo.toml --example export_template_layout -- C:/temp/layout-audit
python tools/verify_native_template_layout.py C:/temp/layout-audit
```

Bladuitbreiding, legenda’s, rastervoorbereiding, de volledige native
sjabloonexportdialoog en praktijkacceptatie blijven afzonderlijke voorwaarden.
