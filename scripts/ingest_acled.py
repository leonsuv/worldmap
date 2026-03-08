#!/usr/bin/env python3
"""Pre-load ACLED conflict event data into static.db (optional).

Fetches the last 12 months of events globally. Uses ACLED OAuth2 auth.
Requires ACLED_EMAIL and ACLED_PASSWORD environment variables.

Requires: httpx (pip install httpx)
"""

import os
import sqlite3
import sys
import time
from datetime import datetime, timedelta
from pathlib import Path

try:
    import httpx
except ImportError:
    sys.exit("httpx is required: pip install httpx")

DATA_DIR = os.environ.get("DATA_DIR", str(Path(__file__).resolve().parent.parent / "data"))
DB_PATH = os.path.join(DATA_DIR, "static.db")

ACLED_TOKEN_URL = "https://acleddata.com/oauth/token"
ACLED_API_URL = "https://acleddata.com/api/acled/read"

SCHEMA = """
CREATE TABLE IF NOT EXISTS conflict_events (
    id          INTEGER PRIMARY KEY,
    date        TEXT NOT NULL,
    country     TEXT,
    lat         REAL NOT NULL,
    lon         REAL NOT NULL,
    event_type  TEXT,
    fatalities  INTEGER,
    notes       TEXT
);

CREATE INDEX IF NOT EXISTS idx_conflict_latlon ON conflict_events(lat, lon);
"""


def get_acled_token(email: str, password: str) -> str:
    """Obtain a Bearer token via ACLED OAuth2 endpoint (valid 24h)."""
    resp = httpx.post(
        ACLED_TOKEN_URL,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
        data={
            "username": email,
            "password": password,
            "grant_type": "password",
            "client_id": "acled",
        },
        timeout=30,
    )
    resp.raise_for_status()
    return resp.json()["access_token"]


def fetch_acled_page(token: str, page: int, limit: int, date_from: str) -> list:
    """Fetch one page of ACLED events using Bearer auth."""
    resp = httpx.get(
        ACLED_API_URL,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
        params={
            "_format": "json",
            "limit": str(limit),
            "page": str(page),
            "event_date": date_from,
            "event_date_where": ">=",
        },
        timeout=60,
    )
    resp.raise_for_status()
    return resp.json().get("data", [])


def main():
    email = os.environ.get("ACLED_EMAIL")
    password = os.environ.get("ACLED_PASSWORD")
    if not email or not password:
        sys.exit("Set ACLED_EMAIL and ACLED_PASSWORD environment variables.")

    os.makedirs(DATA_DIR, exist_ok=True)

    print("Authenticating with ACLED...")
    token = get_acled_token(email, password)
    print("  ✓ Token obtained")

    conn = sqlite3.connect(DB_PATH)
    conn.executescript(SCHEMA)
    conn.execute("DELETE FROM conflict_events")

    # Fetch last 12 months
    date_from = (datetime.utcnow() - timedelta(days=365)).strftime("%Y-%m-%d")
    page = 1
    total = 0
    limit = 5000

    print(f"Fetching ACLED events since {date_from}...")

    while True:
        events = fetch_acled_page(token, page, limit, date_from)
        if not events:
            break

        for ev in events:
            try:
                lat = float(ev.get("latitude", 0))
                lon = float(ev.get("longitude", 0))
            except (ValueError, TypeError):
                continue

            event_id = ev.get("event_id_cnty") or ev.get("data_id")
            if event_id is None:
                continue

            try:
                fatalities = int(ev.get("fatalities", 0) or 0)
            except (ValueError, TypeError):
                fatalities = 0

            conn.execute(
                """INSERT OR IGNORE INTO conflict_events
                   (id, date, country, lat, lon, event_type, fatalities, notes)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    hash(str(event_id)) & 0x7FFFFFFFFFFFFFFF,  # stable integer id
                    ev.get("event_date", ""),
                    ev.get("country", ""),
                    lat,
                    lon,
                    ev.get("event_type", ""),
                    fatalities,
                    (ev.get("notes", "") or "")[:500],
                ),
            )

        total += len(events)
        print(f"  Page {page}: {len(events)} events (total: {total})")

        if len(events) < limit:
            break
        page += 1
        time.sleep(0.5)  # rate-limit politeness

    conn.commit()
    count = conn.execute("SELECT COUNT(*) FROM conflict_events").fetchone()[0]
    conn.close()
    print(f"✓ {count} conflict events inserted into {DB_PATH}")


if __name__ == "__main__":
    main()
