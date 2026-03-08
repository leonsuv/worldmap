import type maplibregl from 'maplibre-gl'

const SOURCE_ID = 'energy-infra-src'
const SOLAR_LAYER = 'solar'
const WIND_LAYER = 'wind-turbines'

export function syncEnergyInfraLayer(map: maplibregl.Map, solar: boolean, wind: boolean) {
  if (!map.getSource(SOURCE_ID)) {
    try {
      map.addSource(SOURCE_ID, {
        type: 'vector',
        url: '/tiles/energy-infra/tilejson.json',
      })

      map.addLayer({
        id: SOLAR_LAYER,
        type: 'circle',
        source: SOURCE_ID,
        'source-layer': 'solar',
        minzoom: 8,
        paint: {
          'circle-radius': 4,
          'circle-color': '#f0c808',
          'circle-opacity': 0.8,
        },
        layout: { visibility: 'none' },
      })

      map.addLayer({
        id: WIND_LAYER,
        type: 'circle',
        source: SOURCE_ID,
        'source-layer': 'wind',
        minzoom: 9,
        paint: {
          'circle-radius': 3,
          'circle-color': '#90e0ef',
          'circle-opacity': 0.8,
        },
        layout: { visibility: 'none' },
      })
    } catch {
      return
    }
  }

  try {
    map.setLayoutProperty(SOLAR_LAYER, 'visibility', solar ? 'visible' : 'none')
  } catch { /* layer not yet added */ }
  try {
    map.setLayoutProperty(WIND_LAYER, 'visibility', wind ? 'visible' : 'none')
  } catch { /* layer not yet added */ }
}
