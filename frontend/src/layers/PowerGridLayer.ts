import type maplibregl from 'maplibre-gl'

const GRID_SOURCE = 'power-grid-src'
const GRID_LAYER = 'power-grid-line'
const HV_SOURCE = 'hv-lines-src'
const HV_LAYER = 'hv-lines-line'

export function syncPowerGridLayer(
  map: maplibregl.Map,
  gridVisible: boolean,
  hvVisible: boolean,
) {
  // ── Gridfinder predicted grid (global coverage) ──
  if (!map.getSource(GRID_SOURCE)) {
    try {
      map.addSource(GRID_SOURCE, {
        type: 'vector',
        tiles: [`${window.location.origin}/tiles/power-grid/{z}/{x}/{y}`],
        minzoom: 2,
        maxzoom: 12,
      })
      map.addLayer({
        id: GRID_LAYER,
        type: 'line',
        source: GRID_SOURCE,
        'source-layer': 'grid',
        minzoom: 2,
        paint: {
          'line-color': '#ffd43b',
          'line-width': ['interpolate', ['linear'], ['zoom'], 2, 0.5, 8, 1.5],
          'line-opacity': 0.6,
        },
        layout: { visibility: 'none' },
      })
    } catch {
      // tiles may not be built yet
    }
  }

  // ── OSM verified high-voltage lines ──
  if (!map.getSource(HV_SOURCE)) {
    try {
      map.addSource(HV_SOURCE, {
        type: 'vector',
        tiles: [`${window.location.origin}/tiles/hv-lines/{z}/{x}/{y}`],
        minzoom: 3,
        maxzoom: 14,
      })
      map.addLayer({
        id: HV_LAYER,
        type: 'line',
        source: HV_SOURCE,
        'source-layer': 'hvlines',
        minzoom: 4,
        paint: {
          'line-color': [
            'interpolate', ['linear'],
            ['to-number', ['get', 'voltage'], 110000],
            110000, '#74c0fc',
            220000, '#f59f00',
            380000, '#f03e3e',
          ],
          'line-width': ['interpolate', ['linear'], ['zoom'], 4, 1, 12, 3],
        },
        layout: { visibility: 'none' },
      })
    } catch {
      // tiles may not be built yet
    }
  }

  try { map.setLayoutProperty(GRID_LAYER, 'visibility', gridVisible ? 'visible' : 'none') } catch { /* */ }
  try { map.setLayoutProperty(HV_LAYER, 'visibility', hvVisible ? 'visible' : 'none') } catch { /* */ }
}
