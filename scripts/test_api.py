#!/usr/bin/env python3
"""Test all backend API endpoints."""
import json
import sys
import urllib.request

BASE = "http://localhost:3000"

def fetch(path):
    req = urllib.request.Request(f"{BASE}{path}")
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return resp.status, resp.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode() if e.fp else ""

def test(name, path, check_fn):
    status, body = fetch(path)
    try:
        check_fn(status, body)
        print(f"  PASS  {name}")
    except Exception as e:
        print(f"  FAIL  {name}: {e} (status={status}, body={body[:200]})")

def main():
    print("=== WorldMap Backend API Tests ===\n")

    # 1. Airports
    test("Airports", "/api/airports", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert_geojson(b, min_features=1000),
    ))

    # 2. Seaports
    test("Seaports", "/api/seaports", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert_geojson(b, min_features=1000),
    ))

    # 3. Reactors (with bbox)
    test("Reactors (bbox)", "/api/reactors?bbox=-180,-90,180,90", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert_geojson(b, min_features=10),
    ))

    # 4. Reactors (no bbox)
    test("Reactors (all)", "/api/reactors", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert_geojson(b, min_features=10),
    ))

    # 5. Flights
    test("Flights", "/api/flights?min_lat=45&max_lat=55&min_lon=5&max_lon=15", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert_geojson(b, min_features=0),  # OpenSky can be empty if rate-limited
    ))

    # 6. Weather
    test("Weather (Berlin)", "/api/weather?lat=52.5&lon=13.4", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert('"latitude"' in b, "no latitude key"),
    ))

    # 7. Ships snapshot (needs AISSTREAM key + ~15s warmup for data)
    test("Ships snapshot", "/api/ships/snapshot", lambda s, b: (
        _assert(s == 200, f"status {s}"),
        _assert_geojson(b, min_features=0),  # 0 ok if AIS stream hasn't connected yet
    ))

    # 8. Traffic (no TomTom key → 503)
    test("Traffic (no key)", "/api/traffic?bbox=13.0,52.0,14.0,53.0", lambda s, b: (
        _assert(s == 503, f"expected 503, got {s}"),
    ))

    # 9. Removed routes should 404
    test("Electricity (removed)", "/api/electricity?zone=DE", lambda s, b: (
        _assert(s == 404, f"expected 404, got {s}"),
    ))
    test("Conflict (removed)", "/api/conflict?bbox=-10,35,40,60&year=2024", lambda s, b: (
        _assert(s == 404, f"expected 404, got {s}"),
    ))

    # 11. Bad requests
    test("Flights (no params → 400)", "/api/flights", lambda s, b: (
        _assert(s == 400, f"expected 400, got {s}"),
    ))
    test("Reactors (bad bbox → 400)", "/api/reactors?bbox=bad", lambda s, b: (
        _assert(s == 400, f"expected 400, got {s}"),
    ))

    print("\n=== Done ===")


def _assert(condition, msg="assertion failed"):
    if not condition:
        raise AssertionError(msg)

def _assert_geojson(body, min_features=0):
    d = json.loads(body)
    _assert(d.get("type") == "FeatureCollection", f"not FeatureCollection: {d.get('type')}")
    n = len(d.get("features", []))
    _assert(n >= min_features, f"expected >= {min_features} features, got {n}")
    if n > 0:
        f = d["features"][0]
        _assert(f.get("type") == "Feature", "feature type wrong")
        _assert("coordinates" in f.get("geometry", {}), "no coordinates")
    print(f"    ({n} features)", end="")


if __name__ == "__main__":
    main()
