# Integratiecontrole 24 oktober 2026

Deze controle geldt voor de geïmplementeerde onderdelen. De volledige
acceptatievoorwaarden in [PARITY.md](PARITY.md) blijven open totdat zij elk met
passend bewijs zijn afgedekt; dit rapport verklaart de port niet compleet.

- GNU Windows: `cargo test --all-targets --locked --offline --jobs 2` met
  `--manifest-path rust/Cargo.toml`: **631 tests geslaagd, nul fouten**, verdeeld
  over 138 targets inclusief de voorbeeldprogramma's zonder eigen tests.
- Dezelfde targets zijn met Clippy gecontroleerd. De controle voltooit met
  stijlwaarschuwingen; er is geen claim van een waarschuwingvrije codebasis.
- Vier oorspronkelijke generators produceren 16 verse bestanden bytegelijk
  aan de AI-, profiel-, overzichts- en virtuele-kaartreferenties. App, accounts,
  oorspronkelijke instellingen, GPU en modeldownloads worden niet gestart.
- Acht echte zelfstandige overzichtsexports en acht gepaarde sjabloonexports
  vergelijken elk 91 volledige oorspronkelijke overzichtsobjecten en twaalf
  affine IMAGEs met bytegelijke TIFF-kopieën. Vergelijking omvat tekenvolgorde,
  laagkleuren/lijndiktes, effectieve font-/stijlwaarden, beeldreactors, scheiding
  van profielstaten en afwezigheid van nieuwe DXF-auditreparaties.
- Acht bestaande productie-workerexports slagen opnieuw: normaal, reverse,
  alleen beeld, geordende varianten, ontbrekende metingen, gekozen meetpunten,
  TIFF-opmerkingen en rotatie. Geschreven profielen en TIFF-pixels worden met
  de oorspronkelijke route vergeleken; bronallocaties keren terug.
- Vijf voorbereide exports leveren acht volledige profielen en 26 blijvende
  PNGs. De echte executable slaagt met manifestrelatieve invoerpaden, automatisch
  discipline-Excel en weigering om bestaande uitvoer te overschrijven. De
  broncontrole accepteert precies het oorspronkelijke contactherstel; een
  bewust gewijzigde andere sjabloontekst wordt afgekeurd.

Centrale review herstelde onder meer het starten van een tweede modelproces na
een tijdelijke gereedheidstime-out, behoud van oude AHN-modetekst, een AI-preview
bij een andere bronkeuze, dubbele metadataquotering, onbegrensd TIFF-kopiëren,
niet-herhaalbare DXF-klassevolgorde en afwijkende nieuw aangemaakte overzichtslagen.
Annulering, bronbescherming, terugkeer van eigendom en strikte projectinstellingen
hebben gerichte regressieproeven.

De twee afsluitende spaties in `cadastral_overview_base.txt` zijn oorspronkelijke
DXF-lijntypebeschrijvingen. Deze gegevens blijven bytegetrouw, net als de DXF-
referenties; zij worden uitgezonderd van de broncodecontrole op witruimte.

Echte GPU-/vLLM-voorspellingen, volledige opruiming van model-subprocessen,
dienst-/accountvarianten, representatieve projecten, AutoCAD-acceptatie,
resterende kaartbedieningen, volledige Python-terugconversie, prestatieacceptatie
en installatie/update/rollback blijven afzonderlijk te bewijzen. De zelfstandige
overdracht naar `LukaStuurman/SleufBase-RUST` volgt pas na de volledige port en
alle leveringsvoorwaarden.
