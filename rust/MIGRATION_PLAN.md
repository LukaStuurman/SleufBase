# Werkplan volledige native Rust-migratie

Doel: volledige functiepariteit, aantoonbaar betere prestaties waar mogelijk,
en alle gevonden regressies herstellen. Geen claim van gegarandeerde bugloosheid.
`PARITY.md` blijft de acceptatiechecklist; een compilatie is geen productacceptatie.

Uiteindelijke bestemming op gebruikersverzoek: de zelfstandige Rust-port en
uiteindelijke Rust-release gaan naar [LukaStuurman/SleufBase-RUST](https://github.com/LukaStuurman/SleufBase-RUST).
Deze repository is op 22 oktober 2026 gecontroleerd: er waren nog geen branches
of tags. De huidige migratietak bewaart het ontwikkelwerk en de oorspronkelijke
Python-referenties. Overdracht/publicatie vervangt geen van de volledige
functie-, integriteits-, migratie-, praktijk- of leveringsvoorwaarden hieronder.

Aanvullende gebruikerswens: projectnaam invoeren in het DXF-sjabloonexportvenster
en automatisch de lokale exportdatum op ieder gebruikt blad invullen.

Scopewijziging op gebruikersverzoek van 22 oktober 2026: de Cyclomedia-
kaartachtergrond en het StreetSmart-streetviewvenster vervallen voor de Rust-versie.
Ze tellen niet meer mee als ontbrekende functies of releasevoorwaarden. Oude
achtergrondvoorkeuren worden bij migratie naar de standaard PDOK omgezet;
oorspronkelijke Python-referenties blijven als bewijs ongewijzigd.

Broncontrole van het **actieve** exportcontract: `__init__.py` installeert V9,
maar niet `template_dynamic_profile_leader_patch`. De expliciete regressies in
`tests/test_profile_leader_v0310_behavior.py` vereisen de oorspronkelijke
attribuutblok-/LEADER-methoden. Die route bepaalt productiepariteit. De eerder
geporte donor/parameterhelpers blijven optionele bouwstenen; prototypekopieÃ«n uit
een uitgeschakelde patch zijn geen voorwaarde voor de actieve profielworkflow.
De gedeelde ATTDEF-/blokondersteuning wordt ook door de actieve route gebruikt.

## Volgorde en bewijs

1. **Gegevensintegriteit en projectlevenscyclus**: atomair opslaan/exporteren,
   harde invoerlimieten, volledige geometrievalidatie, onopgeslagen werk beschermen,
   oude achtergrondresultaten isoleren bij projectwissels, hersteltesten.
2. **Bestaande projectformaten**: echt Python-serialisatiecontract vastleggen,
   verliesvrije conversie inclusief metadata/bronpaden, ontbrekende assets melden,
   oorspronkelijke bestanden behouden. Onbekende onderdelen niet stilzwijgend weggooien.
3. **CAD en profielen**: ASCII/binary DXF, bloktransformaties/attributen/XDATA,
   lokale MarXact-dwarsprofielen en geclipte weergave, volledige sjablonen en assets.
   Geometrische vergelijkingen met Python en echte CAD-bestanden zijn vereist.
4. **Raster en kaarten**: begrensde strip-/tegeldecodering, kleur/alpha/hoogte,
   pyramides, WMS/WMTS/OSM en offline caches; ruimtelijke juistheid en geheugen meten.
5. **Externe diensten**: KickTheMap browserauth/herstel/fallbacks,
   AHN-profielen en AI-workflows. Native netwerkcode met lokale foutscenario-tests;
   accountgebonden acceptatie apart aantonen met een beschikbare sessie.
6. **Dagelijkse bediening en instellingen**: alle menu's, shortcuts, drag/drop,
   objectbewerking, disciplines/Excel, configuratie en foutmeldingen.
7. **Prestaties en levering**: herhaalbare datasets en benchmarks met tijd/geheugen,
   Windows portable/installer/update/rollback, daadwerkelijke acceptatie en signing.

## Voortgang 22 oktober 2026: begin- en referentiepunten

- De GeoTIFF-lijst selecteert nu de laag voor referentie plaatsen/verplaatsen,
  gemeten beginpunt kiezen, alle referenties wissen en alle gekoppelde
  KickTheMap-beginpunten laden. Werkelijke kaartklikken gebruiken de bestaande
  viewporttransformatie, inclusief zoomen, verschuiven en rasterrotatie.
- Oorspronkelijke punthelpers worden zonder appopstart op synthetische invoer
  uitgevoerd: 96 gevallen en 384 klikselecties, inclusief Unicode-markernamen,
  ontbrekende Z, samenvallende punten en de grens van 0,5 m. Beide sjabloonplanners
  ontvangen de huidige bronpunten; oude vakmetadata kan ze niet overschrijven.
- Raw RD-punten en overige metadata blijven in het project. Oude native projecten
  zonder dit veld blijven leesbaar; Python-import past de laagmetadata toe en
  lost het lokale objectdatapad op. Hele wijzigingsbatches worden vooraf op
  bronidentiteit, coördinatenstelsel en metadata gecontroleerd. Een fout wijzigt
  geen eerder punt. Wissen verwijdert ook verouderde vaksnapshots.
- De oorspronkelijke BRT-wegmiddellijnen en BGT-terreinranden zijn aangesloten,
  met dezelfde RD-vraag, uitsnedemarge, geometrievolgorde en paginering.
  48 oorspronkelijke batches/96 requests reproduceren bytegelijk. Begrensde
  reads, gedeelde verbindingen, annulering, veilige vervolg-URL's en een gedeelde
  24-entry-/2-miljoen-puntencache voorkomen onbegrensd ophalen/kopiëren.
  Een openbare live proef levert 20 wegpaden/91 punten en 72 terreinranden/1337
  punten in RD; herhalen deelt dezelfde cachegeometrie zonder nieuwe aanvraag.
- Bulkvoorbereiding gebruikt geladen of lokale objectdata, of de bestaande
  ingelogde downloadwachtrij in groepen van maximaal 500 jobs. Er blijft maximaal
  één lokaal gedecodeerde dataset staan. Metadata is begrensd op 2 MiB per laag
  en 16 MiB per project/export. Bij bronuitval volgt een melding en de
  oorspronkelijke richtingsterugval; de gebruiker kan offline werken.
- Echte egui-/workerproeven controleren klikselectie, bronallocaties,
  annulering, projectwissels en geen wijziging bij fouten. De volledige reeks
  slaagt met 498 tests, inclusief 53 desktoptests. De geoptimaliseerde app en
  puntdriver bouwen; 129 verse oorspronkelijke gevallen/516 selecties en 62
  volledige contextbatches slagen binnen 0,00000002 m tegen die driver.
  Zes echte workerexports met 14 blijvende PNGs en zes Excel-overzichten slagen
  bij onafhankelijk herlezen. Het gekozen punt overrult bewust verouderde
  vakmetadata; alle 52 geschreven profielobjecten en alle TIFF-pixels komen
  overeen met de oorspronkelijke uitvoer. Datum/projectnaam blijven correct.
  Virtuele referentiebewerking, volledige legacy-instellingen, accountvarianten
  en praktijk-/releaseacceptatie blijven open volgens PARITY.md.

## Voortgang 22 oktober 2026: zelfstandige kaartenexport

- De oorspronkelijke menuactie Export kaarten heeft nu een eigen native venster:
  alle geladen TIFFs in bronvolgorde, A4-PNG, schaal/DPI, PDOK/OSM/offline,
  zichtbare CAD-keuze, voortgang/annulering en opgeslagen/migreerbare voorkeuren.
  De bestaande exportworker neemt de echte bronvectors in eigendom en geeft ze
  terug voordat een uitgestelde projectwisseling doorgaat. Export wijzigt het
  project niet. De Kadaster-keuze blijft herkenbaar bewaard maar is
  nog niet uitvoerbaar; volledige provider/cache-/rasterbediening blijft open.
- De zelfstandige oorspronkelijke route gebruikt maximaal 70% TIFF-dekking;
  sjabloonkaarten behouden 100%. RGB wordt per rij geschreven met echte pHYs-
  resolutie, zonder tweede volledige RGB-pagina. Zes oorspronkelijke PNGs met
  verschillende DPI/schaal/affine ligging leveren 2400 pixelreferenties.
- De oorspronkelijke fout met gelijke TIFF-stammen is apart bewaard: twee bronnen
  overschrijven hetzelfde PNG-bestand. Native uitvoer gebruikt unieke namen, ook
  bij Windows-hoofdletterbotsingen; bestaande legacy-PDFs worden behouden.
- Alle pagina's worden voorbereid/geflusht voordat eerdere PNGs worden vervangen.
  Late tekenfouten, annulering en bronpadconflicten behouden eerdere uitvoer.
  Publicatiefouten draaien eerdere vervangingen terug. Bij een mislukte terugzetactie
  blijven oorspronkelijke bytes en een bestemmingsmanifest in een gemelde herstelmap.
  Dit bewijst geen crash-atomaire publicatie van meerdere losse bestanden.
- De onafhankelijke pixelcontrole vond een één-pixelverschuiving en verlengde
  uiteinden bij even brede rechte kaartlijnen. De correctie volgt tekenrichting
  en Pillow-pixelgrenzen; twintig afzonderlijke oorspronkelijke strookmaskers
  controleren even/oneven breedtes en omgekeerde lijnen.
- De review vond ook dat de bestaande OSM-previewlimiet een standaard 300-DPI-
  export zou afwijzen. De aparte exportopdracht gebruikt nu het begrensde
  paginabudget en dezelfde maximaal 24 opgevraagde tegels; de previewlimiet
  blijft staan. Een volledige 3344×2316-kaart wordt uit een eigen tijdelijke
  tegelcache opgebouwd en op alle pixels gecontroleerd, zonder netwerkverkeer.
- Zie tests/fixtures/map-batch voor oorspronkelijke uitvoercontracten,
  echte GUI-/worker-/CAD-/projectwisselproeven en geoptimaliseerde runtime-audit.
  De volledige regressiereeks slaagt na de scopewijziging met 487 tests,
  inclusief 49 desktoptests; de oorspronkelijke template-/kaarttests slagen mee.
  De geoptimaliseerde app en beide kaartdrivers bouwen. Zes verse pagina's met
  gewijzigde TIFF-pixels/hoeken en 2400 nieuwe pixels slagen tegen de echte driver.
  De bestaande audit slaagt opnieuw voor 66.667 exacte RGBA-pixels, twaalf
  tekenplannen, 41.915 virtuele kaartpixels en 53.095 pixels van de echte
  sjabloon-CLI met profiel/datum/projectnaam. De live PDOK-kaartproef met een oude
  Cyclomedia-voorkeur levert via de nieuwe migratie een RGB/DPI-A4-PNG; 140.570
  pixels verschillen van dezelfde witte offline kaart. De nieuwe referenties
  reproduceren bytegelijk met een andere Python-hashseed.
  Volledige instellingen, diensten, echte projecten en uiteindelijke levering
  blijven vereist volgens PARITY.md.

## Voortgang 22 oktober 2026: TIFF-adressenoverzicht

- Het oorspronkelijke overzicht is aangesloten op geladen GeoTIFFs: bestandsnaam,
  eerste adres en afstand, lege/foutregels, selectie en beide kopieeracties.
  Vernieuwen, voortgang en annuleren lopen buiten de kaartbediening. Alleen
  bronmetadata gaat naar de worker; rasterpixels en projectgegevens blijven staan.
- Locatiezoeken en adresbatches delen een begrensde werkverdeler, met per workflow
  één hergebruikte worker/client. Vervanging onderdrukt de volledige oude batch;
  sluiten en projectwisseling kunnen geen verouderde regels/status publiceren.
  Rij-/tekstlimieten en een controle op expliciete niet-RD-projecties begrenzen invoer.
- 48 oorspronkelijke regels en 144 oorspronkelijke clipboardacties controleren
  Unicode, dubbele namen, lege/mislukte aanvragen en afstandafronding. Echte egui-
  proeven controleren clipboardcommando's, projectwisseling en de providercoördinaten
  van een daadwerkelijk geladen GeoTIFF zonder bron- of pixelkopieën.
- De volledige regressiereeks slaagt met 473 tests, inclusief 47 desktoptests.
  Het Windows HTTP-testservertje herstelt nu expliciet blokkerend lezen op een
  geaccepteerde socket; de eerder timingafhankelijke test slaagt in de volledige reeks.
  Clippy levert geen nieuwe waarschuwingen en de geoptimaliseerde app/driver bouwen.
  De driver vergelijkt 81 verse complete adresregels met 243 oorspronkelijke
  clipboardcontracten; de bestaande 375 coördinaten-/38 providerproeven slagen opnieuw.
  Een echte PDOK-batch voor twee openbare proefcoördinaten levert beide adressen in
  bronvolgorde. De nieuwe referentie reproduceert bytegelijk met een andere hashseed.
  Zie tests/fixtures/raster-addresses voor de afzonderlijke contracten en audits.
- Zelfstandige kaartexport, resterende menu's/voorkeuren, AI,
  volledige project-/praktijkacceptatie en levering blijven vereist.

## Voortgang 22 oktober 2026: oorspronkelijke locatiebediening

- De echte oorspronkelijke menubedieningen zijn zonder appopstart vastgelegd in
  tests/fixtures/workflows/original.json. WORKFLOW_PARITY.md koppelt de resterende
  gebruikersacties aan native routes. De broncontrole onderscheidt de acht oude
  DXF-kaartprimitieven van nieuwe native CAD-uitbreidingen; sjabloonbehoud blijft
  een afzonderlijk ruimer contract. Alle bestaande releasevoorwaarden blijven open.
- Locatiezoeken is gekoppeld aan de desktop: debounced PDOK-suggesties, RD/GPS,
  Unicode-cijfers, Ctrl+L en pijl/Enter/Escape, oorspronkelijke typegebonden schaal,
  tijdelijk markeren, lege-kaartnavigatie en luchtfoto. De werkelijke viewport na
  verschuiven/zoomen bepaalt de volgende provideruitsnede. Zoekresultaten, marker
  en focus worden bij projectwisseling gewist zonder de projectdata te wijzigen.
- Eén client/worker houdt één actieve, één laatste wachtende en één gereedstaande
  zoekopdracht bij. Duizend vervangingen, annulering en verouderde/gesloten resultaten
  zijn gecontroleerd. Echte lokale HTTP-proeven toetsen statusretry en bodylimieten.
  Een wachtende socket blijft begrensd door zijn 20-seconden-timeout.
- 287 oorspronkelijke coördinatenvragen, alle 66 Unicode-14-cijferfamilies, 36
  zoekantwoorden, twee reverse-antwoorden en zes kaartschalen zijn vastgelegd.
  GPS gebruikt dezelfde gridloze zevenparameterprojectie met pure Rust; pyproj
  bepaalt het onafhankelijke verwachte resultaat binnen 0,002 m voor navigatie.
  Drie oude GPS-volgordefouten zijn bewaard naast de correctie. Echte egui-proeven
  controleren lege kaart, schaal, verschuiving, toetsenbord en projectwisseling.
- De volledige regressiereeks en gerichte eindcontroles dekken 466 geslaagde tests,
  inclusief 44 desktoptests; Clippy heeft geen nieuwe waarschuwingen. De native
  geoptimaliseerde app/locatiedriver zijn gebouwd. 375 verse coördinatenvragen en
  38 providerantwoorden slagen met maximaal 0,000000159 m horizontaal verschil
  tegenover de oorspronkelijke pyproj-route. De echte PDOK-search/reverse-proef
  levert zes/één geldige resultaten. Beide referenties reproduceren bytegelijk
  onder een andere hashseed. Dit vervangt geen volledige project-/praktijkacceptatie.
- Cargo.lock legt nu de volledige dependencyset vast; Windows CI gebruikt --locked.
  De native projectiebibliotheek heeft geen C/Python-runtime nodig. De bijbehorende
  notices worden met EXE/portable preview meegeleverd en via --licenses getoond.
- Reverse-lookup is geport en is inmiddels gekoppeld aan TIFF-adressen/kopiëren
  (zie de voortgang hierboven). Zelfstandige kaartexport, resterende menu's/
  voorkeuren, AI, volledige projectacceptatie en levering blijven nodig.
  Zie tests/fixtures/location.

## Voortgang 22 oktober 2026: gewone MTEXT-kleuren

- Inline ACI/RGB-kleuren lopen nu van ASCII/binary DXF via opgeslagen contouren
  naar geneste blokken, canvas en beide sjabloonkaartpaden. Alle 257 ACI-rollen,
  expliciet zwart/wit, groepsherstel, decoraties/breukstrepen en laag-/blokkleuren
  worden op 141 oorspronkelijke instanties/1607 gekleurde contouren vergeleken.
- Lettercontouren en bronmetadata blijven gedeeld. Kleine kleurgrenzen behouden
  één selecteerbare bron per instantie; elke bloktransformatie berekent alleen
  haar eigen puntgrenzen. Tekenen leent paden zonder tijdelijke geometriekopieën.
  Alle segmenten delen dezelfde punt-/curve-/annuleerlimieten.
- Projectopslag werkt zonder bronfonts. Oude uniforme caches blijven leesbaar;
  ongeldige kleurbereiken vervangen geen bestaand project. Zichtbaarheid behoudt
  gedeelde bron-/scènecaches. Echte egui-streken, geknipte sjabloonkaarten,
  desktop-exportvoorbereiding en geschreven DXF-true-colors worden gecontroleerd.
- De onafhankelijke oracle voert alleen de pure kleurfunctie uit dxf.py uit,
  zonder pakketopstart, met de eerder vastgelegde zwart-/laag-/BYBLOCK-correcties.
  Bronbestanden reproduceren bytegelijk onder een andere hashseed. De runtimeproef
  wijzigt fysieke fontbytes en bronposities en gebruikt de geoptimaliseerde reader.
  Twee verse ASCII/binary tekeningen vergelijken 282 instanties/3214 gekleurde
  contouren; de eerdere 276 MTEXT-instanties en 270 TEXT-contourgevallen slagen
  opnieuw. In totaal slagen 456 Rust-tests, inclusief 40 desktoptests; Clippy
  voegt geen nieuwe waarschuwingen toe. De geoptimaliseerde app en drie betrokken
  auditdrivers zijn gebouwd. Dit bewijst geen volledige product-/praktijkpariteit.
  Zie tests/fixtures/cad-mtext-paint voor bewijs en resterende beperkingen.
- Achtergronden/frames, volledige kolom-/embedded-/STYLE-bronnen, TTF-vlakvulling
  en echte CAD-/project-/UI-acceptatie blijven open. Ook alle overige voorwaarden
  in PARITY.md blijven vereist vóór overdracht als voltooide SleufBase-RUST.

## Voortgang 17â€“18 oktober 2026

- MarXact-richting, automatische grensmiddens en absolute rotatie/reset geport.
  Tien Python-referentiegevallen, zeventig rotaties, project- en DXF-regressies.
- Commit `1de19f4`: 66 Rust-tests, Clippy, Quality Gate en Windows-builds geslaagd.
- Gegevensintegriteit: atomaire no-clobber exports, validatie alle kaartgeometrie,
  begrensd lezen/schrijven, onopgeslagen werk beschermen en netwerkresultaten per project isoleren.
  76 Rust-tests groen; echte crash-/herstelacceptatie en hersteloverzicht blijven open.
- Python v1-projectcontract vastgelegd met de oorspronkelijke gevalideerde serializer.
  Native import/conversie, volledig bronarchief, assetmeldingen en metadata-/CLI-regressies
  toegevoegd; 81 Rust-tests groen. UI-/sjablooninstellingen nog niet functioneel overgenomen,
  geen Python-formaatexport of volledige acceptatie met echte projecten.
- Lokale MarXact-dwarsmarkeringen geport: 240 Python-referentiegevallen,
  kaartcache en dezelfde geometrie in basis-DXF. Volledige profielen, pixelmaskers
  en kleureninstellingen blijven open. 85 Rust-tests groen.
- Reproduceerbare release-kernbenchmark met 100.000 objecten en CI-artifact:
  twintig kaartinvoerupdates vermijden 10.000.080 allocaties; validatie vermijdt
  2.000.000 tijdelijke allocaties (zie `benchmarks/README.md` voor grenzen).
- Rasterdecoder verwerkt strips/tegels naar begrensde previews op Ã©Ã©n achtergrondthread,
  ondersteunt meer samples/kleurtypen en verdeelt een geschat textuurbudget over lagen.
  47 rasterfixtures (Pillow + numerieke verwachtingen) en 93 Rust-tests groen.
  Planar RGB16 corrigeert een aangetoonde Pillow-bytefout; NoData/nonfinite transparant.
  Releasebenchmark op 16,8 miljoen RGB8-pixels: circa 2,3Ã— sneller en 86% lagere
  procespiek dan volledige Pillow-conversie, bij gelijke previewpixels. Overviews,
  viewportresolutie en volledige raster-/gebruikersacceptatie blijven open.
- Geneste MULTILEADER-labels lezen echte pijllijnen en MTEXT afzonderlijk.
  Een echte synthetische ezdxf R2018-serialisatie is met de oorspronkelijke
  Python-parser vergeleken (11 labels, 3 inserts, 1 gesloten en 1 open contour).
  Open contouren worden terecht overgeslagen; labeltekst volgt eerste match en
  numerieke hoogteprefix. Oudere overlays zonder gesloten-vlag blijven leesbaar.
  97 Rust-tests groen; bloktransformaties en sjablonen blijven open.
- ASCII/binary DXF-invoer deelt nu Ã©Ã©n geometriepad, leest tags incrementeel,
  gebruikt HEADER-codepagina/UTF-8 en weigert beschadigde/afgebroken bestanden.
  Vier ezdxf ASCII/binary-paren (R12/R2000/R2018) en binaire geneste MarXact
  vergeleken; scalaire/XDATA-chunk- en afbreekregressies toegevoegd.
  105 Rust-tests groen. Geen volledige attributen/XDATA, OCS/bulges,
  bloktransformaties of sjablonen; bronbuffer en uitvoergeometrie blijven in RAM.
- CAD-palet/true-color/laagstatus en INSERT-attributen/XDATA geÃ¯mplementeerd,
  met laagkeuzes en een gecachte objectinspectie in de UI. 266 Python-orakels
  (alle ACI's), ASCII/binary-vergelijking en egui-paintregressie.
  Expliciet zwart en true-color op lagen corrigeren aangetoonde Python-fouten.
  Native formaat v2 voorkomt verlies van nieuwe CAD-data in oudere builds;
  v1 blijft leesbaar, onbekende native objectvelden geven een fout, het originele
  Python-bronarchief blijft open/verliesvrij. 115 Rust-tests groen.
- Geneste BLOCK/INSERT/MINSERT, OCS, bulges, CIRCLE/ARC en ELLIPSE ondersteunen
  nu schaal/spiegeling/rotatie en WCS-koordtolerantie na de volledige transformatie.
  72 ezdxf-plaatsingen en 84 dichte WCS-primitieven vergeleken in ASCII/binary;
  fixtures regenereren byte voor byte. Gedeelde bronmetadata/geometrycache,
  gecachte zichtbare grenzen en expliciet herladen/herkoppelen van een DXF-bron.
  Native v3 bewaart blokken/curveparameters, leest v1/v2 zonder bronwijziging en
  bouwt afgeleide caches opnieuw. 125 Rust-tests groen, inclusief polyface-indexen,
  recursie/budgetfouten, 10.000 metadata-delende instanties en mislukte herkoppeling.
- MarXact-labels worden ruimtelijk geÃ¯ndexeerd met gedeelde bronnen en deduplicatie
  van gelijke XY-posities. De Python-tievolgorde corrigeert een native afwijking;
  extra buurcellen herstellen Python-missers op ties-to-even-celgrenzen.
  10.018 onafhankelijke referentievragen, 36 expliciete afrondingscorrecties,
  100.000 dubbele labels en zeer grote coÃ¶rdinaten getest. 128 Rust-tests groen.
  Releasekernmeting: indexopbouw plus 20.000 vragen circa 63Ã— sneller dan de oude
  native lineaire scan op 20.000 verspreide labels; geen volledige importmeting.
- Hoofdletterongevoelige Unicode-laagverwijzingen behouden nu de juiste kleur,
  zichtbaarheid en blokovererving zonder bronnaamverlies of normalisatie per frame.
  Elf ezdxf-referenties in ASCII/binary; opslag, cache/snapshot, oude conflicterende
  varianten en bronherladen bij gewijzigde schrijfwijze getest. 132 Rust-tests groen.
- SPLINE/NURBS met controlepunten, positieve gewichten en CAD-fitpunten geport,
  inclusief natuurlijke/clamped eindvoorwaarden, ongeklemde/periodieke curves,
  bronparameters en gescheiden paden. BÃ©zier-hullafvlakking na WCS-transformatie,
  hergebruikte spanbuffer en lineaire tridiagonale fitoplossing.
  Twintig ezdxf-bronnen, 60 wereldcurves/63 paden, graad 1/3/5/11, opslag,
  numerieke fouten, budgetten en 20.000 fitpunten vergeleken/getest.
  Exacte JSON-floatrondgangen herstellen een aangetoonde precisiefout bij openen;
  grote getallen/decimalen in het Python-archief worden niet naar f64 afgerond.
  139 Rust-tests groen; negatieve/nulgewichten en volledige CAD-acceptatie open.
- Lokale invoer loopt via Ã©Ã©n begrensde achtergrondworker: native openen/herstel,
  Python-import, DXF toevoegen/herladen/herkoppelen, MarXact, jobFeatures en
  GeoTIFF-metadata, inclusief bestanden uit live downloads. Voorbereiding van
  CAD-zichtbaarheid en project-/MarXact-markers gebeurt vÃ³Ã³r toepassing.
  Voortgang/annuleren, behoud van huidig werk bij fouten, batches met losse
  fouten en projectgeneraties zijn gekoppeld aan de echte UI. Nieuwe regressies
  blokkeren de worker doelbewust terwijl een egui-frame Escape verwerkt of een
  nieuw project begint; late resultaten veranderen het huidige project niet.
  Gedeelde invoercontrole begrenst werkelijke bytes, corrigeert de oude
  jobFeatures-groeirace en herhaalt onderbroken systeemlezingen. 151 Rust-tests
  groen. Annuleren is coÃ¶peratief: Ã©Ã©n systeemaanroep, JSON-/geometrie-/cachefase
  kan nog aflopen; opslaan/export/back-ups blijven synchroon. Geen totale
  geheugen-/rendergarantie of volledige workflowacceptatie.
- CAD-vlakken en geÃ¯ndexeerde polyface-/polygon-/MESH-controlekooien geport,
  inclusief verborgen randen, SOLID/TRACE-OCS en puntvolgorde, sluitrichtingen,
  n-gons, expliciete randen/vouwgewichten en geneste bloktransformaties.
  Gedeelde brondata en eenmaal gededupliceerde losse randpaden; native herstel
  herbouwt verouderde randen en bewaart vertex-/vlakmetadata en parameters.
  56 ezdxf-gevallen/278 zichtbare randen in ASCII/binary; alle 3DFACE-maskers,
  native opslagfouten, budgetten, een 100 Ã— 100 raster en echte egui-randen getest.
  Fixtures regenereren byte voor byte. 158 Rust-tests groen. Vlakvulling,
  subdivisie, gladde oppervlakken, dikte-extrusie, afzonderlijke vertex-/vlakstijlen
  en MESH-subobjectoverrides blijven open; meshdata levert geen MarXact-metingen.
- Standalone KickTheMap-objectexport geport naar native R2018 met meters,
  puntlijnrichting, echte 3D-polylijnen, oorspronkelijke XDATA, laagkleuren en
  PS-labelnaam/-plaatsing/-fontstijl. GUI en CLI gebruiken dezelfde encoder;
  de GUI leent gewone jobs en kan ook uitsluitend zichtbare kaartlagen exporteren.
  Virtuele datasetvolgorde is hersteld naar begin, stabiel gesorteerde objecten, eind.
  Eenmaal voorbereide jobgrenzen en streaming vermijden volledige uitvoer-/puntvectors.
  Veertien Python-documentorakels, 87 Unicode-labelgevallen, opslag-/fout-/f64-tests
  en onafhankelijke ezdxf-audit van native uitvoer, echte CLI en aanvullende lagen.
  164 Rust-tests groen. Fout bij eerste laagkleur in de Python-aanroep gecorrigeerd.
  Releasebenchmark, exact dezelfde 61.846.263 bytes/CRC: circa 64 MiB versus 65 KiB
  extra getraceerde Rust-heap en circa 12% kortere encodeertijd voor 100.000 objecten.
  Geen totale RSS-/GUI-/schijf- of Python-snelheidsclaim. Alle meetrondes vastgelegd.
  UI-regelopties, bron-/groepsfilters, exportvoortgang/annuleren, volledige sjablonen,
  profielen en rasterbundels blijven open; echte volledige praktijkacceptatie vereist.
- Objectparser/groepsfilters en Python-waarheid/waardeconversie verder geport:
  ontbrekend versus null, selectie vÃ³Ã³r trimmen, getallen/booleans, samengestelde
  eigenschappen in bronvolgorde, Unicode-decimale coÃ¶rdinaten en underscores.
  Ongeldige geometrycontainers laten de hele job niet meer crashen; niet-eindige
  XY vervalt en niet-eindige optionele Z wordt onbekend. Budgetten en controles
  voor annuleren tussen object-/vertexbatches; JSON-decodering blijft Ã©Ã©n fase.
  Native Unicode-wildcard-/klassenmatcher volgt Python, inclusief ongeldige
  bereiken en eerste-matchvolgorde. Standaard ASCII-regels vermijden tijdelijke
  bron-/keywordnormalisatie. 107 oorspronkelijke parserdocumenten met 1.024
  float-eigenschappen, 149.112 regelbeslissingen en limietregressies; 168 Rust-tests
  groen. Synthetische referenties vervangen geen volledige praktijkacceptatie.
- Instelbare objectregels in GUI en CLI gekoppeld aan kaart/MarXact-kleuren,
  tellingen en standalone Excel/DXF. Bewerkbare volgorde, coderingen, lagen,
  ACI-kleuren, labels en standaardherstel; conceptwijzigingen worden pas na
  geslaagde validatie/opslag actief. Oude voorkeuren nemen regels over en oudere
  native instellingen houden defaults; een expliciet lege lijst blijft leeg.
  Begrensd instellingenlezen/-schrijven en geen vervanging bij ongeldige regels.
  Unicode-hoofdletters/casefold volgen vaste Python 3.11/Unicode 14-tabellen.
  Excel heeft de oorspronkelijke totaalregel, vaste koprij, filter, breedtes,
  rijhoogtes, fonts, kleuren en uitlijning. Gelijke disciplinenamen delen een
  kolom; samenvoegen van verschillende schrijfwijzen corrigeert verdwenen
  Python-aantallen. Geladen jobs worden geleend, kolomgroepen eenmaal voorbereid.
  Negen Python-rapportorakels met kleurconversies, lege/aangepaste regels,
  labelvarianten en formuleachtige tekst, allemaal via de echte native CLI;
  onafhankelijke ZIP/XML-vergelijking en aangepaste DXF-kleur-/laagaudit.
  Instellingenmigratie/opslagfouten, echte egui-kleuren en conceptisolatie getest.
  174 Rust-tests groen. Automatische rapportage bij template-export, overige
  instellingen en praktijkacceptatie blijven open.
- Lokale objectexport heeft bronselectie/zoeken, expliciete eindpunt-/extra-laagkeuzes,
  instelbare puntlijnlengte/labelafstand en export-als-overlay via de bestaande
  achtergrondlader. Geselecteerde gewone jobs worden geleend; virtuele jobs volgen
  de oorspronkelijke padvolgorde. Projectwissels verwerpen oude selecties en
  annulering van overlayladen laat het gepubliceerde bestand staan.
  Een primaire AppSettings-vergelijking herstelt ontbrekende warmtenet-/HS-regels,
  het monitoringlabel en 3,5 meter labelafstand (de losse exporter gebruikt 6).
  Bestaande expliciete regels blijven behouden. Tien rapport-/kleurreferenties,
  originele app-defaults en twintig afstandconversies; echte CLI-labelposities
  worden met Python vergeleken, inclusief een aangepaste nulafstand.
  De R2018-referentiegenerator sorteert CLASS-records om hash-afhankelijke volgorde
  te voorkomen; alle vijf objectassets regenereren gelijk met twee hash seeds.
  181 Rust-tests, Clippy, releasebuild en beide onafhankelijke exportaudits
  geslaagd. Schrijven is nog synchroon; online bronkeuze/downloads vanuit de exportdialog,
  volledige templates/profielen en echte GUI-/praktijkacceptatie blijven open.
- GUI-DXF/Excel schrijven op Ã©Ã©n herbruikbare exportworker. Datasets, virtuele
  proefsleuven en aanvullende kaartlagen worden zonder volledige bronkopie
  tijdelijk overgedragen en bij succes, fout of annulering teruggegeven.
  Virtuele voorbereiding, geometriestappen, rapporttellingen en schrijfbatches
  controleren annulering. Voortgangsscherm/Escape blijven beschikbaar; bewerken
  en netwerktoepassing wachten. Een atomaire eindcontrole nÃ¡ flush/fsync voorkomt
  publicatie als annuleren wint; eenmaal begonnen publicatie wordt afgerond.
  Projectopslag/back-ups worden niet op tijdelijk lege gegevens uitgevoerd;
  nieuw/sluiten wachten op teruggave vÃ³Ã³r de bestaande onopgeslagen-werkkeuze.
  Procesfouten/panics, tweede-taakweigering, gedeeltelijke uitvoer, no-clobber,
  teruggave van dezelfde allocatie en echte egui-interactie met een geblokkeerde
  worker getest. 187 Rust-tests groen. De worker-exportpaden zijn onafhankelijk
  vergeleken met alle veertien DXF- en tien rapportreferenties, inclusief lagen,
  labels/metadata, Excel-opmaak/totalen en aanvullende geometrie; CAD-audit zonder
  reparaties. CI controleert workeruitvoer apart van directe encoder en CLI.
  Annulering blijft coÃ¶peratief; Ã©Ã©n systeem-/sorteer-/geometrie-/XML-/compressiefase
  kan nog aflopen. Geen totale geheugen-/crashgarantie of volledige praktijkacceptatie.
- De onafhankelijke profielrekenkern is overgezet: stabiele begin/eindkeuze,
  weg-/terreinoriÃ«ntatie, expliciet MarXact-begin (0,5 m), zelfstandig omgekeerde
  berekening, synthetische begrenzing inclusief diameter en dubbele markers,
  hoogtemiddens van polylijnen, kettingmaat, peil en disciplinebeschrijvingen.
  Geen gekopieerde vertex-/segmentarrays; bronfeatures via index en regellagen
  geleend. Niet-eindige waarden/overflow geven een fout; scans controleren
  annulering. 224 resultaten en afzonderlijke projectie-/maaiveldwaarden zijn
  bevroren uit de oorspronkelijke V9-gepatchte Python-methoden. CI regenereert
  de orakels en verlangt een identieke diff. 190 Rust-tests groen. Dit is nog
  geen profieltekening of werkende sjabloon-/UI-export; de omgekeerde bronmetadata,
  maatbalken, leaders, clipping, rasterassets en dynamische blokken blijven open.
- De sjabloonbasis bewaart originele UTF-8-bronrecords en indexeert bereiken/handles,
  zodat onbekende CAD-records/layouts/dictionaries/proxydata niet via de verliesgevende
  kaartimport verdwijnen. Alleen bedoelde TEXT/ATTRIB/ATTDEF-/MTEXT-inhoud wordt
  gewijzigd; MTEXT krijgt begrensde Unicode-chunks. Begrensd lezen/schrijven,
  LF/CRLF/BOM, dubbele handles, structuurschade, injectie, herhaalde tekstbewerkingen
  en afbreken tussen schrijfblokken getest. Oorspronkelijke vakherkenning geeft
  dezelfde 32 vakken in het echte sjabloon en dezelfde uitkomst op vijf synthetische
  bronnen. Onafhankelijke Python-tagvergelijking en ezdxf controleren 96 echte
  tekstwijzigingen plus synthetische varianten, alle overige tags, layouts/blokken
  en geen nieuwe auditproblemen. De bekende bronproxy #147B7 met ongeldige eigenaar
  blijft behouden en vergt nog exportopschoning. 196 Rust-tests groen. Binaire/oude
  sjabloonbewerking, pagina-/handlevermenigvuldiging, volledige geometrie/assets,
  dynamische varianten en UI-export ontbreken; dit is geen volledige sjabloonexport.
- Native rasterinvoeging maakt IMAGE/IMAGEDEF/reactors, dictionary/layer/classes,
  tellers en HANDSEED; beeldverhouding/stretch/insets volgen de oorspronkelijke
  box-exporter. EÃ©n verzamelde batch verwerkt alle pagina-/kaart-/logorasterpaden
  zonder een tweede bronkopie. Inactieve papierlayouts gebruiken hun eigen BLOCK;
  de onafhankelijke eigenaarcontrole ving en corrigeerde onjuiste actieve-bladplaatsing.
  Permanente, unieke exportbundles worden na alle fileflushes en een annuleerpoort
  gepubliceerd via vervanging; oude assets voor kopieÃ«n blijven bestaan. Fouten,
  unwinding, incomplete assets, echt vergrendelde Windows-uitvoer, overlappende
  pogingen en annuleren na sync getest. 91 originele plaatsingen op drie bronnen
  zijn met ezdxf vergeleken; beide exportrondes en alle native RGBA-PNG-pixels
  zijn onafhankelijk teruggelezen, inclusief bronbehoud, koppelingen en tellers.
  Rastervoorbereiding/achtergronden/foto's, oude externe assetverplaatsing,
  volledige profiel-/pagina-/dynamische export en de UI-workflow blijven open.
  206 Rust-tests, Clippy, releasebuild en onafhankelijke release-audits geslaagd.
- Projectnaam is op uitdrukkelijk verzoek toegevoegd aan het bestaande desktop-
  sjabloonexportvenster; bevestigen geeft de waarde door aan de exporter en
  annuleren behoudt de vorige waarde. Datum wordt Ã©Ã©n keer per document lokaal
  bepaald en vult DATUM1 of de eerste ongenummerde DATUM-revisieregel. Projectnaam
  vult GEM-PROJECT/PROJECTOMSCHRIJVING/PROJECTNAAM; projectnummer, ongebruikte bladen,
  blokdefinities en latere revisies blijven intact. De native Rust-kern voert
  dezelfde bewerkingen atomair en begrensd uit. Vier onafhankelijke DXF-cases
  controleren 78 velden en alle overige tags; Windows-lokale datum vergeleken met
  Python. 210 Rust-tests en vijf Python-regressies inclusief een echt Tk-venster
  geslaagd. Ook de volledige Python-suite (383 tests), Clippy, releasebuild en
  onafhankelijke native release-audit slagen. Een bestaande subprocestest laadt
  nu expliciet deze checkout, zodat een andere checkout met dezelfde pakketnaam
  de test niet overneemt. De volledige native sjabloonexport en het bijbehorende
  venster blijven open.
- De native sjabloontekenlaag voegt polylijnen, cirkels, uitgelijnde tekst en
  oorspronkelijke profielblokken met automatische ATTRIB-ketens toe. Blokbasis,
  schaal, kleur/stijl, standaardwaarden, rotatie, oblique/uitlijning en flags
  worden met de oorspronkelijke ezdxf-helper vergeleken: 89 object-/blokplaatsingen
  op het echte sjabloon en een synthetische actieve/inactieve papierbron. Iedere
  brontag blijft behouden buiten bedoelde toevoegingen/tellers. Bronblokken en
  eenmaal ingelezen attribuutmetadata worden gedeeld; eerder bewerkte ATTDEF-
  standaardtekst wordt ook bij nieuwe plaatsingen gebruikt. Gedeelde nieuwe
  laagnamen/tellers corrigeren compositie van meerdere tekenbatches en rasters,
  in beide volgordes, zonder bronkopie of herlezen. Atomaire fouten, afbreken,
  budgetten en behouden eerdere tekstwijzigingen getest. Multiline/gekantelde
  attributen, algemene blokrotaties, volledige profielindeling/bandverdeling en
  dynamische leaders blijven open.
  215 Rust-tests, Clippy en releasebuild geslaagd; de onafhankelijke release-
  controles voor alle 89 tekenobjecten, 91 rasterplaatsingen en 78 titelvelden
  slagen ook na de gedeelde document-/laagwijzigingen. De vijf GitHub-controles
  van de voorafgaande projectnaam-/datumstap (`e1bd5c7`) zijn groen.
- Native profielkaders en gekoppelde afstands-/hoogtebanden schrijven nu echte
  tekeninggeometrie: automatische schaal uit asbreedte en maximale polylinehoogte,
  handmatige schaalbegrenzing, profiel-/referentietitels, profielverticalen per
  markerinstantie en stabiel verdeelde bandwaarden met verbindingslijnen.
  Positie/hoogte/rotatie van bandattributen kan na brontransformatie expliciet
  worden gezet; genormaliseerde attribuuttags volgen de oorspronkelijke getter.
  Templates worden eenmaal voorbereid, meetvertices worden geleend en dezelfde
  sorteervolgorde dient voor plaatsing en connectoren. 24 aanroepen van de volledige
  oorspronkelijke V9-tekenmethode leveren 928 vergeleken kader-/bandobjecten,
  inclusief alle attributen, normale/omgekeerde profielen, referentieafstanden en
  RD-omschrijvingen. Nog 100 oorspronkelijke bandverdelingen dekken gelijke/
  ongesorteerde ankers, vastgezette eindpunten en krappe/overvolle banden; nummers
  met Unicode-cijfers/suffixen en afronding/negatieve nul zijn apart gecontroleerd.
  De generator controleert de herbouwde invoerprofielen ook tegen het bestaande
  profielorakel. 220 Rust-tests en Clippy geslaagd. Maaiveld/oppervlakteksten,
  profielmarkeringen, referentie-indicatoren, dynamische leaders en volledige
  sjabloon-/pagina-/UI-koppeling blijven open.
  Releasebuild en alle vier onafhankelijke release-audits slagen: 928 kader-/
  bandobjecten, 89 generieke tekenobjecten, 91 rasterplaatsingen en 78 titelvelden.
  De vijf GitHub-controles van de voorafgaande tekenlaagstap (`3a8d6ea`) zijn groen.
- Maaiveld-/oppervlakteweergave, grondvak, gemeten profielpolylijnen en losse
  profielmarkeringen toegevoegd. Kleuren en tekstprioriteit volgen de oorspronkelijke
  helpers; automatische BGT-teksten worden als vooraf opgeloste invoer doorgegeven.
  Cirkelblokken worden native/atomair gedeclareerd en direct door latere batches
  gebruikt, zonder het document opnieuw te lezen. Geclipte cirkels volgen de 64
  oorspronkelijke segmenten; de onafhankelijke controle ving een fout waarbij
  het tweede clipeindpunt vanaf het gewijzigde beginpunt werd berekend. Dat is
  hersteld. Metadata met ongeldige hex-underscores krijgt een veilige fallback.
  De volledige oorspronkelijke tekenmethode levert 24 samengestelde referenties
  met 2.209 objecten; alle harde/zachte segmenten en tekstobstakels zijn vergeleken.
  Nog 150 clips en twaalf tekst-/kleurgevallen zijn onafhankelijk gecontroleerd.
  Directe profielpuntindices voorkomen herhaald zoeken per band; namen van
  gelijkwaardige lagen worden geleend en eenmaal gedeclareerd met eerste
  schrijfwijze/laatste kleur. Samenstellen verplaatst geometrie zonder kopieÃ«n
  van bronvertices. BGT-oppervlaktegeometrie/-fetch, referentie-indicatoren, donor-
  dynamische leaders, wijzigingen van bestaande aangepaste laagstijlen en de
  volledige sjabloon-/pagina-/UI-workflow blijven open.
  225 Rust-tests, Clippy en releasebuild geslaagd. Alle vijf onafhankelijke
  release-audits slagen: 2.209 samengestelde profielobjecten, 928 kader-/bandobjecten,
  89 generieke tekenobjecten, 91 rasterplaatsingen en 78 titelvelden. De vijf
  GitHub-controles van de voorafgaande kader-/bandstap (`6469ff1`) zijn groen.
- Native referentie-indicatoren toegevoegd: metadata-/standaardpunt, projectie op
  de profielas, RD-afronding op gehele meters, kaartrotatie, cirkel/kruis en
  labelverwijzing binnen beeld, afstandspijl buiten beeld en de oorspronkelijke
  zachte segmenten/tekstkaders voor leaders. De native referentie wordt vÃ³Ã³r de
  afstandsbanden opgelost; de samengestelde vergelijking neemt geen Python-
  kettingmaat over. 216 directe oorspronkelijke helperaanroepen dekken normale/
  omgekeerde profielen, drie schalen, exacte grenzen, zijruimte en buitenliggende
  punten. 24 samengestelde tekeningen vergelijken 2.249 objecten en alle attributen/
  obstakels. Ongeldige metadata valt terug zoals oorspronkelijk; niet-eindige
  waarden, onnodige rotatie-invoer en annulering zijn apart gecontroleerd.
  Dynamische donor-leaders, BGT-oppervlaktegeometrie/-fetch, bestaande aangepaste
  laagstijlupdates en volledige sjabloon-/pagina-/UI-koppeling blijven open.
  228 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  Alle zeven onafhankelijke releasecontroles slagen: 216 referentiegevallen,
  24 samengestelde profielen met 2.249 objecten, de eerdere body-/kadergevallen,
  generieke tekenobjecten, rasterbundels en titelvelden. De vijf GitHub-controles
  van de voorafgaande maaiveld-/markerstap (`97a3ae2`) zijn groen.
- Native donor-metadatafinalisatie toegevoegd: de 15 goedgekeurde AutoCAD-
  records/523 tags blijven op parameterniveau behouden; alleen handles worden
  aangepast. De complete batch gebruikt de gedeelde vrije handleteller en wordt
  pas na alle controles/annulering toegepast. Nieuwe GUIDs komen uit de OS-
  toevalsbron. Bestaande volledige dynamische grafen worden ongewijzigd behouden;
  onvolledige of andere extensiedictionaries geven een fout zonder gegevensverlies.
  De controle vereist ook de schaalactie, die de oorspronkelijke idempotentie-
  controle vergat. De eigenaargraaf wordt eenmaal geÃ¯ndexeerd in plaats van
  herhaald volledig gescand. Vier onafhankelijke vergelijkingen controleren alle
  bron-/donortags, Polar-punten, stretch/move/scale-doelen, representatietags en
  CAD-readback, inclusief het echte voorbereide sjabloon, Unicode, BOM/CRLF,
  oude XDATA, hoge HANDSEED en herhaald uitvoeren. Geometrie-installatie,
  prototype-INSERT-kopieÃ«n, plaatsing en interactieve AutoCAD-acceptatie blijven
  open; dit is nog geen volledige dynamische leaderworkflow.
  233 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  Alle vier donorvergelijkingen slagen ook met de release-executable; de fixtures
  regenereren byte-identiek met een andere Python-hashseed. De vijf GitHub-
  controles van de voorafgaande referentiestap (`681868f`) zijn groen.
- Native donor-installatie/behoud en authored-block-verwijzingen toegevoegd.
  Ontbrekende geometrie, blokrecord, volledige dynamische graaf en vereiste APPIDs
  worden samen atomair toegevoegd. Bestaande volledige blokgrafen worden direct
  behouden; het actuele meegeleverde sjabloon bevat zo'n compleet blok, maar geen
  layoutvoorbeeld dat de oorspronkelijke prototypezoeker vindt. De native route
  verwijdert/herbouwt die bestaande graaf niet. Nieuwe ATTDEFs worden eenmaal
  gecachet en in volgende tekenbatches geleend; blokaantallen en handles worden
  gedeeld met andere blokdeclaraties. Oorspronkelijke minimumschaal, basiscorrectie,
  attribuuttransformaties en optionele/expliciete kleuren zijn geport. Drie
  onafhankelijke audits vergelijken 120 INSERTs/240 attributen, volledige nieuwe
  of behouden blokgrafen, ontbrekende APPID-registratie, bronbehoud en CAD-readback.
  De eerdere vier metadatavergelijkingen blijven gelden. Prototype-INSERT-kopieÃ«n,
  volledige leaderentrygeneratie/-verdeling en profiel-/UI-/AutoCAD-acceptatie
  blijven open; de referentiehelper is nog geen volledige profielworkflow.
  237 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  Alle zeven donorcontroles, 89 generieke tekenobjecten en 24 samengestelde
  profielen/2.249 objecten slagen met de release-uitvoer. Nieuwe en eerdere
  fixtures regenereren byte-identiek met een andere hashseed. De vijf GitHub-
  controles van de metadatastap (`18da34c`) zijn groen.
- Actieve v0.3.10-profielverwijzingen geport: oorspronkelijke kandidaatvolgorde,
  grens-/label-/tekst-/hard-/softlijnscores, tweede pass zonder zachte obstakels,
  laagste-penaltyfallback en stabiele ankersortering. Een vaste scorematrix
  vervangt hashes van floatparen. Als botsingsvermijding uitstaat of de bestaande
  grens van 80 entries wordt overschreden, wordt de onvoorwaardelijk gekozen
  eerste kandidaat direct berekend, zonder nutteloze obstakelscores. Het actieve
  attribuutblok wordt atomair gedeclareerd/gecachet; echte LEADER-records verwijzen
  naar het eerdere INSERT-handle in dezelfde batch/layout, gevolgd door de bandlijn.
  65 oorspronkelijke scenario's bevatten 382 plaatsingen/1.146 DXF-objecten;
  attributen, vertices/flags/kleur/stijl, annotatiekoppeling, bronbehoud en CAD-audit
  worden onafhankelijk gecontroleerd. Ongeldige/verkeerde annotatiereferenties,
  annulering, niet-eindige waarden en invoer-/tekstbudgetten publiceren geen deelbatch.
  De runtime-/regressiecontrole corrigeert de eerdere prioriteit: de dynamische
  prototypepatch staat uit. Volledige native entrygeneratie/compositie en de
  sjabloon-/UI-workflow blijven open.
  240 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  De 65 actieve DXF-audits slagen met release-uitvoer, evenals de zeven eerdere
  donorcontroles, 89 generieke tekenobjecten en 24 samengestelde profielen/2.249
  objecten. Fixtures regenereren byte-identiek met een andere hashseed. De vijf
  GitHub-controles van `8541956` zijn groen.
- Native profielinvoer en complete tekencompositie gekoppeld: meetobjecten en
  eigen profiel-/bandposities bepalen ankers, verschoven band-X, globale
  leaderschaal, startdiepte en volledige beschrijvingen. Een gedeelde Unicode-
  getalmatcher bedient diameter Ã©n bundelvelden; mantelbuizen, materiaal,
  afronding en aangepaste diameterprefixes volgen de oorspronkelijke helpers.
  Dubbele markerinstanties hergebruiken hun beschrijving/diepte. De publieke
  compositiestap valideert en lost alle labels op voordat zij het bestaande
  kader/body/referentie-resultaat aanvult. Verouderde bandindices/hoogtes,
  numerieke overloop en annulering laten eerdere geometrie intact.
  48 complete oorspronkelijke tekeningen met botsingsvermijding uit/aan
  vergelijken 5.398 objecten, 150 native afgeleide entries per stand, alle
  attributen en annotatiehandles. 336 extra beschrijvingsgevallen worden direct
  met Python vergeleken. Bestaande oraclevelden zijn inhoudelijk ongewijzigd;
  alleen nieuwe entry-/leader-/beschrijvingsreferenties zijn toegevoegd.
  BGT-oppervlaktegeometrie/-fetch, bestaande aangepaste laagstijlen, pagina-/
  rastervoorbereiding en de volledige native sjabloon-/UI-workflow blijven open.
  243 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  Alle zes profielmodi slagen met release-uitvoer: de twee complete standen,
  kader/banden, body/markers, body/referenties en 216 directe referentiegevallen.
  De 65 losse actieve labeltekeningen blijven eveneens groen. Fixtures regenereren
  byte-identiek met een andere hashseed. De vijf GitHub-controles van `60453a7`
  zijn groen.
- Native tekststijl-/laagvoorbereiding toegevoegd aan de samengestelde profielroute.
  Ontbrekende fonts/styles worden aangemaakt, bestaande eigen fontinstellingen
  behouden; alleen de bedoelde ACI-/lijngewichtvelden worden bijgewerkt. Ook
  lagen uit eerdere geometrie-/rasterbatches kunnen zonder documentherlezing
  worden gewijzigd. Gerichte vervanging van hun invoegpayloads is atomair;
  ontbrekende tags blijven vÃ³Ã³r XDATA en nieuw toegevoegde records staan.
  Herhaalde kleurenwijzigingen, terugkeer naar de bronkleur en annulering
  behouden het juiste resultaat en consistente gedeelde handles/tabeltellingen.
  DXF-symboolnamen gebruiken nu ezdxf lower() in plaats van casefold(), zodat
  StraÃŸe/STRASSE en Griekse sigmavarianten niet samenvallen. Unicode-14-tabellen
  en 128.358 onafhankelijke contextprobes leggen dit contract vast. De originele
  Python-aanroep voor nieuwe puntlagen gaf kleur/lijngewicht mee via dxfattribs;
  ezdxf overschreef die met zijn standaardargumenten. Expliciete argumenten en
  twee Python-regressies herstellen dezelfde fout in de bestaande app.
  Vier oorspronkelijke reeksen/15 toestanden vergelijken alle laag-/stijlvelden,
  Unicode-blokken/attributen en oorspronkelijke tags, inclusief het echte
  sjabloon. CAD-readback veroorzaakt geen nieuwe auditfouten/reparaties.
  247 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  Release-audits: 15 voorbereidingstoestanden, 48 complete profielen/5.398
  objecten, 89 generieke tekenobjecten en 91 rasterplaatsingen/alle PNG-pixels.
  Fixtures regenereren byte-identiek; ook de eerdere profieloracle blijft
  ongewijzigd. De vijf GitHub-controles van `349be99` zijn groen. BGT-oppervlakte-
  resolutie, pagina-/rastervoorbereiding en de native sjabloon-/UI-workflow
  blijven open.
- Native bladnummering en overzichtsviewports overgezet. Acht vakken vormen een
  blad, met behoud van lege posities. Het eerste INSERT krijgt BLAD/BLADEN;
  de laagste KADER-viewport wordt op de gevulde bladvakken gepast met de zes
  oorspronkelijke schalen, kaartmidden, richting/target en SCHAAL-attribuut.
  Ontbrekende bladen, titelblokken of viewports volgen de oorspronkelijke
  helpersemantiek. Layoutruimtes worden eenmaal geÃ¯ndexeerd; geen volledige
  documentkopieÃ«n of herhaalde scans per blad. Viewportbewerkingen behouden alle
  overige recordbytes en starten bij herhaling vanuit de vorige staged versie.
  Validatiefouten, annulering en overloop op een later blad publiceren geen
  gedeeltelijke eerdere wijzigingen. Twintig oorspronkelijke toestanden,
  inclusief het echte sjabloon, vergelijken alle viewport-/ATTRIB-velden,
  bronbehoud en CAD-readback. Vijf Unicode-layouts vergelijken titelvelden,
  cirkels/rasters en hun ruimte-eigenaren. Layoutnamen volgen ezdxf upper();
  lagen/blokken blijven afzonderlijk lower(). Dit herstelt onder meer Ä±/I en
  voorkomt samenvoeging van K met het Kelvin-teken bij papierplaatsing.
  De productie-Python-patch bleek dezelfde laagaanroepfout te overschrijven.
  Ook die actieve cache gebruikt nu expliciete kleur/lijngewichtargumenten en
  lower-sleutels, zodat een eerdere hoofdlettervariant geen verouderde kleur
  teruggeeft. De bestaande performancetest-double volgt nu de echte ezdxf-API.
  Beide volledige Python-suites slagen lokaal: 385 tests in productiestand en
  385 bij volledige legacy-terugval. Op Windows is TEMP in de testharness
  gecanonicaliseerd om 8.3-padaliassen in bestaande padvergelijkingen te vermijden.
  249 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild geslaagd.
  Release-audits slagen voor 20 bladtoestanden/vijf Unicode-layouts, de eerdere
  15 voorbereidingstoestanden, 78 titelvelden, 89 generieke tekenobjecten,
  91 rasterplaatsingen/alle PNG-pixels en 48 complete profielen/5.398 objecten.
  Nieuwe fixtures regenereren byte-identiek met een andere hashseed; de eerdere
  voorbereidingsoracle blijft inhoudelijk gelijk en controleert ook de actieve
  helper op dezelfde eindtoestand. Bladuitbreiding, legenda's, rastervoorbereiding,
  BGT-oppervlakteresolutie en de complete native sjabloon-/UI-workflow blijven open.
- Native BGT-oppervlakteksten uit voorbereide geldige geometrie overgezet.
  Een geleende index bereidt naamgroepen/begrenzingen eenmaal voor. Drie
  profielproeflijnen berekenen overlap met polygonen/gaten, multipolygonen,
  lijnen en collecties. Een sweep over gesorteerde kruisingen vermijdt volledige
  punt-in-polygoonscans per interval; robuuste oriëntatietekens komen uit GeoRust
  robust 1.2. Intervallen worden binnen één feature verenigd, daarna per
  casefold-naam opgeteld met de oorspronkelijke naam-/schrijfwijzeprioriteit.
  Lengtes worden uit de wereldpunten berekend zodat onnodige parameterronding
  geen gelijke horizontale overlappen tot verschillende scores maakt.
  541 oorspronkelijke Shapely/GEOS-gevallen, waaronder 520 reproduceerbare
  polygonen op lokale/RD-coördinaten, controleren de directe winnaar en alle
  drie profielteksten. Een regressie met 12.000 inkepingen/ruim 60.000 vertices
  bewaakt veel kruisingen zonder kwadratische classificatie. Grenzen voor
  features/vertices/diepte/tekst en numerieke waarden blijven annuleerbaar.
  De complete profielgenerator gebruikt echte geometrie in plaats van een
  voorgedefinieerde tekstfunctie. Native export leidt dezelfde BGT-waarden af;
  alle oudere profieloraclevelden en het blok-DXF blijven inhoudelijk gelijk.
  Handmatige tekst-/kleurprioriteit blijft via resolve_bgt beschikbaar.
  252 Rust-tests, Clippy zonder nieuwe waarschuwingen en de definitieve
  releasebuild slagen. Alle zes
  release-profielmodi slagen, waaronder 48 complete tekeningen/5.398 objecten;
  nieuwe en geïntegreerde fixtures regenereren byte-identiek met een andere
  hashseed. Release-integriteit en legacy-contract zijn gecontroleerd. De drie
  PR-controles van `272e142` zijn groen, inclusief de volledige Python-suite.
  De gerichte native/originele benchmark bewaart alle drie procesrondes en een
  gelijke tekstchecksum in rust/benchmarks/bgt-surface.json. Voorbereiding wordt
  afzonderlijk gemeten; netwerk, geometrie-/JSON-decodering, opstart, GUI/DXF en
  procespiekgeheugen zijn uitgesloten. Dit bewijst geen volledige app-prestatie.
  BGT-tegelinvoer/reprojectie/herstel/union/clipping/cache, bladuitbreiding,
  rastervoorbereiding en de complete native sjabloon-/UI-workflow blijven open.
- Native BGT-tegelinvoer en gedeelde client toegevoegd. De begrensde MVT 1/2-
  protobufdecoder leest waardetypen, packed/unpacked tags/commando's, polygonen
  met gaten, meerdere vlakken/lijnen en oorspronkelijke ring-/Y-semantiek.
  Normalisatie, identifierprioriteit/fallback en RD-omzetting volgen de oude
  helper. Dictionarywaarden en gecachte wereldgeometrie worden gedeeld.
  De matrix bewaart dezelfde epsilon/randklemmen en rij-/kolomvolgorde.
  Een vaste openbare PDOK-tegel en 25 synthetische gevallen vergelijken 819
  oorspronkelijke geometrieën/505 ondergronddelen; 360 matrixvragen controleren
  de grenzen. Raw/decompressed bytes, woorden, features, vertices, metadata,
  groepsdiepte, cache en totaalresultaat zijn begrensd en annuleerbaar.
  De native HTTP-client gebruikt één verbindingenpool, een begrensde werkbatch,
  maximaal vier pogingen met annuleerbare backoff en een LRU met aantallen én
  bytes. Beschadigde antwoorden komen niet in de cache. Volgorde blijft stabiel
  bij parallel ophalen; batchfouten, workerpanic, annuleren of een budgetfout
  geven geen gedeeltelijk resultaat. Tests controleren gzip-leden/beschadiging/
  decompressielimiet, LRU, gedeelde Arc-geometrie, hergebruik en clientherstel.
  Live HTTPS is met native opname van de antwoordbytes gecontroleerd: dezelfde
  518 geometrieën/330 ondergronddelen als de oorspronkelijke code; een tweede
  opvraag deelt dezelfde geometrie en veroorzaakt geen nieuw HTTP-verzoek.
  CI gebruikt alleen de vaste offline gegevens. Alle binaire/JSON-oracles
  regenereren byte-identiek met verschillende hashseeds; PBF krijgt geen
  regeleindeconversie bij checkout.
  De eerdere layoutfixturegenerator bleek vereiste CLASS-records in een door
  Python-hashes beïnvloede volgorde te schrijven. Expliciete registratie en
  sortering in b3d8f31 herstellen die CI-flake. Vier onafhankelijke hashseeds
  leveren nu dezelfde DXF en referenties; 20 bladtoestanden/vijf Unicode-layouts
  slagen opnieuw met onafhankelijke readback.
  259 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuild slagen.
  De 26 tegelgevallen en live-client slagen ook met de release-uitvoer, naast
  48 complete profielen/5.398 objecten en de bladaudit. De koude-/cachebenchmark
  bewaart drie procesrondes en dezelfde aantallen in rust/benchmarks/bgt-tiles.json.
  Dit zijn per-tegelgegevens: bronfeature-union, topologieherstel, clipping en
  aansluiting op de volledige sjabloon-/UI-workflow blijven open. De voorbereide
  profielgeometriekern mag de ruwe delen pas na die stappen gebruiken.
- Native bronfeature-herstel, samenvoegen en knippen aan de tegelclient
  gekoppeld. Groepering volgt laag/id voor paden en laag/id/ondergrond voor
  oppervlakken, met oorspronkelijke drie-decimaal-/omkeer-deduplicatie.
  De geleende ondergrondindex ontvangt volledige voorbereide geometrie.
  Het pure Rust i64-overlay reconstrueert vlakken uit genodeerd lijnwerk,
  behandelt gaten/zelfdoorsnijdingen en behoudt losse lijnen en raakpunten.
  Ongewijzigde vertices krijgen exact hun invoercoördinaten terug via
  roundtrip-sleutels; nieuwe snijpunten gebruiken gecompenseerde rekenkunde.
  De index voor geconstrueerde oppervlakken heeft uitsluitend bij randintervallen
  een expliciet acht-ULP-afrondingsbudget. De bestaande bronindex blijft exact;
  haar 541 oorspronkelijke gevallen blijven slagen. Annuleren en invoer-/
  resultaat-/kruisingsbudgetten begrenzen de afzonderlijke verwerkingsfasen.
  Bovenliggende vlakken krijgen gememoiseerde dieptes, zodat diepe nesting
  geen herhaalde kwadratische ketenwandeling vraagt; 64 geneste ringen hebben
  een afzonderlijk oorspronkelijk GEOS-geval.
  Een werkelijk Python-defect is gereproduceerd: unary_union op één beschadigd
  en één geldig deel kon het hele bronobject weggooien. Beide clients herstellen
  nu ieder deel vóór union; de gepinde generator bewijst de oude uitzondering
  en de Python-regressie bewijst behoud van beide delen voor paden en ondergronden.
  844 onafhankelijke GEOS-geometrieën, inclusief 300 reproduceerbare combinaties,
  487 groepen uit de vaste PDOK-tegel en rand-/hoek-/nulbreedtegevallen worden
  vergeleken op geldigheid, Hausdorff-afstand, symmetrisch vlakverschil en lengte.
  De RD-coördinaattolerantie is 2e-8 meter; ringstart/-richting, componentvolgorde
  en collineaire onderverdeling mogen verschillen. Drie volledige batches
  vergelijken 336 voorbereide oppervlakken, het padnet en alle 120 oorspronkelijke
  ondergrondbeslissingen. Een complete native-clienttest toont cachehergebruik
  met één opgenomen tegel en annuleerbare voorbereide uitvoer.
  Alle referenties zijn byte-reproduceerbaar met verschillende hashseeds;
  onafhankelijke geometrie-/batchreadback is toegevoegd aan Windows CI.
  264 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuilds van app
  en auditdrivers slagen. De onafhankelijke release-audit slaagt voor alle 844
  geometrieën/drie batches, 48 complete profielen/5.398 objecten en 216 losse
  referentiegevallen. Beide volledige
  Python-opstartmodi slagen met 386 tests; release-integriteit en het legacy-
  contract blijven goed. Dit bewijst geen prestatieverbetering voor deze gehele
  geometriefase en geen volledige app-pariteit; eerdere benchmarks behouden
  hun eigen meetgrenzen. Bladuitbreiding/legenda's, raster-/fotovoorbereiding en
  de complete native exportdialoog met projectnaam/datum blijven open.
- Native modelblad-/papierlayoutuitbreiding toegevoegd als één transactie.
  De oorspronkelijke selectie op TEXT/MTEXT/LINE/LWPOLYLINE-ankers en blad-
  begrenzingen bepaalt modelkopieën; ontbrekende layouts krijgen nieuwe
  blokrecords, BLOCK/ENDBLK, dictionaryregistratie en alle ondersteunde
  papierobjecten. Gekoppelde ATTRIB/VERTEX/SEQEND krijgen eigen eigendom;
  viewport-clips worden naar de gekopieerde grenzen omgelegd, alleen het
  hoogste KADER-venster krijgt de oorspronkelijke modelbladverschuiving.
  Bestaande fonts/blokken en IMAGEDEF blijven gedeeld; iedere IMAGE krijgt
  een nieuwe reactor. Geneste dictionarywaarden worden zelfstandig gekopieerd.
  Een brondefect bij soft-owned dictionaries is aangetoond: de oorspronkelijke
  kopie verwijst naar waarden met het oude eigendom, waarna CAD-audit sleutels
  verwijdert. Native gebruikt het daadwerkelijke eigendom en behoudt alle data
  zonder aanvullende auditreparaties. Nieuwe papierruimten gebruiken een gedeelde
  bloknaamindex om herhaald zoeken door het hele document te vermijden.
  OCS, niet-eenheidsextrusie, MTEXT-rotatie en embedded R2018-kolommen zijn
  gecontroleerd. Embedded richtingsvectoren blijven staan terwijl hun invoegpunt
  verhuist. Appdata blijft gelijk; XDATA-posities volgen de wereldverschuiving,
  verplaatsingsvectoren blijven ongewijzigd. Een geïndexeerde documentkopie
  maakt eerder voorbereide tekst/symbolen/geometrie zichtbaar voor het vervolg.
  De oorspronkelijke tekening wordt pas na volledige validatie vervangen;
  annuleren, fout en workerpanic na een gekopieerd blad behouden alle oude bytes.
  Voortgang, limieten en een niet-voortgangcontrole zijn beschikbaar.
  Vijf oorspronkelijke reeksen bevatten twaalf toestanden (elf geslaagde
  bestanden en een atomaire fout). Het echte sjabloon groeit van 32 naar 64
  vakken. Onafhankelijke release-readback vergelijkt geometrie, bladinstellingen,
  alle niet-default-attributen, kolommen, objectgrafen en opvolgende datum-/
  project-/viewportbewerking; overige oorspronkelijke tags en blokken blijven
  behouden. Alle bron-DXF's en JSON regenereren byte-identiek met twee hashseeds.
  267 Rust-tests, Clippy zonder nieuwe waarschuwingen en releasebuilds van app,
  auditdriver en benchmark slagen. Windows CI krijgt dezelfde readback en meting.
  Drie verse processen per route meten laden/uitbreiden naar 64 vakken op de
  echte 18.855.304-byte tekening: mediaan native 152,538 ms tegenover Python
  575,234 ms; proces-piekwerkset 71.966.720 tegenover 145.993.728 bytes. Beide
  routes leveren 64 vakken, vier nieuwe bladen, 18 layouts en 603 modelobjecten.
  Zie rust/benchmarks/template-pages.json voor alle rondes en meetgrenzen.
  Timing sluit opstart/import, DXF-uitvoer, titel-/viewportbewerking, rasters,
  netwerk en GUI uit; procespiek bevat wel de opstart-/importbasis. Bestandscache
  is niet gecontroleerd. Dit is geen volledige export-/app-benchmark.
  Legenda-/titelherstel en de complete native exportbediening blijven open,
  evenals raster-/fotovoorbereiding en zelfstandig kopiëren van DIMENSION,
  proxies/tabellen/underlays en andere uitbreidingsobjecttypen. Niet ondersteunde
  kopieën geven vóór wijziging van de oorspronkelijke tekening een fout.
- Legenda-/titelherstel en samengestelde sjabloonvoorbereiding toegevoegd.
  De oorspronkelijke Blad1-suffixselectie en bestaande-INSERT-controle bepalen
  welke papierlayouts een legenda krijgen. ATTDEFs worden opnieuw omgezet naar
  ATTRIBs met volledige 3D-affine tekstmaten, signed/nonuniform XYZ-schaal,
  spiegeling, OCS, rotatie, schuinte, uitlijning en oorspronkelijke tagwaarden.
  Voorbereide attributen worden per bronblok hergebruikt voor volgende bladen.
  APPDATA met subclass-/geometrieachtige tags beïnvloedt de transformatie niet.
  Negen oorspronkelijke referenties controleren 433 nieuwe attributen, echte
  bladen, actieve papierruimte, dubbele namen en afbeeldingsgrafen (R2007/R2018).
  De atomaire voorbereiding koppelt bladen, fonts/lagen, legenda's, tekening- en
  rastereenheden, schaallijst, projectnaam/tekenaar/datum en overzichtsvensters.
  Alias-schaalobjecten krijgen één vervanging; verwijderde dictionaryentries
  vernietigen hun bronobjecten niet. Elf oorspronkelijke toestanden vergelijken
  het echte sjabloon en synthetische varianten, inclusief ontbrekende INSUNITS,
  lege vakken, herhaalde voorbereiding en aanwezige/ontbrekende rasterinstellingen.
  Ongeldige invoer, late annulering en panic laten het origineel byte-identiek.
  Nieuw tekenwerk en PNG kunnen daarna dezelfde stijlen, lagen en eenheden op
  nieuw aangemaakte bladen gebruiken. Lokale datum wordt één keer vastgelegd;
  een Windows-test controleert de automatische datumoptie op alle gebruikte bladen.
  Alle 272 Rust-tests en releasebuilds slagen; Clippy geeft geen nieuwe
  waarschuwingen. Beide referentiefamilies regenereren met twee hashseeds
  byte-identiek. Onafhankelijke readback controleert velden, geometrie,
  eigendomsgrafen, behouden brontags en auditbaseline; nieuwe SEQEND-eigenaars
  worden ook vóór ezdxf-normalisatie in de ruwe DXF gecontroleerd.
  Drie verse processen per route meten laden plus de gezamenlijke voorbereiding
  naar 64 vakken op het echte sjabloon: mediaan native 242,587 ms tegenover
  oorspronkelijk 589,256 ms; procespiekwerkset 92.401.664 tegenover 146.497.536
  bytes. Beide routes leveren vier nieuwe bladen, zeven herstelde legenda's,
  18 layouts en 723 ATTRIBs. Zie rust/benchmarks/template-setup.json voor rondes
  en meetgrenzen. Dit sluit opstart/import, definitief schrijven, raster/foto,
  BGT/netwerk, profieltekenwerk en GUI uit; procespiek bevat runtime/importbasis.
  Multiline ATTDEF-regeneratie, andere CAD-kopieertypen, raster-/fotovoorbereiding
  en de volledige native exportbediening blijven open.
- Voorbereide sjabloonexport tot één native samenstelling/publicatie gekoppeld.
  De API en --template-dxf accepteren voorbereide PNGs, slotteksten, gemeten
  datasets/richting/regels, BGT-oppervlakken, profielopties en overzichtsgeometrie.
  Blad-/titelvoorbereiding wordt gevolgd door volledige profielopbouw,
  maaiveld-/referentie-/markercompositie en actieve annotatie-INSERTs met LEADER.
  TIFF-/kaartbeelden krijgen indexgebonden eigen bestandsnamen, zodat dubbele
  zichtbare PS-labels nooit dezelfde uitvoer-PNG krijgen. Elke PNG wordt na een
  begrensde, annuleerbare kopie volledig gedecodeerd; alleen afmetingen lezen
  kan een kapotte pixelstroom ten onrechte accepteren. Eén afbeeldingengrafbatch
  voorkomt herhaalde documentherlezing per beeld. Eén eindindexering herstelt
  daarna de oorspronkelijke volgorde beelden/profiel per vak; bestaande objecten
  blijven staan. Profielranges gebruiken binair zoeken; de sorteertabel bewaart alleen
  nieuwe records. Zonder profielen vervalt deze eindindexering.
  Vier oorspronkelijke samenstellingen vergelijken zes complete profielen,
  twintig PNGs, lege vakken, dubbele namen, extra blad, BGT-/handmatige waarden,
  beide botsingsmodi, omkering, schaalbegrenzing en voorbereide overzichtsgeometrie.
  Readback toetst alle velden/geometrie, attribuut-/annotatieverwijzingen, ruwe
  eigendomsgrafen, volgorde, brontags/header, PNG-bytes/pixels en auditbaseline.
  277 Rust-tests slagen, inclusief annulering/panic na het laatste schrijven,
  ontbrekende/corrupte beelden, late profiel-/tekstfouten, herexport en blijvende
  oude bundles nadat invoer-PNGs verwijderd zijn. Een uitvoerpad dat naar een
  invoersjabloon/-PNG wijst wordt vóór wijziging geweigerd; die gevonden
  overschrijffout heeft een regressietest. De echte CLI controleert
  manifest-relatieve invoer en weigert bestaand resultaat te overschrijven.
  Referenties regenereren byte-identiek met twee hashseeds.
  Clippy geeft geen nieuwe waarschuwingen; definitieve release-app en auditdriver
  slagen op alle vier referenties plus de echte CLI. De gedeelde verifier houdt
  ook de elf eerdere voorbereidingstoestanden groen. Drie verse native processen
  meten de gemengde uitvoer met twee profielen en zes kleine voorbereide PNGs:
  mediaan 148,304 ms, procespiekwerkset 75.116.544 bytes, DXF 18.919.176 bytes.
  Laden, voorbereiding, tekenen, PNG-validatie/kopieën, volgordeherstel en
  schrijven/fsync/publicatie zijn inbegrepen. Manifestuitlezing, rasterrendering,
  ophalen, logo-/contactherstel en GUI blijven buiten deze native referentiemeting;
  er is geen Python-vergelijking. Zie rust/benchmarks/template-export.json.
  Dit is expliciet de stap na raster-/serverdatavoorbereiding. Die voorbereiding,
  foto's, automatisch overzichtsgeometrie afleiden, logo-/contactherstel en het
  complete native exportvenster blijven open.
- Volledige TIFF-vakbeeldvoorbereiding aan de native export gekoppeld.
  De volledige decoder deelt de bestaande kleur-/strook-/tegelkern, zonder
  previewverkleining of tijdelijke bronkolom-/rijvectoren. 45 ongeschaalde
  kleurexactreferenties en iedere pixel van een 4097×4097-strokenbestand worden
  getoetst; alle 257 stroken worden gelezen met maximaal 196.656 chunkbytes.
  Oriëntatie gebruikt dezelfde eenmaal gebouwde gemeten profielas als de
  tekening, of de oorspronkelijke gewogen weg-/terreinprojectie. Een reeds
  omgekeerde as krijgt geen tweede omkering. Native bicubische RGBA-rotatie
  volgt 15-decimale affine afronding, premultiplicatie en oorspronkelijke
  rand-/byteconversie. Volledige bronbuffers worden in plaats gepremultiplied.
  Eerst wordt op alpha>15 met twee pixels marge gecropt; daarna verwijdert een
  vier-verbonden scanline-fill randachtergrond met behoud van ingesloten wit.
  Vier staten in de eigen alphabuffer vervangen volledige RGB-/booleaanse
  maskerkopieën. Runs worden meteen gemarkeerd, zodat geen dubbelen ophopen.
  140 oorspronkelijke beeldgevallen vergelijken 42.716 volledige RGBA-pixels
  zonder tolerantie, inclusief verborgen RGB. Vier RGBA-TIFF-oriëntaties 5..8
  tonen een echte Pillow 11.3.0-afmetingenfout; numerieke TIFF-pixels plus
  expliciete oriëntatie corrigeren alleen die gevallen. Zowel de oorspronkelijke
  foutieve als gecorrigeerde PNG/details blijven als bewijs bewaard.
  TIFF-bronnen en voorbereide PNGs kunnen per vak worden gemengd; dubbelzinnige
  invoer wordt geweigerd. Iedere bron blijft beschermd tegen overschrijven.
  Native PNG-opslag is begrensd, annuleerbaar en atomair met behoud van oudere
  assets. Vijf volledige exports vergelijken acht profielen en 26 PNGs,
  waaronder drie direct voorbereide TIFF-vakken. Alle oudere oraclevelden
  blijven ongewijzigd; de echte CLI controleert nu ook directe TIFF-invoer.
  284 Rust-tests, Clippy zonder nieuwe waarschuwingen en definitieve
  release-readback slagen. Beide fixturefamilies regenereren byte-identiek met
  twee hashseeds. Pillow-afgeleide code heeft de MIT-CMU-notice in bron, binary
  (--licenses) en Windows-artifact. Volledige decoder- en rotatiebudgetten geven
  vóór te grote allocaties een fout; geen onzichtbare verlaging van resolutie.
  Drie verse processen per route meten de reeds gedecodeerde beeldstap:
  zonder rotatie mediaan 5,882 tegenover 66,073 ms; met rotatie
  78,500 tegenover 199,368 ms. Procespieken met rotatie
  27.729.920 tegenover 198.361.088 bytes. Iedere uitvoerbyte is gelijk;
  TIFF-decodering, PNG/DXF-opslag, opstart/imports, invoeropbouw en GUI/netwerk
  vallen buiten de timing. Procespieken bevatten runtime-/invoerbasis en zijn
  geen geïsoleerde allocaties of app-pieken; zie benchmarks/template-raster.json.
  Kaart-/foto-/virtuele rastervoorbereiding, gepaarde normale/omgekeerde eind-
  exports, server-/overzichtscompositie en het volledige native venster blijven open.
- Native A4-kaartvoorbereiding aan de sjabloon-CLI/publicatie gekoppeld.
  De kaart neemt volledige TIFF-pixels, CAD-paden, achtergrond-PNG of PDOK WMS,
  coördinaten/grid, schaalbalk, noordpijl en dezelfde weergegeven profielreferentie.
  54 anti-aliased bilineaire crops/resizes en acht oorspronkelijke volledige
  TIFF-paintbewerkingen vergelijken alle RGBA-bytes zonder tolerantie.
  Een rijcache van 8 MiB voorkomt de volledige horizontale tussenafbeelding.
  Twaalf A4-instellingen vergelijken iedere tekenopdracht en exacte geometrie
  met de oorspronkelijke paginaopbouw bij bevroren bronfontmetingen.
  Tien onafhankelijke HarfBuzz-C-referenties controleren glyphs/posities voor
  Latin/combining, Arabic, Hebrew en Indic. Native ab_glyph is unhinted;
  glyphcontouren en primitive edgepixels zijn geen Pillow/FreeType-pixelkopie.
  De originele en native 300-DPI-bladen zijn visueel gecontroleerd.
  PDOK hergebruikt verbindingen, begrenst reads tijdens streaming en controleert
  PNG/JPEG-afmetingen vóór decode; aansluitende tegels gebruiken één pixelgrid.
  De live proef vond een bestaande PNG-only-fout: PDOK beantwoordt een PNG-
  aanvraag soms met JPEG. Contentdetectie ondersteunt beide, met aparte
  decodeerlimieten en een echte 64×64-respons als regressie (JPEG maximaal drie
  bytewaarden kleurtolerantie, los van alle exacte PNG-vergelijkingen).
  Offline tests toetsen alle pixels over zes tegels en vroeg annuleren.
  Een volledige native 300-DPI-export met live HTTPS PDOK-tegels slaagt ook
  op DXF-readback en echte luchtfotopixels; dit is geen tijdbenchmark.
  Kaartfalen, ontbrekende fonts en annulering na een eerder ingevuld vak houden
  de vorige DXF/bundle intact. Bronpaden worden manifest-relatief opgelost en
  tegen uitvoeraliases beschermd. De echte CLI toetst datum/project, profiel,
  DXF-readback en een pixel-exact TIFF-kaartinterieur.
  297 Rust-tests slagen. De onafhankelijke kaartdriver vergelijkt 66.667 exacte
  RGBA-pixels; de echte CLI nog 53.095 kaartinterieurpixels. Twee hashseeds
  regenereren byte-identieke referenties. Een cachebudget van 100.000 glyphs
  voorkomt groei door veel verschillende teksten; gedeelde layouts vermijden
  herhaalde glyphvector-kopieën. Dit zijn componentbudgetten, geen totale RAM-cap.
  Volledige native bediening, foto-/virtuele beelden, aanvullende providers/cache,
  exacte CAD-tekstweergave en praktijkacceptatie blijven open.
- Native sjabloonvenster en desktopworker gekoppeld. Het venster biedt project-
  naam/tekenaar met automatische datum, PS-nummers/varianten, multi-selectie,
  volgorde, lege/uitgesloten vakken, schalen, profieljob/richting, referentie,
  botsingsvermijding, adres/comments en luchtfoto/BGT-keuzes. De ingevoerde
  waarden blijven een draft tot bevestigen; projectwissels sluiten oud werk.
  Nieuwe bronnen beginnen uitgesloten. Native formaat 4 bewaart deze keuzes
  en opaque rijmetadata; 1/2/3 blijven leesbaar zonder bronbytes te herschrijven.
  Grote datasets/rasters/CAD/comments worden naar de bestaande worker verplaatst
  en bij resultaat, fout, annuleren of panic teruggegeven. Projectacties worden
  pas daarna uitgevoerd. Assemblage leent datasets; alle kaartvakken delen één
  CAD-padvector. BGT-batches worden per gelijke uitsnede hergebruikt, met een
  gezamenlijk vertex-/featurebudget en behoud van de geconstrueerde 8-ULP-
  geometrieafspraak. Profielen uitzetten houdt de gemeten beeldas beschikbaar.
  Het echte sjabloon wordt ingebouwd, zodat de losse executable zonder checkout
  kan exporteren. 140 oorspronkelijke label-/volgordegevallen, 64 bewegingen,
  twintig bestandsnamen en 48 commentbatches vergelijken de sourcehelpers.
  Echte egui klik-/Escape-/projectwisseltests en productie-workerexports
  controleren projectnaam, lokale datum, bronallocaties en uitgesteld sluiten.
  310 Rust-tests slagen, inclusief 26 desktoptests. Vier release-workerexports
  (normaal, omgekeerd, zonder getekend profiel en varianten met leeg vak)
  hebben onafhankelijke DXF-/PNG-readback. De oorspronkelijke vijf samengestelde
  exports en kaart-/CLI-pixelcontroles blijven aparte regressiecontroles.
  Uitvoer naar geladen meetbestanden, CAD, TIFF/sidecar, sjabloon of het huidige
  project-/importbestand wordt vóór netwerk- of assetwerk geweigerd.
  Volledige begeleidende exports/Excel, foto’s/virtuele beelden, oude instellingen
  en praktijkacceptatie blijven open; dit vervangt de globale release-eisen niet.
- Gecombineerde native exportbediening: één DXF-sjabloonmenu voor oorspronkelijke
  TIFF-volgorde/PS-varianten, jobs/virtuele sleuven met werkfoto's en opgeslagen
  exportinvoer. Native v4 bewaart beide instellingen en leest eerdere v3-projecten.
  Alle projectexports delen de invoeroverdracht; TIFF/CAD/comments worden niet
  nogmaals gekopieerd of afzonderlijk teruggezet. Opslaan/autosave en andere
  exports wachten op de actieve worker. Een echte desktopexport controleert
  behouden bronallocaties, kaartassets en voorkeuren na terugkomst.
  Een bestaande fout bij virtuele/MarXact-export zonder externe TIFF is hersteld:
  het kaartblad gebruikt nu het eigen ongeoriënteerde RD-raster via dezelfde
  affine sampler als TIFF. Omgekeerde profielen verplaatsen geen kaartgeometrie.
  Pixelvergelijking met een expliciet RGBA-TIFF controleert bronvoorrang en
  ontbrekende/uitgeschakelde bronnen. 360 Rust-tests slagen (34 desktoptests).
  De release-kaartdriver vergelijkt 41.915 pixels van de virtuele RD-kaart
  met de oorspronkelijke Pillow-schilderfunctie, naast 66.667 eerdere exacte
  rasterpixels. Dit bewijst kaartplaatsing/compositie, geen complete pariteit
  van de oorspronkelijke virtuele rastermaskers.
  Volledige oorspronkelijke fotoannotaties/rastermaskers, begeleidende Excel/
  gepaarde exports en alle globale praktijk-/releasevoorwaarden blijven open.
- Automatisch discipline-Excel gekoppeld aan beide projectexportvensters en de
  bestaande voorkeur; CLI-manifests kunnen `settings.discipline_excel` inschakelen.
  Tellen volgt alle geselecteerde vakken, ongeacht profieltekenen. Lege vakken
  vallen weg, PS-varianten/volgorde blijven behouden, ontbrekende data wordt een
  nulregel met melding. Datasetvertices worden geleend. Een cache van maximaal
  2 MiB aan aantallen en 4096 bronverwijzingen vermijdt herhaald classificeren.
  Dit is een componentbudget, geen totale Excel-/app-geheugenlimiet.
  Na DXF-publicatie volgt een afzonderlijke atomaire XLSX-publicatie. Vervangen
  bewaart de vorige werkmap in de blijvende exportbundle (maximaal 128 MiB).
  Bron-/manifest-/DXF-aliases, vergrendelde bestanden, mislukte backup en rapport-
  panic behouden de DXF en oude werkmap; geen onjuiste melding dat de DXF faalde.
  10 oorspronkelijke werkmappen/60 geordende rijen vergelijken cellen, totalen,
  opmaak, breedtes en filters; twee hashseeds reproduceren de fixturebytes.
  Vijf productie-workerexports vergelijken hun automatische Excel met de bron.
  Een nieuwe desktopbewaking sluit verouderde job-/raster-/sleufkeuzes af, zodat
  gewijzigde bronlijsten niet stilzwijgend een andere meetjob exporteren.
  364 Rust-tests slagen, inclusief 35 desktoptests. De bestaande tien standalone
  werkmappen en samengestelde DXF-/CLI-controles blijven afzonderlijke regressies.
  Gepaard normaal/omgekeerd, overige exportfamilies, echte projecten en alle
  oorspronkelijke globale releasevoorwaarden blijven open.
- Native Normaal/Reverse-containerkern toegevoegd op basis van de werkelijk
  geladen productieketen. De donor wordt in hetzelfde sjabloon gevonden en met
  alle acht metadatarecords gekopieerd; geen tweede sjabloon of DXF-import.
  Twee inhoudsblokken en een wrapper behouden oorspronkelijke objecthandles,
  lagen, afbeeldingen en annotaties. De AutoCAD-keuze heet Versie, begint op
  Normaal en bevat Reverse. Parameter en grip verplaatsen met behoud van hun
  onderlinge afstand; bestaande variantlagen worden ingeschakeld met behoud
  van hun overige eigenschappen. Eén eigendomsindex en een batched wijziging
  vermijden herhaalde imports en indexopbouw per vak.
  De oorspronkelijke functies leveren vijf tekeningen/acht paren en 49 Unicode-
  naamgevallen. Onafhankelijke controle vergelijkt alle donortags/referenties,
  geometrie, laagattributen en DXF-audits zonder reparaties. Ruwe ownertags
  worden apart getoetst omdat ezdxf foutieve gekoppelde owners bij lezen stil
  herstelt; hierdoor is een aanvankelijk gemiste fout daadwerkelijk gevonden.
  Twee hashseeds reproduceren alle fixturebytes. Annuleren, callback-panic,
  ongeldige groepen, bewerkte bronvelden en handle-uitputting laten de bron
  intact; het echte sjabloon behoudt alle oorspronkelijke handled records,
  behalve bijgewerkte tabeltellingen. 368 Rust-tests slagen; Clippy geeft geen
  nieuwe waarschuwingen in deze kern. De geoptimaliseerde app en auditdriver
  bouwen; alle vijf native uitvoertekeningen slagen voor de onafhankelijke
  vergelijking. Windows CI voert dezelfde bronvergelijking uit.
  Dit is de container-/zichtbaarheidskern, nog geen complete gepaarde export:
  dubbele raster-/profielvoorbereiding, de oorspronkelijke fontafhankelijke
  selectorplaatsing, de GUI-koppeling en interactieve AutoCAD-acceptatie blijven
  open, naast alle eerdere globale releasevoorwaarden.
- Fontafhankelijke profielgrenzen en automatische Normaal/Reverse-selectors
  toegevoegd aan de native blokkern. Het broncontract gebruikt
  letteradvances zonder kerning en A/x/p-controlgrenzen, niet de UI-/kaarttekst-rasterizer.
  Native TTF-contourmeting, alle vijftien TEXT-uitlijningen, FIT/ALIGNED,
  attribuutplaatsing, spiegel-/schuinhoek en OCS zijn onafhankelijk vergeleken.
  Een zelfgemaakt testfont bevat afwijkende baseline, descender en kwadratische
  controles; geen geïnstalleerd font wordt meegedistribueerd. 360 oorspronkelijke
  tekstgevallen en 23 caret-/percentcodegevallen worden gereproduceerd. Zes door
  de oorspronkelijke fontmanager opgeloste aanvragen controleren 36 extra
  gevallen met precies dezelfde actuele fontbestanden.
  De grensindex volgt profielblokken/attributen, oorspronkelijke snelle
  curve-controlgrenzen, eenvoudige profielleaders, 2D/3D-geometrie en geknipte
  afbeeldingsgrenzen. 44 oorspronkelijke geometrie-/selectorgevallen en dertien
  vakgroepen uit vijf native sjabloonexports vergelijken grenzen en de werkelijke
  bronhelper voor gripplaatsing. Vergelijkingsbudget: 1e-12 plus 32 maal f64-
  epsilon maal coördinaatgrootte. Verouderde documentindices worden geweigerd;
  ontbrekende/ongeldige gegevens, cycli, nestingbudgetten en annuleren leveren
  geen verzonnen of gedeeltelijke selector. Fontbytes worden gedeeld; advances
  hebben een begrensde cache. Geen claim over totaal app-geheugen of FPS.
  Automatische gripplaatsing en containergeneratie gebruiken één verse DXF-index;
  de herhaald berekende Unicode-sorteersleutels zijn vervangen door gecachte
  sleutels. Zeven volledige tekeningen/twaalf dynamische paren controleren ook
  profielkandidaten en fallback met alleen afbeeldingen. Een mislukte/afgebroken/panikerende
  fontprovider laat bewerkte bronvelden intact. Alle drie fixturefamilies worden
  met twee hashseeds byte-identiek gereproduceerd; Windows CI herhaalt de audits.
  373 Rust-tests slagen, inclusief 35 desktoptests; Clippy heeft geen nieuwe
  waarschuwingen in de gewijzigde code. De definitieve geoptimaliseerde app en
  drie auditdrivers bouwen; alle bovenstaande onafhankelijke controles slagen
  ook met die uitvoer. De vijf samengestelde exports/acht profielen/26 PNGs en
  de echte CLI blijven afzonderlijk gecontroleerd op inhoud, grafen, pixels,
  bronbehoud en veilige publicatie. Alle drie Windows-runs van de vorige stap
  zijn geslaagd.
  Dubbele raster-/profielvoorbereiding, automatische fontdiscovery, de GUI-
  koppeling en interactieve AutoCAD-acceptatie blijven open. MTEXT, SHX/SHP/LFF,
  niet-uniforme bloktekst en overige CAD-families vragen hun volledige eigen
  semantiek. Alle oorspronkelijke globale pariteits-/releasevoorwaarden blijven
  gelden; dit is nog geen productie-release.
- De volledige native exportworker bouwt nu beide richtingen in hetzelfde
  document en verbindt beide native vensters standaard aan Normaal/Reverse.
  Enkelvoudige uitvoer blijft beschikbaar; de keuze wordt in beide soorten
  projectvoorkeuren opgeslagen. Oude voorkeuren zonder veld krijgen de
  productiestandaard, expliciet enkelvoudige keuzes blijven behouden. De
  voorbereide CLI blijft standaard enkelvoudig; `settings.paired` vraagt echte
  TIFF-/virtuele en kaartbronnen voor een betrouwbare tweede richting.
  Windows-lettertypen worden op oorspronkelijke bestandsnaam-/familie-/fallback-
  regels gekozen. Alleen kleine SFNT name/OS2-tabellen worden geïndexeerd;
  glyphdata wordt op gebruik geladen en gedeeld. Vijftien oorspronkelijke
  selecties toetsen bestandsnamen, hoofdletters, families en ontbrekende fonts.
  Bekende SHX/SHP/LFF-files worden expliciet geweigerd totdat hun uitlezer is
  geport, in plaats van een onjuiste selector te maken.
  Normal/Reverse delen bladmetadata, profieltemplates, geleende datasets en de
  BGT-index per vak. Werkfoto's worden eenmaal genormaliseerd; gelijke effectieve
  kaartreferenties hergebruiken de PNG zonder nieuwe kaart-/netwerkrendering.
  Eén eindindex wordt voor ouderhandles, grensmeting en zichtbaarheidsgrafen
  gebruikt. Nieuwe document-/assetfouten, late panic of annulering bewaren de
  vorige DXF en bundle. Fontbronnen worden ook tegen overschrijven beschermd.
  Vijf echte samengestelde scenario's leveren veertien dynamische paren:
  profielen, geen getekend profiel, foto's, virtuele sleuven, alleen beelden,
  dubbele labels, lege vakken en een toegevoegd blad. Beide standen vergelijken
  alle geometrie/gekoppelde attributen/annotaties en PNG-pixels met aparte
  enkelvoudige exports. Het oorspronkelijke promote/finalize/grip-pad bouwt
  een onafhankelijke metadata-oracle. De bestaande owner-0 proxy in het
  goedgekeurde sjabloon blijft tag voor tag behouden; geen nieuwe auditreparatie
  is toegestaan. Alle Excel-onderdelen vergelijken met de enkelvoudige uitvoer,
  afgezien van geldige aanmaak-/wijzigingstijdstempels. Vakken worden eenmaal
  geteld. Beide echte desktopworkers zijn op beide richtingen gecontroleerd,
  inclusief automatische lokale datum, projectnaam, bronallocaties en blokkeren
  van opslaan tijdens de worker. 378 Rust-tests en alle 35 desktoptests slagen.
  Clippy geeft geen nieuwe waarschuwingen. De geoptimaliseerde app en vijf
  auditdrivers bouwen; alle nieuwe controles slagen ook met die uitvoer.
  Vijf eerdere samengestelde exports/acht profielen/26 PNGs, de echte CLI,
  vijf enkelvoudige workerexports met hun Excel en zeven losse dynamische
  containertekeningen/twaalf paren blijven onafhankelijk gecontroleerd.
  Alle drie Windows-runs van de vorige stap zijn geslaagd.
  Oorspronkelijke fotoannotaties/virtuele maskers, overige CAD-/fontfamilies,
  volledige oude voorkeuren en interactieve AutoCAD-/projectacceptatie blijven
  open, naast alle eerdere globale releasevoorwaarden.
- Native SHP/SHX-tekstmeting en projectvaste CAD-fontmappen (21 oktober 2026):
  SHP, legacy SHX 1.0/1.1 en Unicode-programma's worden native ingelezen en
  uitgevoerd voor de automatische profiel-/beeldgrenzen en AutoCAD-keuzegrip.
  De interpreter ondersteunt alle gedefinieerde opdrachten, subtekens met
  gedeelde pen/schaal/stack, octant-/fractionele bogen, bulges en verticale
  voorwaarden. Opdracht-/puntbudgetten, recursie-/cycluscontrole, een vierdiepe
  positiestack en annulering begrenzen foutieve fonts. Ook verplaatsingen zonder
  groeiende puntlijst tellen mee in het werkbudget. Glyphprogramma's worden
  gedeeld; tekstlengten gebruiken een beperkte advance-cache zonder alle
  glyphcontouren vast te houden. Dit bewijst geen totale app-performance.
  Beide exportvensters bieden Extra CAD-lettertypen; mappen worden in beide
  projectvoorkeuren opgeslagen, teruggezet en aan de worker doorgegeven.
  Manifestpaden worden relatief aan het manifest opgelost. Eigen folders
  overschrijven dubbele systeemnamen; ontbrekende SHX/SHP-namen volgen de
  oorspronkelijke txt/iso-fallbackvolgorde. Ook enkelvoudige exports beschermen
  expliciet opgegeven fontbronnen tegen overschrijven.
  Elf zelfgemaakte fonts leveren 2486 oorspronkelijke controletraces en 990
  oorspronkelijke tekst-/uitlijningsgevallen, inclusief gebogen A/x/p,
  Unicode-subtekens, blanco/ontbrekende glyphs, decimale fontkoppen, hexadecimale
  glyphcodes, haakjes, commentaar en vervolgregels. Twee hashseeds produceren
  dezelfde veertien fixturebestanden. Vier gerichte correcties behouden het
  oude bewijs apart: penstand bij subtekens, opeenvolgende verticale voorwaarden,
  ruwe legacy-fontkoppen en de eerste ontbrekende control-glyph die ondanks een
  gecachete spatiebreedte een IndexError veroorzaakt. De laatste gebruikt nu
  direct die oorspronkelijke fallbackbreedte. Acht verse runtime-fonts/120
  tekstgevallen vergelijken de geoptimaliseerde executable met de oorspronkelijke
  functies zonder opgeslagen verwachte uitvoer.
  Zes echte gepaarde exports/zeventien paren/36 leaders vergelijken beide
  richtingen volledig met losse exports en de oorspronkelijke grip-/grafiekcode.
  Een scenario gebruikt daadwerkelijk een eigen SHX-profiellettertype. De
  bestaande proxy blijft behouden; geen nieuwe auditreparatie is toegestaan.
  De eerdere vijf samengestelde exports/acht profielen/26 PNGs, echte CLI,
  vijf enkelvoudige workerexports/Excel en zeven containertekeningen/twaalf paren
  slagen opnieuw. Ook 360 TTF-tekstgevallen/44 geometriegevallen/36 runtime-
  fontgevallen/dertien samengestelde vakgroepen en vijftien systeemfontselecties
  blijven gecontroleerd. Alle 386 Rust-tests (35 desktoptests), Clippy zonder
  nieuwe waarschuwingen en de geoptimaliseerde app/auditdrivers slagen.
  De ezdxf MIT-kennisgeving is opgenomen bij de bron, --licenses, CI-artifact en
  portable ZIP; commerciële fonts worden niet meegeleverd. Windows CI herhaalt
  de nieuwe referenties en runtime-/gepaarde controles. De drie Windows-runs
  van de voorafgaande commit zijn geslaagd; de nieuwe run volgt na push.
  LFF/BIGFONT, SHX-kaartweergave, overige CAD/MTEXT-/fontvarianten, resterende
  oude instellingenmigratie en echte AutoCAD-/projectacceptatie blijven open.
  Alle globale go/no-go-voorwaarden blijven verplicht.
- Native LibreCAD LFF-tekstmeting en gedeelde contouren (21 oktober 2026):
  LFF is aangesloten op dezelfde lokale fontcatalogus, beide exportvensters en
  de automatische profiel-/keuzegrip. Letter-/woordafstand, Unicode-/hex-ID's,
  polylijnen met bestemmingbulges, lege contouren, ontbrekende glyphs en ouder-
  overerving volgen het oorspronkelijke meetcontract. De ouder is een immutable
  snapshot: latere herdefinitie van hetzelfde teken verandert bestaande kinderen
  niet. Gedeelde oudernodes vermijden herhaald kopiëren van contourlijsten;
  alleen maximaal 8192 advances worden gecachet. Een ouder met 2000 polylijnen en
  1000 kinderen behoudt minder dan 512 KiB gemeten programmastructuur. Dat is geen
  claim over totale proces-RAM of app-snelheid. Lege/single-point polylijnen
  verbruiken werkbudget zonder tijdelijke padallocatie. Bestands-/definitie-/
  vertex-/diepte-/puntbudgetten, tekstwerkbudget en annulering begrenzen invoer.
  De oorspronkelijke fallback_lff()-fout wordt afzonderlijk bewezen: zelfs met
  standard.lff ontbreekt de fallback, of txt.shx wordt als LFF gekozen. Native
  code gebruikt de bedoelde standard/iso/simplex-prioriteit. Ongeldige of niet-
  eindige fontdata geeft een duidelijke fout; overige fout-/BIGFONT-fallback-
  compatibiliteit blijft nog te toetsen.
  Zes originele testfonts leveren 71 oorspronkelijke controletraces en 360
  tekst-/uitlijningsgevallen, inclusief gebogen A/x/p, Unicode-decimalen, hex-
  prefixen, ongeldige velden, ontbrekende/lege glyphs, ouderredefinitie, spaties,
  controltekens, alle vijftien uitlijningen, OCS, spiegeling en schuinhoek.
  Twee hashseeds reproduceren alle zeven gegenereerde bestanden. Acht verse
  runtime-fonts/120 gevallen toetsen de geoptimaliseerde executable zonder
  opgeslagen verwachte uitvoer. De bestaande SHP/SHX-boogconstructie gebruikt
  nu dezelfde cubic-kern; alle eerdere 2486 SHX-traces/990 tekstgevallen blijven
  gelijk, inclusief lege bogen. De afzonderlijke padrepresentaties houden hun
  eigen lege-padcontract (LFF NumpyPath versus SHX Path).
  Zeven echte gepaarde exports/twintig paren/48 leaders vergelijken beide
  richtingen, volledige geometrie/annotaties/beeldpixels en Excel. De
  oorspronkelijke grip-/grafiekcode ontvangt hetzelfde werkelijke SHX- of
  LFF-profiellettertype. Geen nieuwe auditreparatie; de bestaande proxy blijft
  behouden. De eerdere samengestelde exports/CLI, workerexports/werkboeken,
  tekst-/geometrie-/systeemfontproeven en donorgrafen slagen opnieuw.
  Alle 392 Rust-tests (35 desktoptests), de laatste gerichte hercontrole,
  Clippy zonder nieuwe waarschuwingen en de geoptimaliseerde app/auditdrivers
  slagen. MIT-kennisgevingen voor LFF en gedeelde path/math-helpers zijn bewaard
  en via --licenses gecontroleerd; CI herhaalt de nieuwe referenties en audits.
  Alle drie Windows-runs van de voorafgaande SHX-commit zijn geslaagd.
  BIGFONT/errorfallbacks, SHX/LFF-kaartweergave, brede CAD-/MTEXT-ondersteuning,
  overige oorspronkelijke instellingen en echte CAD-/projectacceptatie blijven
  open. De volledige oorspronkelijke go/no-go-lijst blijft vereist voor release.
- Oorspronkelijke fontfactory-terugval en correcte allocatietelling (21 oktober 2026):
  De catalogus volgt nu ook de oorspronkelijke fase-afhankelijke terugval voor
  niet-ondersteunde SHX/BIGFONT-programma's, niet-renderbare eerste glyphs en
  ontbrekende bestanden. TrueType en daarna monospace worden uitsluitend volgens
  die regels gekozen. Beschadigde ondersteunde SHX/LFF-data en een onbruikbare
  standaard-TTF blijven fouten; de standaard kan niet naar zichzelf recursen.
  Fontfouten hebben nu een getypeerde oorzaak: invalid, unsupported, unavailable,
  limit of cancelled. Limieten en annuleren blijven fataal zonder stringmatching
  of een stilzwijgend vervangend font. De gedeelde invoerlezer bewaart dezelfde
  gebruikersfouten, met aanvullende getypeerde I/O-/limiet-/annuleerinformatie.
  De werkelijke fontbytegrens wordt bij gebruik gecontroleerd; de metadata-index
  leest alleen beperkte naam-/OS2-tabellen en slaat grote glyphdata niet voortijdig
  over alsof het font ontbreekt. Tijdelijk verdwenen bestanden kunnen later
  herstellen; succesvol geladen data blijft een gedeeld immutable snapshot.
  De gemelde fontbron is na resolution het werkelijke vervangende bestand.
  Een onterechte selectorfout is hersteld: 32 aliassen van één font van 9 MiB
  telden als 288 MiB en overschreden het 256 MiB-budget. Gedeelde Arc-data telt nu
  eenmaal; verschillende allocaties blijven afzonderlijk begrensd. Dit is een
  concrete allocatiebudgetcorrectie, geen claim over totale app-RAM/snelheid.
  Zeventien oorspronkelijke factoryscenario's/32 vragen vergelijken fonttype,
  werkelijk backingbestand, advance, metingen en iedere TEXT-hoek. Hieronder
  BIGFONT, onbekende SHX-handtekeningen, onbruikbare SHP-/TTF-glyphs, geen defaults,
  fatale data, verdwenen bestanden tussen index en laden en bestaande caches na
  verwijderen. Het bronbewijs kijkt naar het font in de glyphcache; aangevraagde
  namen of de laatste cachelookup kunnen een ander bestand beschrijven.
  Een oorspronkelijke TTF-fixture zonder outlines en alle bronverwachtingen
  reproduceren byte voor byte met een andere hashseed. De geoptimaliseerde
  measure_font_resolution-driver herhaalt dezelfde scenario's in eigen tijdelijke
  mappen; de onafhankelijke Python-controle vergelijkt de complete uitvoer.
  Getypeerde limiet-/annuleringsproeven en herstel zonder indexherbouw slagen.
  Acht echte gepaarde exports/drieëntwintig paren/60 leaders vergelijken opnieuw
  beide richtingen, geometrie, beelden, Excel en de oorspronkelijke grip-/
  zichtbaarheidscode. De nieuwe BIGFONT-stijl wordt echt geïndexeerd; de bron-
  factory levert dezelfde vervangende TrueType-data. Geen nieuwe auditreparaties.
  Ook de eerdere samengestelde exports/CLI, workerexports/Excel, SHX/LFF- en
  systeemfontproeven, profielgrenzen en donorgrafen slagen opnieuw.
  Alle 396 Rust-tests (35 desktoptests), de gerichte eindcontrole, Clippy zonder
  nieuwe waarschuwingen en de geoptimaliseerde app/auditdrivers slagen. Windows
  CI reproduceert de factoryfixtures en controleert de driver naast de echte
  gepaarde uitvoer. Alle drie Windows-runs van de voorafgaande LFF-commit slagen.
  Verdere font-/vendor-/mutatiegevallen, viewporttekst/SHX/LFF-stroken, volledige
  CAD-/MTEXT-/projectworkflows en echte acceptatie blijven open. BIGFONT-glyphs
  worden niet geclaimd: deze terugval volgt juist de oorspronkelijke factory.
  Alle oorspronkelijke globale go/no-go-voorwaarden blijven verplicht.
- Eenregelige CAD TEXT is nu op de echte canvas- en sjabloonkaartpaden aangesloten als native TTF/SHP/SHX/LFF-vectorcontouren. 675 oorspronkelijke teksten en 25 geneste/niet-uniforme OCS/MINSERT-instanties controleren iedere controlecoördinaat en WCS-koorden. Bronstijl/inhoud en font-onafhankelijke contouren overleven projectopslag zonder de fonts. Pen-up blijft gescheiden van inkt; fontcache, controles, punten en ook onzichtbaar interpreterwerk zijn begrensd. Echte egui-/kaartproeven, projectbehoud en annulering zijn toegevoegd. Negen verse fonts/270 runtimegevallen toetsen de geoptimaliseerde driver rechtstreeks. De eindcontrole telt 405 geslaagde Rust-tests, inclusief 36 desktoptests; Clippy voegt geen waarschuwingen toe. Vlakvulling, ATTRIB/ATTDEF/MTEXT, verticale/BIGFONT-glyphs, volledige importinstellingen en praktijkacceptatie blijven open. Zie tests/fixtures/cad-text. De Windows-preview overschreed aantoonbaar de oude 35-minutenlimiet; release/auditdrivers worden gezamenlijk gebouwd en de volledige controle krijgt 75 minuten, zonder audits te schrappen.
- Eenregelige gekoppelde ATTRIB-bronnen en contouren zijn aangesloten op ASCII/binary-inleesroute, geneste INSERT/MINSERT, canvas en sjabloonkaarten. Eigencoördinaten worden niet dubbel getransformeerd; 240 oude grid-uitlijningsfouten hebben een expliciete ankercorrectie met bewaard bronbewijs. 471 oorspronkelijke en 1884 verse geoptimaliseerde runtime-instanties controleren geometrie, laag-/kleur-/vlaggedrag en metadata. Dubbele tags blijven geordend; grote brondata wordt door 10.000 instanties gedeeld. Native projecten bewaren contouren en ruwe ingebouwde MTEXT zonder fonts; ongeldige wijzigingen behouden het vorige bestand. ATTDEF, embedded MTEXT-layout, tekstvlakvulling en volledige acceptatie blijven open. Inspectie vond ook een nog open verschil: dxf.py leest document.modelspace(), terwijl de native ENTITIES-inleesroute papier-/modelselectie nog niet gelijk afhandelt. Dit moet vóór release worden opgelost en onafhankelijk getest. Zie tests/fixtures/cad-attrib. De volledige eindcontrole telt 413 geslaagde Rust-tests, inclusief 37 desktoptests; Clippy heeft geen nieuwe waarschuwingen.
- Het gevonden model-/papierselectieverschil is in de inleesroute hersteld: eigenaars hebben dezelfde prioriteit als de oorspronkelijke loader, kinderen volgen hun ouder, papierblokken worden niet getesselleerd en model-BLOCK-inhoud blijft rootgeometrie. Een korte voorafscan indexeert ruimte-eigenaars, ook bij late TABLES, met annulering/limieten. 24 oorspronkelijke ASCII/binary R12–R2018-bestanden/168 roots vergelijken selectie, bronpaden en kaartgrenzen; acht verse runtimebestanden toetsen de echte geoptimaliseerde reader. Het echte canvas controleert fitting/streken. Nieuwe projecten bewaren de ruimtekeuze; oude caches worden niet blind aangepast en krijgen een herlaadmelding. Papierlayoutviewer/clipping, verdere CAD/vendor- en projectacceptatie blijven open. Zie tests/fixtures/cad-space. De volledige controle telt 419 geslaagde Rust-tests, inclusief 38 desktoptests; Clippy heeft geen nieuwe waarschuwingen.
- Eenregelige ATTDEF-kaartweergave volgt nu de oorspronkelijke frontend: de veldnaam wordt getekend, terwijl standaardwaarde en invoerprompt afzonderlijk bewaard blijven. Normale blokdefinities worden vóór font-/contourwerk overgeslagen; gekoppelde ATTRIB-waarden blijven apart zichtbaar. Acht R12–R2018 ASCII/binary tekeningen vergelijken 442 brondefinities en 602 scèneobjecten met oorspronkelijke tagselectie en vectorcontroles. Native projecten bewaren metadata en contouren zonder fonts; ongeldige grote prompts laten het vorige bestand intact. De echte canvas-/sjabloonkaart, zichtbaarheid en oude metadata zonder prompt zijn gecontroleerd. 425 Rust-tests, inclusief 39 desktoptests, en Clippy zonder nieuwe waarschuwingen slagen. Vaste metadata en gesorteerde klassen maken alle negen gegenereerde bestanden byte-reproduceerbaar onder een andere hashseed. Zie tests/fixtures/cad-attdef. Acht verse geoptimaliseerde runtimebestanden vergelijken 602 scèneobjecten/402 rootdefinities. De laatste codecontrole voegt een gedeelde index per fysiek blok toe; 2000 definities/20.000 geneste cellen/40.000 zichtbaarheidsentries bewaren brongegevens eenmaal. Een gecontroleerde GNU-releasevergelijking van uitsluitend deze indexwijziging meet bij 10.000 definities/20.000 cellen mediaan 0,607→0,120 s (5,07×), met identieke scènebytes; dit is geen totale app-/RAM-claim. Eerdere attribuut-/ruimteaudits, workerexports met datum/projectnaam/Excel, vijf samengestelde exports/acht profielen/26 PNGs, echte CLI en acht gepaarde exports/23 paren/60 leaders slagen opnieuw. De drie Windows-controles op voorafgaande commit 17a68e1 zijn geslaagd. Embedded MTEXT/multiline ATTDEF, tekstvlakvulling, verdere CAD/vendor-/projectacceptatie en alle globale gates blijven open.
- De native MTEXT-opmaakparser vormt nu de eerste kern voor de volledige tekstlayouter: 976 oorspronkelijke gevallen/3467 tokens/14 expliciete fouten of finite guards vergelijken woorden, escapes/carets, breuken, font-/kleur-/decoratie-/alinea-/tabwissels en alle 66 Unicode-14-cijferfamilies, met propertytokens aan/uit. Opmaak-, font- en tabdata blijft gedeeld; 100.000 tokens met een grote fontnaam en 20.000 tabstops met duizenden wijzigingen controleren dit. Ongewijzigde tabarrays worden niet opnieuw per scalarwijziging gevalideerd. Annulering, context-/token-/bronlimieten en overflow zijn getypeerd en geven geen gedeeltelijk resultaat. 1104 verse runtimegevallen/4246 tokens/14 fouten of guards komen overeen met de oorspronkelijke parser; echte 32/64 MiB-grensproeven controleren atomaire uitvoer en cleanup. Alle 430 Rust-tests (39 desktoptests), Clippy zonder nieuwe waarschuwingen, de geoptimaliseerde app/driver en de tools/text.py-licentieproef slagen. De volledige MTEXT-regel-/kolomlayout, glyphgeometrie, bron-/projectintegratie en kaart-/embedded-weergave zijn de volgende noodzakelijke stappen, geen reeds bereikte pariteit. Zie tests/fixtures/mtext. De oorspronkelijke globale releasevoorwaarden blijven volledig van kracht.
- De gemeten-cel MTEXT-layouter is native geport: 369 oorspronkelijke gevallen/8747 primitieve onderdelen vergelijken alle kaders, regel-/cel-/breukposities, tablocks, uitvulling, doorlopende alinea's/kolommen, marges en negen ankers. Bestaande clone-/leadingregels en expliciete oorspronkelijke fouten blijven behouden. Genormaliseerde celbereiken blijven gedeeld bij splitsing; kolomhoogtes worden met oorspronkelijke optelgroepering gecachet. Een index met gesorteerde posities en suffix-minima bewaart de eerste geschikte oorspronkelijke tab, ook bij ongesorteerde lijsten. Proeven met 3000 alinea's/20.000 gedeelde tabstops en 50.000 lege alinea's slagen. De JSON-audittransportlaag is tussen parser en layouter gedeeld en atomair/begrensd. 425 verse layouts/928 kolommen/11.700 primitieve onderdelen/77 oorspronkelijke fouten komen overeen met de geoptimaliseerde driver. De nieuwe runtimecontrole vond en verifieerde herstel van een decimale JSON-inleesfout bij tagged types/arbitrary_precision; de gedeelde controlelaag volgt nu de Value-route van de projectreader. Ook de 1104 parsergevallen met extra decimale tabstops en uitvoergrenzen slagen opnieuw. Alle 435 Rust-tests (39 desktoptests), Clippy zonder nieuwe waarschuwingen en de geoptimaliseerde app/drivers slagen; de drie Windows-controles op 0ca51fc zijn geslaagd. De afzonderlijke oorspronkelijke simple-MTEXT-route, font-/parserbrug, glyphgeometrie en daadwerkelijke CAD-/project-/kaartintegratie blijven open, evenals alle globale releasevoorwaarden. Zie tests/fixtures/mtext-layout.
- Volgende stappen: volledige templates/profielen en
  overige instellingen; tekst/ATTRIB-weergave,
  volledige meshoppervlakken, HATCH, dynamische
  blokken, volledige stijlen en CAD-/profiel-/projectworkflows. Overige mijlpalen,
  fijnmaziger afbreken van zware parserfasen en echte acceptatie nog open.

Per afgeronde stap worden bron, tests, resterende beperkingen en metingen bijgewerkt.
PR #116 blijft concept totdat **alle** releasevoorwaarden aantoonbaar gehaald zijn.
