#!/usr/bin/env python3
"""Ingest seaport data into static.db.

Uses OpenStreetMap Overpass API to find harbour nodes globally.

Requires: httpx (pip install httpx)
"""

import json
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

OVERPASS_URL = "https://overpass-api.de/api/interpreter"
OVERPASS_QUERY = """
[out:json][timeout:120];
(
  node["harbour"="yes"]["name"];
  node["leisure"="marina"]["name"];
  node["industrial"="port"]["name"];
);
out body;
"""

SCHEMA = """
CREATE TABLE IF NOT EXISTS seaports (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    locode  TEXT,
    name    TEXT NOT NULL,
    country TEXT,
    lat     REAL NOT NULL,
    lon     REAL NOT NULL
);
"""


def main():
    os.makedirs(DATA_DIR, exist_ok=True)

    print("Querying Overpass API for harbour nodes (this may take a minute)...")
    resp = httpx.post(
        OVERPASS_URL,
        data={"data": OVERPASS_QUERY},
        timeout=180,
    )
    resp.raise_for_status()
    data = resp.json()

    elements = data.get("elements", [])
    if not elements:
        print("No harbour nodes returned from Overpass.", file=sys.stderr)
        sys.exit(1)

    conn = sqlite3.connect(DB_PATH)
    conn.executescript(SCHEMA)

    # Clear for idempotent runs
    conn.execute("DELETE FROM seaports")

    count = 0
    seen_names = set()
    for el in elements:
        if el.get("type") != "node":
            continue
        tags = el.get("tags", {})
        name = tags.get("name", "").strip()
        if not name:
            continue

        lat = el.get("lat")
        lon = el.get("lon")
        if lat is None or lon is None:
            continue

        # Deduplicate by name + approximate location
        dedup_key = f"{name}:{lat:.1f}:{lon:.1f}"
        if dedup_key in seen_names:
            continue
        seen_names.add(dedup_key)

        locode = tags.get("ref:locode") or tags.get("ref") or None
        country = tags.get("addr:country") or tags.get("is_in:country") or None

        conn.execute(
            "INSERT INTO seaports (locode, name, country, lat, lon) VALUES (?, ?, ?, ?, ?)",
            (locode, name, country, lat, lon),
        )
        count += 1

    conn.commit()
    conn.close()
    print(f"✓ {count} seaports inserted into {DB_PATH}")


if __name__ == "__main__":
    main()
