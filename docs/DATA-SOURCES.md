# Data sources and notices

[Back to WorldMap](../README.md)

WorldMap combines third-party data. Each source has its own license, terms and rate limits; you are responsible for complying with them, especially for commercial use or redistribution.

| Data | Provider | License / terms | Notes |
|---|---|---|---|
| Basemap | [CARTO](https://carto.com/basemaps) with [OpenStreetMap](https://www.openstreetmap.org/copyright) data | CARTO basemap terms; ODbL | Free for non-commercial use; larger or commercial use needs a CARTO plan |
| Aircraft positions, tracks, flight history | [OpenSky Network](https://opensky-network.org) | [OpenSky terms of use](https://opensky-network.org/about/terms-of-use) | Anonymous access is heavily rate-limited; flight history needs an account; cite OpenSky in publications |
| Vessel positions, navigation aids | [AISstream.io](https://aisstream.io) | AISstream terms | Needs an API key; storing or redistributing AIS data may be regulated in your country |
| Weather | [Open-Meteo](https://open-meteo.com) | CC BY 4.0; free for non-commercial use | Commercial use needs an API plan |
| Road traffic | [TomTom](https://developer.tomtom.com) | TomTom terms | Free tier limits apply (map tiles per day) |
| Place search | [Nominatim](https://nominatim.org) / OpenStreetMap | ODbL; [usage policy](https://operations.osmfoundation.org/policies/nominatim/) | WorldMap sends at most one request per second, caches results and never autocompletes |
| Airports | [OurAirports](https://ourairports.com/data/) | Public domain | Large and medium airports |
| Seaports | [NGA World Port Index (Pub. 150)](https://msi.nga.mil/Publications/WPI) | U.S. government work, public domain | |
| Nuclear plants | [GeoNuclearData](https://github.com/cristianst85/GeoNuclearData) (IAEA PRIS / WNA derived) | ODbL | Units grouped into plants by location |
| High-voltage lines, pipelines | OpenStreetMap via [Overpass API](https://wiki.openstreetmap.org/wiki/Overpass_API) | ODbL | Derived tiles are a database under ODbL (attribution and share-alike apply if you distribute them); respect the public Overpass servers' usage policy |
| Pipelines (optional) | [OGIM](https://zenodo.org/records/15103476), Environmental Defense Fund / MethaneSAT | CC BY 4.0 | |
| Estimated power grid | [Gridfinder](https://zenodo.org/records/3628142), Arderne et al. 2020 | CC BY 4.0 | Modelled from night-time lights; not a survey |

## Responsible use

- AIS and flight data are broadcast publicly, but tracking specific vessels or aircraft can be restricted by law in some places. Do not use WorldMap for unlawful surveillance.
- Infrastructure locations come from public sources. Combining them with other data can still be sensitive; share derived datasets thoughtfully.
- If you store or process data about identifiable people (for example crews or operators), data protection law such as the GDPR applies.

## No warranty

The software is provided "as is", without warranty of any kind. Data can be incomplete, delayed or wrong. See [LICENSE](../LICENSE).
