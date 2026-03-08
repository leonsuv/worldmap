import type maplibregl from 'maplibre-gl'

const LAYER_ID = 'buildings-3d'

export function syncBuildings3DLayer(map: maplibregl.Map, enabled: boolean, zoom: number) {
  const shouldShow = enabled && zoom >= 14

  if (map.getLayer(LAYER_ID)) {
    map.setLayoutProperty(LAYER_ID, 'visibility', shouldShow ? 'visible' : 'none')
    return
  }

  if (!shouldShow) return

  // The OpenFreeMap Liberty style includes an "openmaptiles" source with a "building" layer
  // Try to add fill-extrusion on top of it
  try {
    map.addLayer({
      id: LAYER_ID,
      type: 'fill-extrusion',
      source: 'openmaptiles',
      'source-layer': 'building',
      minzoom: 14,
      paint: {
        'fill-extrusion-color': [
          'match', ['get', 'class'],
          'residential', '#c4a882',
          'commercial', '#8eabcc',
          'industrial', '#b8b8b8',
          '#d4c4a8',
        ],
        'fill-extrusion-height': [
          'case',
          ['has', 'render_height'], ['get', 'render_height'],
          ['has', 'height'], ['get', 'height'],
          10,
        ],
        'fill-extrusion-base': [
          'case',
          ['has', 'render_min_height'], ['get', 'render_min_height'],
          0,
        ],
        'fill-extrusion-opacity': 0.7,
      },
    })
  } catch {
    // source may not have building layer
  }
}
