# Werkplan volledige native Rust-migratie

Doel: volledige functiepariteit, aantoonbaar betere prestaties waar mogelijk,
en alle gevonden regressies herstellen. Geen claim van gegarandeerde bugloosheid.
`PARITY.md` blijft de acceptatiechecklist; een compilatie is geen productacceptatie.

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

## Voortgang 17–18 oktober 2026

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
- Rasterdecoder verwerkt strips/tegels naar begrensde previews op één achtergrondthread,
  ondersteunt meer samples/kleurtypen en verdeelt een geschat textuurbudget over lagen.
  47 rasterfixtures (Pillow + numerieke verwachtingen) en 93 Rust-tests groen.
  Planar RGB16 corrigeert een aangetoonde Pillow-bytefout; NoData/nonfinite transparant.
  Releasebenchmark op 16,8 miljoen RGB8-pixels: circa 2,3× sneller en 86% lagere
  procespiek dan volledige Pillow-conversie, bij gelijke previewpixels. Overviews,
  viewportresolutie en volledige raster-/gebruikersacceptatie blijven open.
- Geneste MULTILEADER-labels lezen echte pijllijnen en MTEXT afzonderlijk.
  Een echte synthetische ezdxf R2018-serialisatie is met de oorspronkelijke
  Python-parser vergeleken (11 labels, 3 inserts, 1 gesloten en 1 open contour).
  Open contouren worden terecht overgeslagen; labeltekst volgt eerste match en
  numerieke hoogteprefix. Oudere overlays zonder gesloten-vlag blijven leesbaar.
  97 Rust-tests groen; bloktransformaties en sjablonen blijven open.
- ASCII/binary DXF-invoer deelt nu één geometriepad, leest tags incrementeel,
  gebruikt HEADER-codepagina/UTF-8 en weigert beschadigde/afgebroken bestanden.
  Vier ezdxf ASCII/binary-paren (R12/R2000/R2018) en binaire geneste MarXact
  vergeleken; scalaire/XDATA-chunk- en afbreekregressies toegevoegd.
  105 Rust-tests groen. Geen volledige attributen/XDATA, OCS/bulges,
  bloktransformaties of sjablonen; bronbuffer en uitvoergeometrie blijven in RAM.
- CAD-palet/true-color/laagstatus en INSERT-attributen/XDATA geïmplementeerd,
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
- MarXact-labels worden ruimtelijk geïndexeerd met gedeelde bronnen en deduplicatie
  van gelijke XY-posities. De Python-tievolgorde corrigeert een native afwijking;
  extra buurcellen herstellen Python-missers op ties-to-even-celgrenzen.
  10.018 onafhankelijke referentievragen, 36 expliciete afrondingscorrecties,
  100.000 dubbele labels en zeer grote coördinaten getest. 128 Rust-tests groen.
  Releasekernmeting: indexopbouw plus 20.000 vragen circa 63× sneller dan de oude
  native lineaire scan op 20.000 verspreide labels; geen volledige importmeting.
- Hoofdletterongevoelige Unicode-laagverwijzingen behouden nu de juiste kleur,
  zichtbaarheid en blokovererving zonder bronnaamverlies of normalisatie per frame.
  Elf ezdxf-referenties in ASCII/binary; opslag, cache/snapshot, oude conflicterende
  varianten en bronherladen bij gewijzigde schrijfwijze getest. 132 Rust-tests groen.
- SPLINE/NURBS met controlepunten, positieve gewichten en CAD-fitpunten geport,
  inclusief natuurlijke/clamped eindvoorwaarden, ongeklemde/periodieke curves,
  bronparameters en gescheiden paden. Bézier-hullafvlakking na WCS-transformatie,
  hergebruikte spanbuffer en lineaire tridiagonale fitoplossing.
  Twintig ezdxf-bronnen, 60 wereldcurves/63 paden, graad 1/3/5/11, opslag,
  numerieke fouten, budgetten en 20.000 fitpunten vergeleken/getest.
  Exacte JSON-floatrondgangen herstellen een aangetoonde precisiefout bij openen;
  grote getallen/decimalen in het Python-archief worden niet naar f64 afgerond.
  139 Rust-tests groen; negatieve/nulgewichten en volledige CAD-acceptatie open.
- Lokale invoer loopt via één begrensde achtergrondworker: native openen/herstel,
  Python-import, DXF toevoegen/herladen/herkoppelen, MarXact, jobFeatures en
  GeoTIFF-metadata, inclusief bestanden uit live downloads. Voorbereiding van
  CAD-zichtbaarheid en project-/MarXact-markers gebeurt vóór toepassing.
  Voortgang/annuleren, behoud van huidig werk bij fouten, batches met losse
  fouten en projectgeneraties zijn gekoppeld aan de echte UI. Nieuwe regressies
  blokkeren de worker doelbewust terwijl een egui-frame Escape verwerkt of een
  nieuw project begint; late resultaten veranderen het huidige project niet.
  Gedeelde invoercontrole begrenst werkelijke bytes, corrigeert de oude
  jobFeatures-groeirace en herhaalt onderbroken systeemlezingen. 151 Rust-tests
  groen. Annuleren is coöperatief: één systeemaanroep, JSON-/geometrie-/cachefase
  kan nog aflopen; opslaan/export/back-ups blijven synchroon. Geen totale
  geheugen-/rendergarantie of volledige workflowacceptatie.
- Volgende stappen: tekst/ATTRIB-weergave, meshes, HATCH, dynamische
  blokken, volledige stijlen en CAD-/profiel-/projectworkflows. Overige mijlpalen,
  fijnmaziger afbreken van zware parserfasen en echte acceptatie nog open.

Per afgeronde stap worden bron, tests, resterende beperkingen en metingen bijgewerkt.
PR #116 blijft concept totdat **alle** releasevoorwaarden aantoonbaar gehaald zijn.
