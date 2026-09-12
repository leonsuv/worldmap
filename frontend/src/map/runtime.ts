import type { Map } from 'maplibre-gl'
import type { MapLibreOverlay } from '@deck.gl/maplibre'

export let mapInstance: Map | null = null
export let deckOverlay: MapLibreOverlay | null = null
export function setMapRuntime(map: Map | null, overlay: MapLibreOverlay | null) {
  mapInstance = map
  deckOverlay = overlay
}
