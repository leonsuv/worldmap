#!/usr/bin/env python3
"""HTTP smoke test against a running backend (default http://127.0.0.1:3000).

    python scripts/smoke_test.py [--url http://host:port] [--write]

Read-only by default. `--write` also creates and deletes a watchlist item and
an event to exercise the alert pipeline. Checks that depend on third-party
services are reported as SKIP when the provider is unreachable.
"""

from __future__ import annotations

import argparse
import gzip
import json
import sys
import urllib.error
import urllib.request

results: list[tuple[str, str, str]] = []


class Skip(Exception):
    pass


def request(base: str, path: str, method: str = "GET", body: dict | None = None, headers: dict | None = None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(base + path, data=data, method=method, headers={"Content-Type": "application/json", **(headers or {})})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            raw = r.read()
            if r.headers.get("Content-Encoding") == "gzip":
                raw = gzip.decompress(raw)
            return r.status, r.headers, raw
    except urllib.error.HTTPError as e:
        return e.code, e.headers, e.read()


def check(name: str, fn) -> None:
    try:
        detail = fn() or ""
        results.append(("PASS", name, str(detail)))
    except Skip as s:
        results.append(("SKIP", name, str(s)))
    except Exception as e:  # noqa: BLE001 - report every failure
        results.append(("FAIL", name, f"{type(e).__name__}: {e}"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--url", default="http://127.0.0.1:3000")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    base = args.url.rstrip("/")

    def get_json(path, expect=200, **kw):
        status, headers, raw = request(base, path, **kw)
        assert status == expect, f"{path}: HTTP {status} {raw[:200]!r}"
        return json.loads(raw) if raw else None, headers

    def expect_status(path, *codes):
        def run():
            status_code = request(base, path)[0]
            assert status_code in codes, f"{path}: HTTP {status_code}, expected {codes}"
        return run

    status_holder = {}

    def status():
        data, _ = get_json("/api/status")
        for key in ("tiles", "ships_configured", "traffic_configured", "datasets", "ais"):
            assert key in data, key
        status_holder.update(data)
        return f"datasets {data['datasets']}, tiles {data['tiles']}"

    check("status", status)

    def dataset(path, minimum):
        def run():
            data, headers = get_json(path, headers={"Accept-Encoding": "gzip"})
            assert data["type"] == "FeatureCollection"
            assert len(data["features"]) >= minimum, f"{len(data['features'])} features"
            if minimum:
                assert headers.get("Content-Encoding") == "gzip", "pre-compressed response expected"
            return f"{len(data['features'])} features"
        return run

    ds = status_holder.get("datasets", {})
    check("airports", dataset("/api/airports", 1000 if ds.get("airports") else 0))
    check("seaports", dataset("/api/seaports", 1000 if ds.get("seaports") else 0))
    check("nuclear plants", dataset("/api/reactors", 100 if ds.get("reactors") else 0))
    check("aids to navigation", dataset("/api/ships/aton", 0))

    def snapshot():
        data, _ = get_json("/api/ships/snapshot")
        assert data["type"] == "snapshot" and isinstance(data["rows"], list) and len(data["fields"]) == 11
        return f"{len(data['rows'])} vessels"

    check("vessel snapshot", snapshot)
    check("unknown vessel 404", lambda: get_json("/api/ships/1", expect=404) and None)

    def local_search():
        if not ds.get("airports"):
            raise Skip("no airports imported")
        data, _ = get_json("/api/search?q=FRA&places=false")
        assert any(r["kind"] == "airport" and "Frankfurt" in r["name"] for r in data["results"]), data["results"][:3]
        return data["results"][0]["name"]

    check("search (local)", local_search)
    check("search too short 400", lambda: get_json("/api/search?q=x", expect=400) and None)

    def weather():
        status_code, _, raw = request(base, "/api/weather/grid?bbox=5,47,15,55")
        if status_code in (502, 503):
            raise Skip(json.loads(raw).get("error", "provider unavailable"))
        assert status_code == 200, status_code
        data = json.loads(raw)
        assert 0 < len(data["points"]) <= 60
        return f"{len(data['points'])} points, step {data['step']} deg"

    check("weather grid", weather)
    check("weather bad bbox 400", lambda: get_json("/api/weather/grid?bbox=1,2,3", expect=400) and None)
    check("weather bad lat 400", lambda: get_json("/api/weather?lat=91&lon=0", expect=400) and None)

    def flights():
        status_code, _, raw = request(base, "/api/flights")
        if status_code in (502, 503):
            raise Skip(json.loads(raw).get("error", "provider unavailable"))
        data = json.loads(raw)
        assert status_code == 200 and data["fields"][0] == "icao24"
        return f"{len(data['rows'])} aircraft{' (cached)' if data['stale'] else ''}"

    check("flights", flights)
    check("flight track bad id 400", lambda: get_json("/api/flights/track?icao24=zz", expect=400) and None)

    def traffic():
        status_code, headers, raw = request(base, "/api/traffic/5/16/10")
        if not status_holder.get("traffic_configured"):
            assert status_code == 503, status_code
            raise Skip("TOMTOM_API_KEY not set (503 as expected)")
        assert status_code == 200 and headers.get("Content-Type") == "image/png" and raw[:4] == b"\x89PNG"
        return f"{len(raw)} bytes"

    check("traffic tile", traffic)

    def tiles():
        names = status_holder.get("tiles", [])
        if not names:
            raise Skip("no tiles built")
        out = []
        for name in names:
            tj, _ = get_json(f"/tiles/{name}/tilejson.json")
            assert tj["tiles"][0].startswith(f"/tiles/{name}/") and tj["maxzoom"] >= tj["minzoom"]
            status_code, _, _ = request(base, f"/tiles/{name}/0/0/0")
            assert status_code in (200, 204), status_code
            out.append(f"{name} z{tj['minzoom']}-{tj['maxzoom']}")
        return ", ".join(out)

    check("vector tiles", tiles)
    check("unknown tile source 404", expect_status("/tiles/missing/1/0/0", 404))
    if status_holder.get("tiles"):
        check("out-of-range tile 400", expect_status(f"/tiles/{status_holder['tiles'][0]}/1/5/5", 400))
    check("unknown API route 404 json", lambda: get_json("/api/nope", expect=404)[0]["error"] and None)

    for path in ("/api/watchlist", "/api/events", "/api/alerts", "/api/alerts/count", "/api/history/timestamps", "/api/export/report"):
        check(path, lambda p=path: get_json(p) and None)
    check("csv export", expect_status("/api/export/csv?type=events", 200))
    check("csv bad type 400", lambda: get_json("/api/export/csv?type=nope", expect=400) and None)

    def validation():
        get_json("/api/events", expect=400, method="POST", body={"name": "", "event_type": "storm", "lat": 0, "lon": 0})
        get_json("/api/events", expect=400, method="POST", body={"name": "x", "event_type": "party", "lat": 0, "lon": 0})
        get_json("/api/watchlist", expect=400, method="POST", body={"wtype": "vessel", "name": "x", "params": {"mmsi": 1}})

    check("input validation", validation)

    if args.write:
        def alert_pipeline():
            item, _ = get_json("/api/watchlist", expect=201, method="POST", body={"wtype": "port", "name": "Smoke test port", "params": {"lat": 10.5, "lon": 20.5}})
            event, _ = get_json("/api/events", expect=201, method="POST", body={"name": "Smoke test storm", "event_type": "storm", "lat": 10.5, "lon": 20.6, "radius_km": 50})
            try:
                alerts, _ = get_json("/api/alerts")
                assert any(a["event_id"] == event["id"] and a["watch_id"] == item["id"] for a in alerts), "alert raised"
                affected, _ = get_json(f"/api/events/affected?event_id={event['id']}")
                assert "total" in affected
            finally:
                request(base, f"/api/events/{event['id']}", method="DELETE")
                request(base, f"/api/watchlist/{item['id']}", method="DELETE")
            return "watchlist -> event -> alert"

        check("alert pipeline (write)", alert_pipeline)

    width = max(len(n) for _, n, _ in results)
    for state, name, detail in results:
        print(f"{state:4}  {name:<{width}}  {detail}")
    failed = sum(1 for s, _, _ in results if s == "FAIL")
    print(f"\n{sum(1 for s, _, _ in results if s == 'PASS')} passed, {sum(1 for s, _, _ in results if s == 'SKIP')} skipped, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
