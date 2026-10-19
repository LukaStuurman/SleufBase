# BGT-oppervlakteksten uit voorbereide geometrie

`tools/generate_bgt_surface_fixtures.py` roept de oorspronkelijke
`_dominant_bgt_physical_appearance` en `_template_bgt_surface_texts` aan.
Referentie: Python 3.11 / Unicode 14, Shapely 2.0.7 en GEOS. Het bronpakket
wordt geïsoleerd geïmporteerd, zonder app-opstart, instellingen of netwerk.

541 oorspronkelijke gevallen dekken gaten en hun randen, aanrakingen, nul-
en heel korte assen, de grens van 1e-8 meter, overlappende onderdelen van één
feature, samengetelde afzonderlijke features, naamgroepen met Unicode-casefold,
eerste schrijfwijze en lexicografische tievolgorde. Er zijn 520 reproduceerbare
gevallen met rechthoeken, concave vlakken met gaten, convexe hulls en gescheiden
multipolygonen, afwisselend rond de oorsprong en op RD-coördinaten.

De native invoer bestaat uit **geldige, voorbereide geometrie**. Polygonen,
multipolygonen, lijn-/puntgeometrie en collecties worden ondersteund. Eenmaal
voorbereide namen en begrenzingen worden hergebruikt; de geometrie blijft
geleend. Snij-intervallen worden per feature verenigd, waarna afzonderlijke
features met dezelfde naam worden opgeteld. Robuuste oriëntatietekens komen uit
[GeoRust robust](https://github.com/georust/robust). Het sweep-algoritme sorteert
kruisingen; het doet geen nieuwe volledige punt-in-polygoonscan per interval.
Een onafhankelijke regressie met 12.000 inkepingen/ruim 60.000 vertices bewaakt
dit geval. Gatenranden blijven inbegrepen, zoals bij GEOS-intersectie.

De drie proeflijnen liggen één meter vóór de sleuf, over de sleufas en één
meter erna. De oorspronkelijke lengte-/asgrenzen en naamprioriteit blijven
behouden. Annulering, ongeldige/niet-eindige coördinaten, te diepe collecties,
te veel features/vertices en ongeldige teksten worden begrensd. De kern vervangt
geen topologieherstel. Native tegelinvoer/RD en HTTP/cache zijn inmiddels apart
beschikbaar; make-valid, union per bronfeature en clipping blijven te porteren.

`resolve_bgt` koppelt deze uitkomst aan dezelfde handmatige tekst-/kleurprioriteit.
De complete profielgenerator gebruikt nu echte BGT-polygonen in plaats van een
voorgegeven tekstfunctie. Alle oudere oraclevelden en het blok-DXF zijn daarbij
ongewijzigd; alleen de geometrie-invoer is toegevoegd. De native exportdriver
berekent deze teksten zelf voordat de profieltekening wordt opgebouwd.

```powershell
python tools/generate_bgt_surface_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test bgt_surface
cargo build --release --manifest-path rust/Cargo.toml --example benchmark_bgt_surface
python tools/benchmark_bgt_surface.py rust/target/release/examples/benchmark_bgt_surface.exe rust/benchmarks/bgt-surface.json
```

De benchmark leest dezelfde 541 geometrieën buiten de meettijd en voert tien
rondes uit in elk van drie afzonderlijke native processen. Beide kanten maken
dezelfde tekstchecksum. Het rapport meet queries en rapporteert native index-
voorbereiding apart. JSON-/geometriedecodering, netwerk, procesopstart, GUI/DXF
en procespiekgeheugen zijn uitgesloten. Dit is geen meting van de volledige app.
