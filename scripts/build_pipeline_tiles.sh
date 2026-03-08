#!/usr/bin/env bash
set -euo pipefail

# Build pipeline vector tiles from OGIM v2.7 (EDF/MethaneSAT) + OSM supplement.
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
# Step 1: Download OGIM v2.7 GeoPackage (~2 GB)
# ══════════════════════════════════════════════════════════════════════════════
OGIM_GPKG="$DATA_DIR/ogim_v2.7.gpkg"
if [ ! -f "$OGIM_GPKG" ]; then
    echo "=== Downloading OGIM v2.7 GeoPackage (~3.1 GB) ==="
    curl -L "https://zenodo.org/records/15103476/files/OGIM_v2.7.gpkg?download=1" \
         -o "$OGIM_GPKG" \
         --progress-bar
else
    echo "Already downloaded: $OGIM_GPKG"
fi

# ══════════════════════════════════════════════════════════════════════════════
# Step 2: Discover pipeline layer name
# ══════════════════════════════════════════════════════════════════════════════
echo "=== Listing layers in OGIM GeoPackage ==="
ogrinfo -so "$OGIM_GPKG" 2>/dev/null | grep -i "^[0-9]*:" || true

# ══════════════════════════════════════════════════════════════════════════════
# Step 3: Extract pipeline geometries to GeoJSON
# ══════════════════════════════════════════════════════════════════════════════
PIPELINES_MBTILES="$TILES_DIR/pipelines.mbtiles"
if [ -f "$PIPELINES_MBTILES" ]; then
    echo "Already built: $PIPELINES_MBTILES"
    echo "Delete it to rebuild."
    ls -lh "$PIPELINES_MBTILES"
    exit 0
fi

TMP_OGIM="/tmp/ogim_pipelines.geojson"
echo "=== Extracting pipelines to GeoJSON ==="

# Try the known layer name; if it fails, discover it
LAYER_NAME="pipelines"
if ! ogrinfo -so "$OGIM_GPKG" "$LAYER_NAME" &>/dev/null; then
    echo "Layer '$LAYER_NAME' not found, discovering..."
    LAYER_NAME=$(ogrinfo -so "$OGIM_GPKG" 2>/dev/null \
        | grep -i "pipe" \
        | head -1 \
        | sed 's/^[0-9]*: //' \
        | sed 's/ (.*//')
    if [ -z "$LAYER_NAME" ]; then
        echo "Could not find a pipeline layer. Available layers:"
        ogrinfo -so "$OGIM_GPKG"
        exit 1
    fi
    echo "Using layer: $LAYER_NAME"
fi

echo "Extracting layer '$LAYER_NAME' (active/operating pipelines only)..."
ogr2ogr \
    -f GeoJSON "$TMP_OGIM" \
    "$OGIM_GPKG" \
    "$LAYER_NAME" \
    -select "COMMODITY,OPERATOR,COUNTRY,FAC_STATUS,PIPE_DIAMETER_MM" \
    -where "FAC_STATUS IN ('ACTIVE','OPERATING','IN SERVICE','NEW')" \
    -t_srs EPSG:4326 \
    2>/dev/null || \
ogr2ogr \
    -f GeoJSON "$TMP_OGIM" \
    "$OGIM_GPKG" \
    "$LAYER_NAME" \
    -t_srs EPSG:4326

echo "OGIM GeoJSON size:"
du -sh "$TMP_OGIM"

# ══════════════════════════════════════════════════════════════════════════════
# Step 4: Fetch OSM pipelines as supplement
# ══════════════════════════════════════════════════════════════════════════════
echo "=== Fetching OSM pipeline data via Overpass ==="
TMP_OSM="/tmp/osm_pipelines.geojson"

python3 - <<'PYEOF'
import urllib.request, json, time

OVERPASS = "https://overpass-api.de/api/interpreter"

# Fetch by region to avoid timeouts
REGIONS = [
    ("Europe",          "35,-12,72,40"),
    ("Russia",          "45,25,75,180"),
    ("Middle East",     "12,25,42,65"),
    ("South Asia",      "5,65,38,100"),
    ("East Asia",       "18,73,55,150"),
    ("SE Asia",         "-12,90,25,145"),
    ("Africa",          "-38,-20,38,55"),
    ("North America",   "15,-170,72,-52"),
    ("Central America", "5,-120,24,-60"),
    ("South America",   "-56,-82,12,-34"),
    ("Oceania",         "-48,110,0,180"),
]

def fetch_region(name, bbox):
    query = f"""
[out:json][timeout:180][bbox:{bbox}];
(
  way["man_made"="pipeline"]["substance"~"oil|gas|fuel|lng|cng|lpg",i];
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

print("Fetching OSM pipelines by region...")
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
        tags = el.get("tags", {})
        substance = tags.get("substance", "").lower()
        commodity = "OIL" if any(x in substance for x in ["oil", "fuel", "petroleum"]) \
                    else "GAS" if any(x in substance for x in ["gas", "lng", "cng", "lpg"]) \
                    else "OTHER"
        all_features.append({
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": coords},
            "properties": {
                "COMMODITY": commodity,
                "OPERATOR_NAME": tags.get("operator", ""),
                "SOURCE": "OSM"
            }
        })
    time.sleep(10)

out = {"type": "FeatureCollection", "features": all_features}
with open("/tmp/osm_pipelines.geojson", "w") as f:
    json.dump(out, f)
print(f"OSM: {len(all_features)} unique pipeline segments")
PYEOF

# ══════════════════════════════════════════════════════════════════════════════
# Step 5: Normalize OGIM properties + merge with OSM
# ══════════════════════════════════════════════════════════════════════════════
echo "=== Merging OGIM + OSM ==="
TMP_MERGED="/tmp/all_pipelines.geojson"

python3 - <<'PYEOF'
import json

with open("/tmp/ogim_pipelines.geojson") as f:
    ogim = json.load(f)
with open("/tmp/osm_pipelines.geojson") as f:
    osm = json.load(f)

# Normalize OGIM properties to consistent uppercase COMMODITY
for feat in ogim["features"]:
    p = feat.get("properties", {})
    c = (p.get("COMMODITY") or "").upper()
    if not c:
        c = "OTHER"
    p["COMMODITY"] = c

merged = {
    "type": "FeatureCollection",
    "features": ogim["features"] + osm["features"]
}
with open("/tmp/all_pipelines.geojson", "w") as f:
    json.dump(merged, f)

print(f"Merged: {len(merged['features'])} total pipeline segments")
print(f"  OGIM: {len(ogim['features'])}")
print(f"  OSM:  {len(osm['features'])}")
PYEOF

# ══════════════════════════════════════════════════════════════════════════════
# Step 6: Build MBTiles
# ══════════════════════════════════════════════════════════════════════════════
echo "=== Building pipelines.mbtiles (zoom 2–12) ==="
tippecanoe \
    -o "$PIPELINES_MBTILES" \
    --force \
    --minimum-zoom=2 \
    --maximum-zoom=12 \
    --simplification=3 \
    --drop-densest-as-needed \
    --layer=pipelines \
    --attribute-type=COMMODITY:string \
    "$TMP_MERGED"

rm -f "$TMP_OGIM" "$TMP_OSM" "$TMP_MERGED"
echo "  ✓ Created $PIPELINES_MBTILES"

echo ""
echo "Done! File:"
ls -lh "$PIPELINES_MBTILES"
echo ""
echo "Your tile server will serve this at:"
echo "  /tiles/pipelines/{z}/{x}/{y}"
