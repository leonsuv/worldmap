/**
 * Unified hover and click handling for deck.gl objects and MapLibre features.
 */
import type { Map as MapLibreMap, MapGeoJSONFeature, MapMouseEvent } from 'maplibre-gl'
import { create } from 'zustand'
import { getOverlay } from './runtime'
import { PICKABLE_LAYERS } from './overlays'
import { useSelection, type Selection } from '../store/selection'
import { useUi } from '../store/ui'
import { useViewport } from '../store/viewport'
import type { Flight } from '../data/flights'
import type { Ship } from '../data/ships'
import type { WeatherPoint } from '../data/weather'
import { shipTypeLabel } from '../lib/ais'
import { fmtAltitude, fmtKnots, fmtNumber, fmtSpeedMs } from '../lib/format'

export interface Hover {
  x: number
  y: number
  title: string
  subtitle?: string
}

export const useHover = create<{ hover: Hover | null; set: (h: Hover | null) => void }>(set => ({ hover: null, set: hover => set({ hover }) }))

interface Picked {
  selection: Selection
  title: string
  subtitle?: string
}

function fromDeck(layerId: string, object: unknown, lngLat: [number, number]): Picked | null {
  if (layerId === 'flights') {
    const f = object as Flight
    return {
      selection: { kind: 'flight', id: f.icao24 },
      title: f.callsign || f.icao24.toUpperCase(),
      subtitle: f.on_ground ? 'On the ground' : `${fmtAltitude(f.altitude)} · ${fmtSpeedMs(f.velocity).split(' · ')[0]}`,
    }
  }
  if (layerId === 'ships') {
    const s = object as Ship
    return {
      selection: { kind: 'ship', id: s.mmsi },
      title: s.name || `MMSI ${s.mmsi}`,
      subtitle: `${s.sar ? 'SAR aircraft' : shipTypeLabel(s.ship_type)} · ${fmtKnots(s.speed)}`,
    }
  }
  if (layerId === 'weather-wind') {
    const p = object as WeatherPoint
    return {
      selection: { kind: 'weather', id: `${p.lat},${p.lon}`, lngLat: [p.lon, p.lat], props: { ...p } },
      title: `${fmtNumber(p.temperature, 0, '°C')} · wind ${fmtNumber(p.wind_speed, 1, 'm/s')}`,
      subtitle: 'Click for the 24-hour outlook',
    }
  }
  void lngLat
  return null
}

function fromFeature(feature: MapGeoJSONFeature, lngLat: [number, number]): Picked | null {
  const p = (feature.properties ?? {}) as Record<string, unknown>
  const point = feature.geometry.type === 'Point' ? (feature.geometry.coordinates as [number, number]) : lngLat
  const str = (v: unknown) => (v == null || v === '' ? undefined : String(v))
  switch (feature.layer.id) {
    case 'wm-airports-large':
    case 'wm-airports-medium':
      return {
        selection: { kind: 'airport', id: String(p.ident ?? p.name), lngLat: point, props: p },
        title: String(p.name ?? 'Airport'),
        subtitle: [str(p.iata), str(p.ident), str(p.city)].filter(Boolean).join(' · '),
      }
    case 'wm-seaports-major':
    case 'wm-seaports-minor':
      return {
        selection: { kind: 'seaport', id: String(p.locode ?? p.name), lngLat: point, props: p },
        title: String(p.name ?? 'Seaport'),
        subtitle: [str(p.locode), str(p.country)].filter(Boolean).join(' · '),
      }
    case 'wm-reactors': {
      // MapLibre serialises nested arrays in properties to JSON strings.
      const units = typeof p.units === 'string' ? JSON.parse(p.units) : p.units
      return {
        selection: { kind: 'reactor', id: String(p.name), lngLat: point, props: { ...p, units } },
        title: `${p.name} nuclear plant`,
        subtitle: `${p.country} · ${fmtNumber(Number(p.capacity_mw), 0, 'MW')}`,
      }
    }
    case 'wm-aton':
      return { selection: { kind: 'aton', id: String(p.mmsi), lngLat: point, props: p }, title: str(p.name) ?? 'Aid to navigation', subtitle: `MMSI ${p.mmsi}` }
    case 'wm-hv-line':
    case 'wm-hv-cable':
      return {
        selection: { kind: 'hvline', id: `${feature.id ?? p.name ?? ''}-${lngLat.join(',')}`, lngLat, props: p },
        title: str(p.name) ?? 'Power line',
        subtitle: [p.hvdc ? 'HVDC' : null, str(p.voltage_kv) ? `${p.voltage_kv} kV` : null, str(p.operator)].filter(Boolean).join(' · '),
      }
    case 'wm-pipelines':
      return {
        selection: { kind: 'pipeline', id: `${p.name ?? ''}-${lngLat.join(',')}`, lngLat, props: p },
        title: str(p.name) ?? str(p.FAC_NAME) ?? 'Pipeline',
        subtitle: [str(p.commodity) ?? str(p.COMMODITY), str(p.operator) ?? str(p.OPERATOR)].filter(Boolean).join(' · '),
      }
    case 'wm-grid':
      return { selection: { kind: 'grid', id: lngLat.join(','), lngLat, props: p }, title: 'Estimated power line', subtitle: 'Gridfinder model' }
  }
  return null
}

