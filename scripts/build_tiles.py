#!/usr/bin/env python3
"""Build the optional network tile layers into data/tiles/.

    python scripts/build_tiles.py hv-lines              # OSM transmission lines, worldwide
    python scripts/build_tiles.py hv-lines --bbox 5,47,16,55   # a region (west,south,east,north)
    python scripts/build_tiles.py pipelines             # OSM oil & gas pipelines
    python scripts/build_tiles.py pipelines --ogim OGIM_v2.7.gpkg
    python scripts/build_tiles.py power-grid            # Gridfinder estimated grid (downloads ~200 MB)
    python scripts/build_tiles.py all

Only Python 3.10+ and Rust are required: the tiles are built by
`worldmap-backend tiles build`, which reads GeoJSON and GeoPackage natively
(no GDAL or tippecanoe). Overpass downloads are cached per area under
data/sources/cache/, so an interrupted run continues where it stopped.
A running WorldMap server shows new tiles within 30 seconds.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

from common import (
    SOURCES_DIR,
    TILES_DIR,
    backend_command,
    build_tiles,
    download,
    log,
    overpass_cells,
    parse_bbox,
    write_geojsonseq,
)

OSM_ATTRIBUTION = '<a href="https://www.openstreetmap.org/copyright">© OpenStreetMap contributors</a>'
GRIDFINDER_URL = "https://zenodo.org/records/3628142/files/grid.gpkg?download=1"

# power=line/cable with any listed voltage >= 110 kV ("380000;220000" style lists).
HV_REGEX = "(^|;)(1[1-9][0-9]{4}|[2-9][0-9]{5}|[1-9][0-9]{6,})(;|$)"


def _geometry(way: dict) -> list[list[float]] | None:
    coords = [[round(p["lon"], 6), round(p["lat"], 6)] for p in way.get("geometry") or [] if p]
    return coords if len(coords) >= 2 else None


def _number(value: str | None) -> float | None:
    if not value:
        return None
    match = re.search(r"\d+(?:\.\d+)?", value)
    return float(match.group()) if match else None


def voltages_kv(value: str | None) -> list[int]:
    out = []
    for part in (value or "").split(";"):
        part = part.strip()
        if part.isdigit() and int(part) >= 1000:
            out.append(round(int(part) / 1000))
    return out


def hv_feature(way: dict) -> dict | None:
    coords = _geometry(way)
    tags = way.get("tags", {})
    kv = voltages_kv(tags.get("voltage"))
    if not coords or not kv:
        return None
    top = max(kv)
    frequency = tags.get("frequency", "")
    props = {
        "voltage_kv": top,
        "voltage": tags.get("voltage"),
        "kind": "cable" if tags.get("power") == "cable" else "line",
        "hvdc": frequency.strip() == "0" or "hvdc" in tags.get("line", "").lower(),
        "circuits": int(_number(tags.get("circuits")) or 0) or None,
        "name": tags.get("name"),
        "operator": tags.get("operator"),
        "ref": tags.get("ref"),
        "location": tags.get("location"),
        # Coarser zooms only show the backbone.
        "_minzoom": 2 if top >= 500 else 3 if top >= 300 else 5 if top >= 200 else 7,
    }
    return {"type": "Feature", "geometry": {"type": "LineString", "coordinates": coords}, "properties": {k: v for k, v in props.items() if v not in (None, "")}}


def build_hv_lines(args: argparse.Namespace) -> None:
    def query(cell):
        s, w, n, e = cell
        box = f"({s},{w},{n},{e})"
        return f"""[out:json][timeout:300];
