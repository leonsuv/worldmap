#!/usr/bin/env python3
"""Ingest airport data from OurAirports CSV into static.db.

Source: https://davidmegginson.github.io/ourairports-data/airports.csv
Filters: large_airport and medium_airport only.

Requires: httpx (pip install httpx)
"""

import csv
import io
import os
import sqlite3
import sys
from pathlib import Path

try:
    import httpx
except ImportError:
    sys.exit("httpx is required: pip install httpx")

DATA_DIR = os.environ.get("DATA_DIR", str(Path(__file__).resolve().parent.parent / "data"))
DB_PATH = os.path.join(DATA_DIR, "static.db")

CSV_URL = "https://davidmegginson.github.io/ourairports-data/airports.csv"

SCHEMA = """
CREATE TABLE IF NOT EXISTS airports (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    icao         TEXT,
    name         TEXT NOT NULL,
    city         TEXT,
    country      TEXT,
    lat          REAL NOT NULL,
    lon          REAL NOT NULL,
    elevation_ft REAL
);
"""


def main():
    os.makedirs(DATA_DIR, exist_ok=True)

    print(f"Downloading airports CSV from OurAirports...")
    resp = httpx.get(CSV_URL, timeout=60, follow_redirects=True)
    resp.raise_for_status()

    reader = csv.DictReader(io.StringIO(resp.text))

    conn = sqlite3.connect(DB_PATH)
    conn.executescript(SCHEMA)

    # Clear for idempotent runs
    conn.execute("DELETE FROM airports")

    count = 0
    for row in reader:
        airport_type = row.get("type", "")
        if airport_type not in ("large_airport", "medium_airport"):
            continue

        try:
            lat = float(row["latitude_deg"])
            lon = float(row["longitude_deg"])
        except (ValueError, KeyError):
            continue

        icao = row.get("gps_code") or row.get("ident") or None
        name = row.get("name", "Unknown")
        city = row.get("municipality") or None
        country = row.get("iso_country") or None

        try:
            elevation = float(row["elevation_ft"]) if row.get("elevation_ft") else None
        except ValueError:
            elevation = None

        conn.execute(
            "INSERT INTO airports (icao, name, city, country, lat, lon, elevation_ft) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (icao, name, city, country, lat, lon, elevation),
        )
        count += 1

    conn.commit()
    conn.close()
    print(f"✓ {count} airports inserted into {DB_PATH}")


if __name__ == "__main__":
    main()