function pick(map: MapLibreMap, x: number, y: number, lngLat: [number, number]): Picked | null {
  const overlay = getOverlay()
  const info = overlay?.pickObject({ x, y, radius: 5 })
  if (info?.object && info.layer) {
    const picked = fromDeck(info.layer.id, info.object, lngLat)
    if (picked) return picked
  }
  const layers = PICKABLE_LAYERS.filter(id => map.getLayer(id) && map.getLayoutProperty(id, 'visibility') !== 'none')
  if (!layers.length) return null
  const features = map.queryRenderedFeatures(
    [
      [x - 5, y - 5],
      [x + 5, y + 5],
    ],
    { layers },
  )
  for (const f of features) {
    const picked = fromFeature(f, lngLat)
    if (picked) return picked
  }
  return null
}

export function attachInteraction(map: MapLibreMap): () => void {
  let frame: number | null = null
  let last: MapMouseEvent | null = null

  const hover = () => {
    frame = null
    if (!last) return
    const e = last
    const lngLat: [number, number] = [e.lngLat.lng, e.lngLat.lat]
    useViewport.getState().setPointer(lngLat)
    if (useUi.getState().pick) {
      map.getCanvas().style.cursor = 'crosshair'
      useHover.getState().set(null)
      return
    }
    const picked = pick(map, e.point.x, e.point.y, lngLat)
    map.getCanvas().style.cursor = picked ? 'pointer' : ''
    useHover.getState().set(picked ? { x: e.point.x, y: e.point.y, title: picked.title, subtitle: picked.subtitle } : null)
  }

  const onMove = (e: MapMouseEvent) => {
    last = e
    if (frame === null) frame = requestAnimationFrame(hover)
  }
  const onLeave = () => {
    last = null
    useHover.getState().set(null)
    useViewport.getState().setPointer(null)
    map.getCanvas().style.cursor = ''
  }
  const onClick = (e: MapMouseEvent) => {
    const lngLat: [number, number] = [e.lngLat.lng, e.lngLat.lat]
    const request = useUi.getState().pick
    if (request) {
      useUi.getState().cancelPick()
      request.onPick(lngLat)
      return
    }
    const picked = pick(map, e.point.x, e.point.y, lngLat)
    if (picked) useSelection.getState().select(picked.selection)
    else if (useSelection.getState().selected) useSelection.getState().clear()
  }
  const onDragStart = () => useHover.getState().set(null)

  map.on('mousemove', onMove)
  map.on('mouseout', onLeave)
  map.on('click', onClick)
  map.on('dragstart', onDragStart)
  return () => {
    if (frame !== null) cancelAnimationFrame(frame)
    map.off('mousemove', onMove)
    map.off('mouseout', onLeave)
    map.off('click', onClick)
    map.off('dragstart', onDragStart)
  }
}
