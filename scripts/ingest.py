#!/usr/bin/env python3
"""Import static datasets into data/static.db.

    python scripts/ingest.py            # everything
    python scripts/ingest.py airports seaports

Datasets:
  airports  OurAirports - large and medium airports (public domain)
  seaports  NGA World Port Index, Pub. 150 (public domain)
  reactors  GeoNuclearData - operating, suspended and under-construction units (ODbL)

A running WorldMap server picks up the new data automatically.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import sys

from common import connect_static_db, fetch, log

OURAIRPORTS = "https://davidmegginson.github.io/ourairports-data"
WPI_CSV = "https://msi.nga.mil/api/publications/download?type=view&key=16920959/SFH00000/UpdatedPub150.csv"
GEONUCLEAR = "https://raw.githubusercontent.com/cristianst85/GeoNuclearData/master/data/json/raw"


def _float(value: str | None) -> float | None:
    try:
        v = float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None
    return v if v == v else None  # NaN check


def _text(value: str | None) -> str | None:
    value = (value or "").strip()
    return value or None


def ingest_airports() -> int:
    log("Airports: downloading OurAirports...")
    countries = {row["code"]: row["name"] for row in csv.DictReader(io.StringIO(fetch(f"{OURAIRPORTS}/countries.csv").decode("utf-8")))}
    reader = csv.DictReader(io.StringIO(fetch(f"{OURAIRPORTS}/airports.csv", timeout=180).decode("utf-8")))
    rows = []
    for row in reader:
        kind = {"large_airport": "large", "medium_airport": "medium"}.get(row.get("type", ""))
        if not kind:
            continue
        lat, lon = _float(row.get("latitude_deg")), _float(row.get("longitude_deg"))
        if lat is None or lon is None or not (-90 <= lat <= 90 and -180 <= lon <= 180):
            continue
        ident = _text(row.get("icao_code")) or _text(row.get("gps_code")) or _text(row.get("ident"))
        rows.append((
            ident,
            _text(row.get("iata_code")),
            row["name"].strip(),
            _text(row.get("municipality")),
            countries.get(row.get("iso_country", ""), _text(row.get("iso_country"))),
            kind,
            lat,
            lon,
            _float(row.get("elevation_ft")),
            1 if row.get("scheduled_service") == "yes" else 0,
        ))
    if len(rows) < 1000:
        raise RuntimeError(f"only {len(rows)} airports parsed; the source format may have changed")
    conn = connect_static_db()
    with conn:
        conn.executescript(
            """DROP TABLE IF EXISTS airports;
            CREATE TABLE airports (
                id INTEGER PRIMARY KEY AUTOINCREMENT, ident TEXT, iata TEXT, name TEXT NOT NULL, city TEXT, country TEXT,
                kind TEXT NOT NULL, lat REAL NOT NULL, lon REAL NOT NULL, elevation_ft REAL, scheduled INTEGER NOT NULL DEFAULT 0
            );"""
        )
        conn.executemany(
            "INSERT INTO airports (ident, iata, name, city, country, kind, lat, lon, elevation_ft, scheduled) VALUES (?,?,?,?,?,?,?,?,?,?)",
            rows,
        )
    conn.close()
    large = sum(1 for r in rows if r[5] == "large")
    log(f"  OK {len(rows):,} airports ({large:,} large)")
    return len(rows)


SIZES = {"large": "large", "medium": "medium", "small": "small", "very small": "very_small"}


def ingest_seaports() -> int:
    log("Seaports: downloading the World Port Index...")
    text = fetch(WPI_CSV, timeout=180).decode("utf-8-sig")
    rows = []
    for row in csv.DictReader(io.StringIO(text)):
        lat, lon = _float(row.get("Latitude")), _float(row.get("Longitude"))
        name = _text(row.get("Main Port Name"))
        if lat is None or lon is None or not name or not (-90 <= lat <= 90 and -180 <= lon <= 180):
            continue
        locode = (row.get("UN/LOCODE") or "").replace(" ", "").upper() or None
        size = SIZES.get((row.get("Harbor Size") or "").strip().lower())
        wpi = row.get("World Port Index Number", "").strip()
        rows.append((int(wpi) if wpi.isdigit() else None, name, locode, _text(row.get("Country Code")), size, _text(row.get("Harbor Type")), lat, lon))
    if len(rows) < 1000:
        raise RuntimeError(f"only {len(rows)} ports parsed; the source format may have changed")
    conn = connect_static_db()
    with conn:
        conn.executescript(
            """DROP TABLE IF EXISTS seaports;
            CREATE TABLE seaports (
                id INTEGER PRIMARY KEY AUTOINCREMENT, wpi INTEGER, name TEXT NOT NULL, locode TEXT, country TEXT,
                harbor_size TEXT, harbor_type TEXT, lat REAL NOT NULL, lon REAL NOT NULL
            );"""
        )
        conn.executemany("INSERT INTO seaports (wpi, name, locode, country, harbor_size, harbor_type, lat, lon) VALUES (?,?,?,?,?,?,?,?)", rows)
    conn.close()
    log(f"  OK {len(rows):,} seaports")
    return len(rows)


STATUSES = {2: "Under Construction", 3: "Operational", 4: "Suspended Operation"}


def ingest_reactors() -> int:
    log("Nuclear reactors: downloading GeoNuclearData...")
    countries = {c["Code"]: c["Name"] for c in json.loads(fetch(f"{GEONUCLEAR}/1-countries.json"))}
    types = {t["Id"]: t["Type"] for t in json.loads(fetch(f"{GEONUCLEAR}/3-nuclear_reactor_type.json"))}
    plants = json.loads(fetch(f"{GEONUCLEAR}/4-nuclear_power_plants.json"))
    rows = []
    for unit in plants:
        status = STATUSES.get(unit.get("StatusId"))
        lat, lon = _float(unit.get("Latitude")), _float(unit.get("Longitude"))
        if not status or lat is None or lon is None:
            continue
        capacity = _float(unit.get("Capacity"))
        rows.append((
            unit["Name"].strip(),
            countries.get(unit.get("CountryCode", ""), unit.get("CountryCode") or "Unknown"),
            lat,
            lon,
            capacity if capacity and capacity > 0 else None,
            status,
            types.get(unit.get("ReactorTypeId")),
            _text(unit.get("ReactorModel")),
            _text(unit.get("OperationalFrom")),
            unit.get("IAEAId"),
        ))
    if len(rows) < 300:
        raise RuntimeError(f"only {len(rows)} reactor units parsed; the source format may have changed")
    conn = connect_static_db()
    with conn:
        conn.executescript(
            """DROP TABLE IF EXISTS nuclear_reactors_rtree;
            DROP TABLE IF EXISTS nuclear_reactors;
            CREATE TABLE nuclear_reactors (
                id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, country TEXT NOT NULL, lat REAL NOT NULL, lon REAL NOT NULL,
                capacity_mw REAL, status TEXT NOT NULL, reactor_type TEXT, model TEXT, operational_from TEXT, iaea_id INTEGER
            );"""
        )
        conn.executemany(
            "INSERT INTO nuclear_reactors (name, country, lat, lon, capacity_mw, status, reactor_type, model, operational_from, iaea_id) VALUES (?,?,?,?,?,?,?,?,?,?)",
            rows,
        )
    conn.close()
    operating = sum(1 for r in rows if r[5] == "Operational")
    log(f"  OK {len(rows):,} reactor units ({operating:,} operational)")
    return len(rows)


DATASETS = {"airports": ingest_airports, "seaports": ingest_seaports, "reactors": ingest_reactors}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("datasets", nargs="*", metavar="dataset", help=f"one of {', '.join(DATASETS)} (default: all)")
    names = parser.parse_args().datasets or list(DATASETS)
    unknown = [n for n in names if n not in DATASETS]
    if unknown:
        parser.error(f"unknown dataset(s): {', '.join(unknown)}")
    failed = []
    for name in names:
        try:
            DATASETS[name]()
        except Exception as error:  # keep going so one outage does not block the rest
            log(f"  FAILED {name}: {error}")
            failed.append(name)
    if failed:
        log(f"Finished with errors: {', '.join(failed)}")
        return 1
    log("All datasets imported.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
