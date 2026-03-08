import type maplibregl from 'maplibre-gl'

const SOURCE_ID = 'pipelines-src'
const LAYER_ID = 'pipelines'

export function syncPipelinesLayer(map: maplibregl.Map, visible: boolean) {
  // Add source + layer if not present
  if (!map.getSource(SOURCE_ID)) {
    try {
      map.addSource(SOURCE_ID, {
        type: 'vector',
        tiles: [`${window.location.origin}/tiles/pipelines/{z}/{x}/{y}`],
        minzoom: 2,
        maxzoom: 12,
      })
      map.addLayer({
        id: LAYER_ID,
        type: 'line',
        source: SOURCE_ID,
        'source-layer': 'pipelines',
        minzoom: 2,
        paint: {
          'line-color': [
            'match', ['get', 'COMMODITY'],
            'OIL',              '#e63946',
            'PETROLEUM',        '#e63946',
            'CONDENSATE',       '#e63946',
            'REFINED PRODUCT',  '#e63946',
            'GAS',              '#f4a261',
            'NATURAL GAS',      '#f4a261',
            'GAS/LPG',          '#f4a261',
            'GAS/CONDENSATE',   '#f4a261',
            'COAL SEAM GAS',    '#f4a261',
            'FUEL GAS',         '#f4a261',
            'LPG',              '#457b9d',
            'OIL AND GAS',      '#e9c46a',
            '#adb5bd',
          ],
          'line-width': ['interpolate', ['linear'], ['zoom'], 2, 0.5, 6, 1.0, 10, 2.5],
          'line-opacity': 0.85,
        },
        layout: { visibility: 'none' },
      })
    } catch {
      // source/layer may already exist or tiles unavailable
      return
    }
  }

  map.setLayoutProperty(LAYER_ID, 'visibility', visible ? 'visible' : 'none')
}