(
  way["power"="line"]["voltage"~"{HV_REGEX}"]{box};
  way["power"="cable"]["voltage"~"{HV_REGEX}"]{box};
);
out tags geom;"""

    seen: set[int] = set()

    def features():
        for el in overpass_cells("hv-lines", query, bbox=args.bbox, step=args.cell):
            if el.get("type") != "way" or el["id"] in seen:
                continue
            seen.add(el["id"])
            if f := hv_feature(el):
                yield f

    source = SOURCES_DIR / "hv-lines.geojsonl"
    count = write_geojsonseq(source, features())
    log(f"High-voltage lines: {count:,} ways written to {source}")
    build_tiles([
        "-o", str(TILES_DIR / "hv-lines.mbtiles"), "-i", str(source), "-l", "hvlines",
        "-Z", "2", "-z", str(args.maxzoom or 13),
        "--name", "High-voltage lines", "--description", "Transmission lines of 110 kV and above (OpenStreetMap)",
        "--attribution", OSM_ATTRIBUTION,
    ])


def commodity(substance: str) -> str:
    s = substance.lower()
    if "hydrogen" in s:
        return "HYDROGEN"
    gas = any(k in s for k in ("gas", "lng", "lpg", "cng", "methane"))
    oil = any(k in s for k in ("oil", "crude", "petroleum", "fuel", "diesel", "kerosene", "condensate", "gasoline", "naphtha"))
    if gas and oil and "gasoline" not in s:
        return "OIL AND GAS"
    if oil:
        return "OIL"
    if gas:
        return "GAS"
    return "OTHER"


def pipeline_feature(way: dict) -> dict | None:
    coords = _geometry(way)
    if not coords:
        return None
    tags = way.get("tags", {})
    substance = tags.get("substance") or tags.get("type") or ""
    usage = tags.get("usage")
    diameter = _number(tags.get("diameter"))
    props = {
        "commodity": commodity(substance),
        "substance": substance or None,
        "name": tags.get("name"),
        "operator": tags.get("operator"),
        "diameter_mm": round(diameter) if diameter else None,
        "location": tags.get("location"),
        "usage": usage,
        "_minzoom": 6 if usage in ("gathering", "flowline") else 2,
    }
    return {"type": "Feature", "geometry": {"type": "LineString", "coordinates": coords}, "properties": {k: v for k, v in props.items() if v not in (None, "")}}


def build_pipelines(args: argparse.Namespace) -> None:
    target = TILES_DIR / "pipelines.mbtiles"
    common_args = ["-o", str(target), "-l", "pipelines", "-Z", "2", "-z", str(args.maxzoom or 12), "--name", "Oil and gas pipelines"]
    if args.ogim:
        gpkg = Path(args.ogim)
        if not gpkg.exists():
            sys.exit(f"{gpkg} not found. Download OGIM from https://zenodo.org/records/15103476")
        tables = subprocess.run(backend_command() + ["tiles", "tables", str(gpkg)], check=True, capture_output=True, text=True).stdout
        names = [line.split("\t")[0] for line in tables.splitlines() if "pipe" in line.lower()]
        if not names:
            sys.exit(f"No pipeline table in {gpkg}:\n{tables}")
        log(f"Using OGIM table {names[0]}")
        build_tiles(common_args + [
            "-i", str(gpkg), "--gpkg-table", names[0],
            "--include", "COMMODITY,OPERATOR,FAC_NAME,FAC_STATUS,PIPE_DIAMETER_MM",
            "--where", "UPPER(COALESCE(FAC_STATUS, '')) NOT LIKE '%ABANDON%' AND UPPER(COALESCE(FAC_STATUS, '')) NOT LIKE '%PROPOS%' "
                       "AND UPPER(COALESCE(FAC_STATUS, '')) NOT LIKE '%CANCEL%' AND UPPER(COALESCE(FAC_STATUS, '')) NOT LIKE '%DECOMMISSION%'",
            "--description", "Oil and gas pipelines (OGIM, Environmental Defense Fund / MethaneSAT)",
            "--attribution", "OGIM © Environmental Defense Fund (CC BY 4.0)",
        ])
        return

    def query(cell):
        s, w, n, e = cell
        box = f"({s},{w},{n},{e})"
        return f"""[out:json][timeout:300];
(
  way["man_made"="pipeline"]["substance"~"gas|oil|fuel|petroleum|lng|lpg|cng|hydrogen|condensate|kerosene|diesel|naphtha",i]["usage"!~"^(distribution|branch)$"]{box};
  way["man_made"="pipeline"]["type"~"^(gas|oil|fuel|petroleum)$"]["usage"!~"^(distribution|branch)$"]{box};
);
out tags geom;"""

    seen: set[int] = set()

    def features():
        for el in overpass_cells("pipelines", query, bbox=args.bbox, step=args.cell):
            if el.get("type") != "way" or el["id"] in seen:
                continue
            seen.add(el["id"])
            if f := pipeline_feature(el):
                yield f

    source = SOURCES_DIR / "pipelines.geojsonl"
    count = write_geojsonseq(source, features())
    log(f"Pipelines: {count:,} ways written to {source}")
    build_tiles(common_args + [
        "-i", str(source), "--description", "Oil, gas and hydrogen pipelines (OpenStreetMap)", "--attribution", OSM_ATTRIBUTION,
    ])


def build_power_grid(args: argparse.Namespace) -> None:
    gpkg = Path(args.gpkg) if args.gpkg else SOURCES_DIR / "gridfinder-grid.gpkg"
    if not gpkg.exists():
        if args.gpkg:
            sys.exit(f"{gpkg} not found")
        download(GRIDFINDER_URL, gpkg, label="Gridfinder grid.gpkg (~200 MB) from Zenodo")
    build_tiles([
        "-o", str(TILES_DIR / "power-grid.mbtiles"), "-i", str(gpkg), "-l", "grid",
        "-Z", "2", "-z", str(args.maxzoom or 11), "--simplify", "1.5",
        "--name", "Estimated power grid", "--description", "Gridfinder predicted medium- and high-voltage network",
        "--attribution", "Gridfinder, Arderne et al. 2020 (CC BY 4.0)",
    ])


BUILDERS = {"hv-lines": build_hv_lines, "pipelines": build_pipelines, "power-grid": build_power_grid}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("layer", choices=[*BUILDERS, "all"])
    parser.add_argument("--bbox", help="limit Overpass downloads to west,south,east,north")
    parser.add_argument("--maxzoom", type=int, help="highest zoom level to build")
    parser.add_argument("--cell", type=float, default=20.0, help="initial Overpass cell size in degrees (split automatically)")
    parser.add_argument("--ogim", help="pipelines: use an OGIM GeoPackage instead of OpenStreetMap")
    parser.add_argument("--gpkg", help="power-grid: use a local Gridfinder grid.gpkg")
    args = parser.parse_args()
    try:
        args.bbox = parse_bbox(args.bbox)
    except ValueError as error:
        parser.error(str(error))
    layers = list(BUILDERS) if args.layer == "all" else [args.layer]
    failed = []
    for layer in layers:
        log(f"=== {layer} ===")
        try:
            BUILDERS[layer](args)
        except (subprocess.CalledProcessError, RuntimeError, OSError) as error:
            log(f"FAILED {layer}: {error}")
            failed.append(layer)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
