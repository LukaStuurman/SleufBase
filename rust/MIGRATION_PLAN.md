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

## Stand 17 oktober 2026

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
- Volgende stappen: verdere CAD-/renderpariteit en volledige
  projectworkflows. Overige mijlpalen en representatieve acceptatie nog open.

Per afgeronde stap worden bron, tests, resterende beperkingen en metingen bijgewerkt.
PR #116 blijft concept totdat **alle** releasevoorwaarden aantoonbaar gehaald zijn.
