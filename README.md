# WorldMap Infrastructure Explorer

## Was ist das?

Eine Echtzeit-Weltkarte, die globale Infrastruktur und Logistik auf einer einzigen interaktiven Oberfläche visualisiert: Flugzeuge, Schiffe, Wetterdaten, Energieversorgung, Verkehr, Häfen, Kernkraftwerke, Pipelines und mehr — live, auf Klick.

## Business-Hintergrund

Lieferketten, Energieversorgung und Mobilität sind heute eng miteinander verknüpft. Unterbrechungen — ein gestrandetes Containerschiff, ein Sturm über einem Knotenpunkt, ein Stromausfall in einer Region — haben schnell globale Folgen. Klassische Dashboards zeigen einzelne Datensätze isoliert; der Kontext fehlt.

**WorldMap Infrastructure Explorer** bietet einen integrierten Blick auf mehrere Infrastrukturebenen gleichzeitig. Typische Anwendungsfälle:

- **Risiko & Resilienz**: Versicherungsunternehmen, Rückversicherer und Risikoabteilungen können in Echtzeit sehen, ob ein Schadenereignis (Sturm, Ausfall) mehrere Infrastruktursysteme gleichzeitig trifft.
- **Supply-Chain-Monitoring**: Logistik- und Handelsunternehmen verfolgen Schiffe und Flüge mit vollständigen AIS-Daten (IMO, Kurs, Zielhafen, ETA, Ladung) und können Verzögerungen frühzeitig erkennen.
- **Energie & Infrastruktur**: Netzbetreiber und Analysten sehen Hochspannungsleitungen, Pipelines und Kraftwerke gemeinsam mit Wetterdaten und können Engpässe räumlich einordnen.
- **Behörden & Lagebilder**: Behörden nutzen die kombinierte Ansicht (Schiffe, AtoN-Seezeichen, SAR-Luftfahrzeuge, Verkehr) als operatives Lagebild für Krisenmanagement oder Grenzüberwachung.

Das Projekt ist als selbst gehostete, leichtgewichtige Plattform konzipiert — ein einziges Rust-Binary, kein externes Datenbank-Cluster, deploybar auf einem einzelnen Server.

## Stack

- **Backend**: Rust + Axum 0.8 + rusqlite (single binary, no external DB)
- **Frontend**: React 19 + TypeScript + Vite + MapLibre GL JS + deck.gl
- **Storage**: SQLite — `cache.db` (API cache), `static.db` (POIs), `*.mbtiles` (vector tiles)
- **Ingestion**: Python scripts to populate `static.db`

## Quick Start

```sh
make setup   # install deps, create venv, run ingestion scripts
make dev     # start backend + frontend dev servers
```

That's it. Backend runs on `http://localhost:3000`, frontend on `http://localhost:5173`.

## Make Targets

| Target | Description |
|---|---|
| `make setup` | One-time setup: install deps, create venv, ingest data |
| `make dev` | Start backend (cargo run) + frontend (vite dev) in parallel |
| `make build` | Production build: Rust release binary + Vite build |
| `make ingest` | Re-run all Python data ingestion scripts |
| `make tiles` | Build vector tiles from GeoPackage sources |
| `make clean` | Remove build artifacts (frontend/dist, backend/target) |
| `make help` | Show all available targets |

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) >= 20
- Python 3.10+ (for ingestion scripts)
- Optional: [tippecanoe](https://github.com/felt/tippecanoe) + GDAL (for building `.mbtiles` tiles)

## Manual Setup

If you prefer not to use `make setup`:

```sh
# Backend
cd backend
cp .env.example .env   # fill in API keys
cargo run
```

```sh
# Frontend
cd frontend
npm install
npm run dev
```

```sh
# Data ingestion
cd scripts
python3 ingest_airports.py
python3 ingest_seaports.py
python3 ingest_reactors.py
```

## Environment Variables

| Variable | Required | Description |
|---|---|---|
| `AISSTREAM_API_KEY` | Yes | AIS ship tracking stream |
| `TOMTOM_API_KEY` | No | Traffic flow data |
| `DATA_DIR` | No | Data directory (default: `../data`) |
| `FRONTEND_DIR` | No | Frontend dist (default: `../frontend/dist`) |
| `BIND_ADDR` | No | Listen address (default: `0.0.0.0:3000`) |
| `RUST_LOG` | No | Log level filter (default: `info`) |

## Free API Sign-Up Links

| Service | URL | Key Required |
|---|---|---|
| OpenSky Network | <https://opensky-network.org/index.php?option=com_users&view=registration> | No (anonymous, rate-limited) |
| AISstream | <https://aisstream.io> | Yes (free) |
| Open-Meteo | <https://open-meteo.com> | No |
| TomTom Traffic | <https://developer.tomtom.com/sign-up> | Yes (free 2,500 req/day) |
| Nominatim (geocoding) | Direct use, 1 req/s limit | No |
| OpenFreeMap | <https://openfreemap.org> | No |

## Tile Data

Place `.mbtiles` files in `data/tiles/`. They are auto-discovered at startup and served via `/tiles/{source}/{z}/{x}/{y}`.

Run `make tiles` to build tiles from GeoPackage sources (requires tippecanoe + GDAL).

## Project Structure

```
worldmap/
├── backend/        # Rust Axum server
├── frontend/       # React + Vite SPA
├── scripts/        # Python ingestion & tile build scripts
├── Makefile        # Build & run targets
└── data/
    ├── tiles/      # .mbtiles vector tile files
    ├── cache.db    # API response cache (auto-created)
    └── static.db   # POI data (populated by scripts)
```
