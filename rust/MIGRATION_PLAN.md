# Werkplan volledige native Rust-migratie

Doel: volledige functiepariteit, aantoonbaar betere prestaties waar mogelijk,
en alle gevonden regressies herstellen. Geen claim van gegarandeerde bugloosheid.
`PARITY.md` blijft de acceptatiechecklist; een compilatie is geen productacceptatie.

Aanvullende gebruikerswens: projectnaam invoeren in het DXF-sjabloonexportvenster
en automatisch de lokale exportdatum op ieder gebruikt blad invullen.

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
5. **Externe diensten**: KickTheMap browserauth/herstel/fallbacks, StreetSmart,
   AHN-profielen en AI-workflows. Native netwerkcode met lokale foutscenario-tests;
   accountgebonden acceptatie apart aantonen met een beschikbare sessie.
6. **Dagelijkse bediening en instellingen**: alle menu's, shortcuts, drag/drop,
   objectbewerking, disciplines/Excel, configuratie en foutmeldingen.
7. **Prestaties en levering**: herhaalbare datasets en benchmarks met tijd/geheugen,
   Windows portable/installer/update/rollback, daadwerkelijke acceptatie en signing.

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
- Volgende stappen: volledige templates/profielen en
  overige instellingen; tekst/ATTRIB-weergave,
  volledige meshoppervlakken, HATCH, dynamische
  blokken, volledige stijlen en CAD-/profiel-/projectworkflows. Overige mijlpalen,
  fijnmaziger afbreken van zware parserfasen en echte acceptatie nog open.

Per afgeronde stap worden bron, tests, resterende beperkingen en metingen bijgewerkt.
PR #116 blijft concept totdat **alle** releasevoorwaarden aantoonbaar gehaald zijn.
