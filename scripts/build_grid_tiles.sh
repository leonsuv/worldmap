#!/usr/bin/env bash
set -euo pipefail

# Build power-grid and high-voltage line MBTiles from free public datasets.
# Requires: curl, ogr2ogr (GDAL), tippecanoe, python3

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="${DATA_DIR:-$SCRIPT_DIR/../data}"
TILES_DIR="$DATA_DIR/tiles"
mkdir -p "$TILES_DIR"

# ── Dependency checks ────────────────────────────────────────────────────────
for cmd in curl ogr2ogr tippecanoe python3; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "ERROR: $cmd is not installed."
        case "$cmd" in
            ogr2ogr) echo "  brew install gdal" ;;
            tippecanoe) echo "  brew install tippecanoe" ;;
            *) ;;
        esac
        exit 1
    fi
done

# ══════════════════════════════════════════════════════════════════════════════
# Step 1: Download Gridfinder global predicted grid (World Bank / ESMAP)
# ══════════════════════════════════════════════════════════════════════════════
GRID_GPKG="$DATA_DIR/grid.gpkg"
if [ ! -f "$GRID_GPKG" ]; then
    echo "=== Downloading Gridfinder dataset (~200 MB) ==="
    curl -L "https://zenodo.org/record/3628142/files/grid.gpkg" -o "$GRID_GPKG"
else
    echo "Already downloaded: $GRID_GPKG"
fi

# ══════════════════════════════════════════════════════════════════════════════
# Step 2: Convert Gridfinder GeoPackage → GeoJSON → MBTiles
# ══════════════════════════════════════════════════════════════════════════════
GRID_MBTILES="$TILES_DIR/power-grid.mbtiles"
if [ ! -f "$GRID_MBTILES" ]; then
    echo "=== Converting Gridfinder to GeoJSON ==="
    TMP_GRID="/tmp/grid_lines.geojson"
    ogr2ogr \
        -f GeoJSON "$TMP_GRID" \
        "$GRID_GPKG" \
        -t_srs EPSG:4326

    echo "=== Building power-grid.mbtiles (zoom 2–12) ==="
    tippecanoe \
        -o "$GRID_MBTILES" \
        --force \
        --minimum-zoom=2 \
        --maximum-zoom=12 \
        --simplification=4 \
        --drop-densest-as-needed \
        --layer=grid \
        "$TMP_GRID"

    rm -f "$TMP_GRID"
    echo "  ✓ Created $GRID_MBTILES"
else
    echo "Already built: $GRID_MBTILES"
fi

# ══════════════════════════════════════════════════════════════════════════════
# Step 3: Fetch OSM high-voltage lines via Overpass API
# ══════════════════════════════════════════════════════════════════════════════
HV_MBTILES="$TILES_DIR/hv-lines.mbtiles"
if [ ! -f "$HV_MBTILES" ]; then
    echo "=== Fetching OSM high-voltage transmission lines via Overpass ==="
    TMP_HV="/tmp/osm_hv_lines.geojson"

    python3 - <<'PYEOF'
import urllib.request, json, time, sys

OVERPASS = "https://overpass-api.de/api/interpreter"

# Split the world into regional bounding boxes to avoid Overpass timeouts
# Only fetch >= 220kV lines (major transmission backbone) to keep data manageable
REGIONS = [
    ("Europe NW",       "48,-12,72,10"),
    ("Europe NE",       "48,10,72,40"),
    ("Europe SW",       "35,-12,48,10"),
    ("Europe SE",       "35,10,48,40"),
    ("Turkey+Ukraine",  "35,25,55,45"),
    ("Russia West",     "50,40,72,80"),
    ("Russia East",     "45,80,72,180"),
    ("Middle East",     "12,25,42,65"),
    ("South Asia",      "5,65,38,100"),
    ("China",           "18,73,55,135"),
    ("Japan/Korea",     "25,125,50,150"),
    ("SE Asia",         "-12,90,25,145"),
    ("North Africa",    "15,-20,38,40"),
    ("Sub-Saharan Af",  "-38,-20,15,55"),
    ("USA East",        "24,-105,55,-65"),
    ("USA West+Can",    "30,-170,72,-105"),
    ("Mexico+Central",  "5,-120,30,-60"),
    ("South America N", "-20,-82,12,-34"),
    ("South America S", "-56,-82,-20,-34"),
    ("Oceania",         "-48,110,0,180"),
]

def fetch_region(name, bbox):
    s, w, e, n = bbox.split(",")
    query = f"""
[out:json][timeout:180][bbox:{bbox}];
(
  way["power"="line"]["voltage"~"^(220|275|330|380|400|500|735|765|1000)[0-9]*$"];
);
out geom;
"""
    print(f"  Fetching {name} ({bbox})...", end=" ", flush=True)
    for attempt in range(3):
        try:
            req = urllib.request.Request(
                OVERPASS,
                data=("data=" + query).encode(),
                headers={"Content-Type": "application/x-www-form-urlencoded"}
            )
            with urllib.request.urlopen(req, timeout=240) as r:
                data = json.loads(r.read())
            elements = data.get("elements", [])
            print(f"{len(elements)} elements")
            return elements
        except Exception as exc:
            print(f"attempt {attempt+1} failed ({exc})")
            if attempt < 2:
                time.sleep(30)
    print(f"  SKIP {name} after 3 attempts")
    return []

print("Fetching OSM high-voltage transmission lines by region...")
all_features = []
seen_ids = set()

for name, bbox in REGIONS:
    elements = fetch_region(name, bbox)
    for el in elements:
        if el["type"] != "way" or "geometry" not in el:
            continue
        eid = el.get("id")
        if eid in seen_ids:
            continue
        seen_ids.add(eid)
        coords = [[n["lon"], n["lat"]] for n in el["geometry"]]
        if len(coords) < 2:
            continue
        all_features.append({
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": coords},
            "properties": {
                "voltage": el.get("tags", {}).get("voltage", ""),
                "name": el.get("tags", {}).get("name", ""),
                "operator": el.get("tags", {}).get("operator", ""),
            }
        })
    time.sleep(10)  # polite delay between regions

geojson = {"type": "FeatureCollection", "features": all_features}
path = "/tmp/osm_hv_lines.geojson"
with open(path, "w") as f:
    json.dump(geojson, f)
print(f"OSM: {len(all_features)} unique high-voltage line segments written to {path}")
PYEOF

    echo "=== Building hv-lines.mbtiles (zoom 3–14) ==="
    tippecanoe \
        -o "$HV_MBTILES" \
        --force \
        --minimum-zoom=3 \
        --maximum-zoom=14 \
        --simplification=2 \
        --layer=hvlines \
        "$TMP_HV"

    rm -f "$TMP_HV"
    echo "  ✓ Created $HV_MBTILES"
else
    echo "Already built: $HV_MBTILES"
fi

echo ""
echo "Done! Files:"
ls -lh "$GRID_MBTILES" "$HV_MBTILES" 2>/dev/null || true
echo ""
echo "Your tile server will serve these at:"
echo "  /tiles/power-grid/{z}/{x}/{y}"
echo "  /tiles/hv-lines/{z}/{x}/{y}"
