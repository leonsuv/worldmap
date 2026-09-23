import type { Map as MapLibreMap } from 'maplibre-gl'
import type { MapLibreOverlay } from '@deck.gl/maplibre'

let map: MapLibreMap | null = null
let overlay: MapLibreOverlay | null = null
let labelLayer: string | undefined

export function setRuntime(m: MapLibreMap | null, o: MapLibreOverlay | null) {
  map = m
  overlay = o
}

export const getMap = () => map
export const getOverlay = () => overlay
export const getLabelLayer = () => labelLayer
export const setLabelLayer = (id: string | undefined) => {
  labelLayer = id
}

/** Right-hand padding so targets are not hidden behind the open drawer. */
function drawerPadding(): number {
  const drawer = document.querySelector('.drawer')
  return drawer && window.innerWidth > 760 ? drawer.getBoundingClientRect().width : 0
}

export function flyTo(lon: number, lat: number, zoom?: number) {
  if (!map) return
  const target = zoom ?? Math.max(map.getZoom(), 8)
  map.flyTo({ center: [lon, lat], zoom: target, duration: 1400, essential: true, padding: { right: drawerPadding(), left: 0, top: 0, bottom: 0 } })
}

export function fitCircle(lat: number, lon: number, radiusKm: number) {
  if (!map) return
  const dLat = radiusKm / 111
  const dLon = radiusKm / (111 * Math.max(0.1, Math.cos((lat * Math.PI) / 180)))
  map.fitBounds(
    [
      [lon - dLon, Math.max(-85, lat - dLat)],
      [lon + dLon, Math.min(85, lat + dLat)],
    ],
    { padding: { top: 60, bottom: 60, left: 60, right: 60 + drawerPadding() }, duration: 1200, maxZoom: 12 },
  )
}
