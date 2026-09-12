# Data source notices

[Back to WorldMap](../README.md)

## Existing project notice

### Important: Read Before Use

This software is provided **for educational and research purposes**. By using this software, you acknowledge and accept full responsibility for ensuring your use complies with all applicable laws and regulations in your jurisdiction.

### Data Source Terms of Service

This application aggregates data from multiple third-party APIs. **Each data source has its own terms of service, rate limits, and usage restrictions.** It is your responsibility to:

1. **Read and comply with the terms of service** of every API you connect to
2. **Respect rate limits** — exceeding them may violate the provider's ToS and result in your access being revoked
3. **Verify commercial use rights** — some APIs (notably Open-Meteo, Nominatim, OpenFreeMap) are free for non-commercial use only. Commercial use may require a paid license or explicit permission

### AIS & Maritime Data

- AIS (Automatic Identification System) data is broadcast publicly over radio frequencies. Receiving and displaying AIS data is generally legal in most jurisdictions.
- However, **redistributing, storing, or commercially exploiting AIS data** may be subject to national maritime regulations and the data provider's terms.
- Some jurisdictions restrict tracking of military, government, or certain flagged vessels. Ensure compliance with local maritime law.

### Aviation Data (OpenSky Network)

- OpenSky Network data is provided under their specific [terms of use](https://opensky-network.org/about/terms-of-use).
- Tracking military aircraft or using flight data for surveillance purposes may be restricted or illegal in certain jurisdictions.
- If you use OpenSky data in academic publications, proper citation is required.

### Web Scraping & API Usage

- The ingestion scripts in `scripts/` fetch data from various public sources (OurAirports, OpenStreetMap Overpass, GeoNuclearData).
- **Automated data collection may violate certain websites' terms of service**, even when the data itself is publicly available.
- Overpass API (OpenStreetMap) has strict [usage policies](https://operations.osmfoundation.org/policies/nominatim/). Heavy or abusive querying is prohibited.
- Always use appropriate request intervals and respect `robots.txt` where applicable.

### GIS & Map Data

- OpenStreetMap data is licensed under [ODbL](https://opendatacommons.org/licenses/odbl/). If you distribute derived datasets, you must comply with ODbL attribution and share-alike requirements.
- GeoPackage data used for pipeline and power grid tiles may originate from government open-data portals with their own license terms.

### Nuclear Facility Data

- Nuclear reactor locations are sourced from [GeoNuclearData](https://github.com/cristianst85/GeoNuclearData), which compiles publicly available IAEA data.
- Displaying nuclear facility locations is legal in most countries, as this information is publicly available through the IAEA. However, combining it with other operational data could raise security concerns in some jurisdictions.

### No Warranty

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND. THE AUTHORS ARE NOT LIABLE FOR ANY CLAIM, DAMAGES, OR OTHER LIABILITY ARISING FROM THE USE OF THIS SOFTWARE OR THE DATA IT ACCESSES. See [LICENSE](../LICENSE) for the full MIT license text.

### Your Responsibility

- **Do not use this tool for illegal surveillance, military intelligence, or any unlawful purpose.**
- **Do not redistribute third-party data** without verifying you have the right to do so.
- **Comply with GDPR** and equivalent data protection laws if you store or process data that could identify individuals (e.g., vessel crew, aircraft operators).
- When in doubt, consult a legal professional in your jurisdiction.

---
