#!/usr/bin/env bash
set -euo pipefail

# WorldMap Infrastructure Explorer — one-time setup
# Installs system deps, creates Python venv, and runs all ingestion scripts.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== WorldMap Setup ==="
echo ""

# ---------- System dependencies -----------------------------------------------

check_cmd() {
    command -v "$1" &>/dev/null
}

missing=()

if ! check_cmd rustc; then
    missing+=("rust (install via https://rustup.rs)")
fi

if ! check_cmd node; then
    missing+=("node (>= 20, https://nodejs.org)")
fi

if ! check_cmd python3; then
    missing+=("python3 (>= 3.10)")
fi

if ! check_cmd tippecanoe; then
    echo "⚠  tippecanoe not found — tile building will be skipped."
    echo "   macOS:  brew install tippecanoe"
    echo "   Ubuntu: sudo apt-get install -y tippecanoe"
    echo ""
fi

if [ ${#missing[@]} -gt 0 ]; then
    echo "ERROR: Missing required tools:"
    for m in "${missing[@]}"; do
        echo "  • $m"
    done
    exit 1
fi

echo "✓ System dependencies OK"

# ---------- Frontend npm install ----------------------------------------------

echo ""
echo "--- Installing frontend dependencies ---"
cd "$ROOT_DIR/frontend"
npm install

# ---------- Python venv + ingestion ------------------------------------------

echo ""
echo "--- Setting up Python venv ---"
cd "$ROOT_DIR"
if [ ! -d .venv ]; then
    python3 -m venv .venv
fi
source .venv/bin/activate
pip install --quiet requests

echo ""
echo "--- Running ingestion scripts ---"
cd "$ROOT_DIR/scripts"

python3 ingest_airports.py
python3 ingest_seaports.py
python3 ingest_reactors.py

# ---------- Build tiles (optional) --------------------------------------------

if check_cmd tippecanoe && check_cmd ogr2ogr; then
    echo ""
    echo "--- Building vector tiles ---"
    bash "$SCRIPT_DIR/build_tiles.sh"
else
    echo ""
    echo "⚠  Skipping tile build (tippecanoe or ogr2ogr not found)"
fi

# ---------- Backend .env ------------------------------------------------------

if [ ! -f "$ROOT_DIR/backend/.env" ]; then
    echo ""
    echo "--- Creating backend/.env from template ---"
    cp "$ROOT_DIR/backend/.env.example" "$ROOT_DIR/backend/.env"
    echo "⚠  Edit backend/.env and fill in your API keys!"
fi

echo ""
echo "=== Setup complete ==="
echo "Run 'make dev' to start the development servers."
