# Thin wrappers around the cross-platform Python scripts.
# On Windows without make, run the commands shown next to each target directly.
PYTHON ?= python3

.PHONY: help setup dev build ingest tiles hv-lines pipelines power-grid test smoke clean

help:                  ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

setup:                 ## Install dependencies and import datasets   (python scripts/setup.py)
	$(PYTHON) scripts/setup.py

dev:                   ## Run backend + frontend dev servers          (python scripts/dev.py)
	$(PYTHON) scripts/dev.py

build:                 ## Production build of frontend and backend
	cd frontend && npm ci && npm run build
	cd backend && cargo build --release

ingest:                ## Import airports, seaports, nuclear plants   (python scripts/ingest.py)
	$(PYTHON) scripts/ingest.py

tiles:                 ## Build all optional network tile layers      (python scripts/build_tiles.py all)
	$(PYTHON) scripts/build_tiles.py all

hv-lines:              ## Build high-voltage line tiles (OpenStreetMap)
	$(PYTHON) scripts/build_tiles.py hv-lines

pipelines:             ## Build pipeline tiles (OpenStreetMap)
	$(PYTHON) scripts/build_tiles.py pipelines

power-grid:            ## Build estimated power-grid tiles (Gridfinder)
	$(PYTHON) scripts/build_tiles.py power-grid

test:                  ## Run all unit tests and linters
	cd frontend && npm run lint && npm test
	cd backend && cargo clippy --all-targets -- -D warnings && cargo test

smoke:                 ## HTTP smoke test against a running backend   (python scripts/smoke_test.py)
	$(PYTHON) scripts/smoke_test.py

clean:                 ## Remove build output
	rm -rf frontend/dist backend/target
