#!/usr/bin/env python3
"""Ingest nuclear reactor data from GeoNuclearData into static.db.

Source: https://github.com/cristianst85/GeoNuclearData
  - 800+ reactors worldwide, sourced from IAEA PRIS + WNA
  - Free, no API key needed
"""

import json
import os
import sqlite3
import urllib.request
from pathlib import Path

REACTORS_URL = "https://raw.githubusercontent.com/cristianst85/GeoNuclearData/master/data/json/raw/4-nuclear_power_plants.json"
COUNTRIES_URL = "https://raw.githubusercontent.com/cristianst85/GeoNuclearData/master/data/json/raw/1-countries.json"

DATA_DIR = os.environ.get("DATA_DIR", str(Path(__file__).resolve().parent.parent / "data"))
DB_PATH = os.path.join(DATA_DIR, "static.db")

SCHEMA = """
CREATE TABLE IF NOT EXISTS nuclear_reactors (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    country      TEXT NOT NULL,
    lat          REAL NOT NULL,
    lon          REAL NOT NULL,
    capacity_mw  REAL,
    status       TEXT,
    reactor_type TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS nuclear_reactors_rtree USING rtree(
    id,
    min_lat, max_lat,
    min_lon, max_lon
);
"""


def main():
    os.makedirs(DATA_DIR, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.executescript(SCHEMA)

    # Download country code -> name mapping
    print("Downloading country data...")
    with urllib.request.urlopen(COUNTRIES_URL) as r:
        countries = {c["Code"]: c["Name"] for c in json.loads(r.read())}

    # Download reactor data
    print("Downloading reactor data...")
    with urllib.request.urlopen(REACTORS_URL) as r:
        reactors = json.loads(r.read())

    # Clear existing data for idempotent runs
    conn.execute("DELETE FROM nuclear_reactors")
    conn.execute("DELETE FROM nuclear_reactors_rtree")

    inserted = 0
    for npp in reactors:
        # StatusId: 3 = Operational, 4 = Suspended Operation
        if npp.get("StatusId") not in (3, 4):
            continue
        lat = npp.get("Latitude")
        lon = npp.get("Longitude")
        if lat is None or lon is None:
            continue

        country = countries.get(npp.get("CountryCode", ""), "Unknown")
        capacity = npp.get("Capacity", 0) or 0
        reactor_model = npp.get("ReactorModel") or ""
        status = "Suspended" if npp.get("StatusId") == 4 else "Operational"

        cur = conn.execute(
            """INSERT INTO nuclear_reactors (name, country, lat, lon, capacity_mw, status, reactor_type)
               VALUES (?, ?, ?, ?, ?, ?, ?)""",
            (npp["Name"], country, float(lat), float(lon), capacity, status, reactor_model),
        )
        rid = cur.lastrowid
        conn.execute(
            "INSERT INTO nuclear_reactors_rtree (id, min_lat, max_lat, min_lon, max_lon) VALUES (?, ?, ?, ?, ?)",
            (rid, float(lat), float(lat), float(lon), float(lon)),
        )
        inserted += 1

    conn.commit()
    conn.close()
    print(f"✓ {inserted} operational reactors ingested into {DB_PATH}")


if __name__ == "__main__":
    main()
