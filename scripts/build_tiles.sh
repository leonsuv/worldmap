#!/usr/bin/env bash
set -euo pipefail

# Build vector tile (.mbtiles) files from GeoPackage sources.
# Places output in data/tiles/.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="${DATA_DIR:-$SCRIPT_DIR/../data}"
TILES_DIR="$DATA_DIR/tiles"

mkdir -p "$TILES_DIR"

# --- Check for tippecanoe ---------------------------------------------------
if ! command -v tippecanoe &>/dev/null; then
    echo "ERROR: tippecanoe is not installed."
    echo ""
    echo "Install on macOS:  brew install tippecanoe"
    echo "Install on Ubuntu: sudo apt-get install -y tippecanoe"
    echo "Build from source: https://github.com/felt/tippecanoe"
    exit 1
fi

if ! command -v ogr2ogr &>/dev/null; then
    echo "ERROR: ogr2ogr (GDAL) is not installed."
    echo ""
    echo "Install on macOS:  brew install gdal"
    echo "Install on Ubuntu: sudo apt-get install -y gdal-bin"
    exit 1
fi

# --- Helper: convert GeoPackage to MBTiles via GeoJSON intermediate ----------
gpkg_to_mbtiles() {
    local input="$1"
    local output="$2"
    local min_zoom="${3:-2}"
    local max_zoom="${4:-12}"
    local name
    name="$(basename "$output" .mbtiles)"

    if [ ! -f "$input" ]; then
        echo "SKIP: $input not found"
        return
    fi

    echo "Converting $input → $output (zoom ${min_zoom}–${max_zoom})..."
    local tmp_geojson
    tmp_geojson="$(mktemp /tmp/tiles_XXXXXX.geojson)"

    ogr2ogr -f GeoJSON "$tmp_geojson" "$input" 2>/dev/null || {
        echo "  WARNING: ogr2ogr conversion failed for $input"
        rm -f "$tmp_geojson"
        return
    }

    tippecanoe \
        -o "$output" \
        -z "$max_zoom" \
        -Z "$min_zoom" \
        --name="$name" \
        --force \
        --no-tile-size-limit \
        --simplification=10 \
        --detect-shared-borders \
        "$tmp_geojson"

    rm -f "$tmp_geojson"
    echo "  ✓ Created $output"
}

# --- Convert pipeline data ---------------------------------------------------
PIPELINES_GPKG="$DATA_DIR/ogim-pipelines.gpkg"
if [ -f "$PIPELINES_GPKG" ]; then
    gpkg_to_mbtiles "$PIPELINES_GPKG" "$TILES_DIR/pipelines.mbtiles" 2 12
else
    echo "SKIP: $PIPELINES_GPKG not found."
    echo "  Download OGIM pipeline data from: https://globalenergymonitor.org/projects/global-oil-infrastructure-tracker/"
fi

# --- Convert energy infrastructure data --------------------------------------
ENERGY_GPKG="$DATA_DIR/osm-energy-infra.gpkg"
if [ -f "$ENERGY_GPKG" ]; then
    gpkg_to_mbtiles "$ENERGY_GPKG" "$TILES_DIR/energy-infra.mbtiles" 4 14
else
    echo "SKIP: $ENERGY_GPKG not found."
    echo "  Download OSM harmonized wind/solar data or use Overpass to extract."
fi

# --- Base map tiles -----------------------------------------------------------
BASE_TILES="$TILES_DIR/basemap.mbtiles"
if [ -f "$BASE_TILES" ]; then
    echo "Base map tiles already present: $BASE_TILES"
else
    echo ""
    echo "No base map tiles found at $BASE_TILES."
    echo "Options to obtain base tiles:"
    echo "  1. Protomaps: https://protomaps.com/downloads — download a regional .pmtiles extract"
    echo "  2. OpenMapTiles: https://openmaptiles.org/ — download planet.mbtiles"
    echo "  3. Use a free hosted style (no local tiles needed):"
    echo "     https://tiles.openfreemap.org/styles/liberty"
    echo ""
fi

echo ""
echo "Done. Tiles directory contents:"
ls -lh "$TILES_DIR/" 2>/dev/null || echo "  (empty)"
