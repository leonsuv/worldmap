.PHONY: dev build ingest tiles clean setup

# ---------- Development -------------------------------------------------------

dev:                          ## Start backend + frontend dev servers
	@bash scripts/dev.sh

# ---------- Production build --------------------------------------------------

build: build-frontend build-backend   ## Full production build

build-frontend:
	cd frontend && npm install && npm run build

build-backend:
	cd backend && cargo build --release

# ---------- Data ingestion ----------------------------------------------------

ingest: ingest-airports ingest-seaports ingest-reactors   ## Run all ingestion scripts
	@echo "✓ All ingestion complete"

ingest-airports:
	cd scripts && python3 ingest_airports.py

ingest-seaports:
	cd scripts && python3 ingest_seaports.py

ingest-reactors:
	cd scripts && python3 ingest_reactors.py

tiles:                        ## Build vector tiles from GeoPackage sources
	bash scripts/build_tiles.sh

grid-tiles:                   ## Download & build power-grid + HV-line tiles
	bash scripts/build_grid_tiles.sh

pipeline-tiles:               ## Download & build pipeline tiles
	bash scripts/build_pipeline_tiles.sh

# ---------- Setup -------------------------------------------------------------

setup:                        ## One-time project setup
	@bash scripts/setup.sh

# ---------- Clean -------------------------------------------------------------

clean:                        ## Remove build artifacts
	rm -rf frontend/dist
	rm -rf backend/target
	@echo "✓ Cleaned build artifacts"

# ---------- Help --------------------------------------------------------------

help:                         ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
