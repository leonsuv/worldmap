/**
 * deck.gl layers for high-volume live data. Built only when inputs change;
 * sizes follow the zoom so symbols stay legible from the globe to street level.
 */
import { IconLayer, PathLayer, ScatterplotLayer, TextLayer } from '@deck.gl/layers'
import type { Layer } from '@deck.gl/core'
import { CATEGORY_COLOR, isStationary, shipCategory, type ShipCategory } from '../lib/ais'
import { altitudeColor } from '../lib/aircraft'
import { hexToRgba, rampColor, zoomBucket, zoomRamp } from '../lib/scale'
import { getFlights, extrapolate, flightsDataTime, getFlight, type Flight } from '../data/flights'
import { getShips, getShip, type Ship } from '../data/ships'
import { getWeather, type WeatherPoint } from '../data/weather'
import { useLayers } from '../store/layers'
import { useViewport } from '../store/viewport'
import { useUi, type Theme } from '../store/ui'
import { useSelection, type Selection } from '../store/selection'
import { useEvents, EVENT_COLOR, type MapEvent } from '../store/events'
import { useHistory, type HistoryPoint } from '../store/history'
import { createRenderScheduler, memo, setRedraw } from './scheduler'
import { getFlightTrack, getShipTrack, type FlightTrack, type ShipTrack } from './tracks'
import { getLabelLayer, getOverlay } from './runtime'
import { iconAtlas } from './icons'
import { ACCENT, WIND_MAX, WIND_STOPS, type RGBA } from './colors'


// Live symbols stay visible above 3D buildings and terrain.
const DRAW_ON_TOP = { depthCompare: 'always', depthWriteEnabled: false } as const

function shade([r, g, b, a]: RGBA, f: number): RGBA {
  return [Math.round(r * f), Math.round(g * f), Math.round(b * f), a]
}

const categoryColors = memo((theme: Theme) => {
  const out = {} as Record<ShipCategory, RGBA>
  for (const [k, hex] of Object.entries(CATEGORY_COLOR)) out[k as ShipCategory] = theme === 'light' ? shade(hexToRgba(hex, 245), 0.8) : hexToRgba(hex, 240)
  return out
})


// ── builders ────────────────────────────────────────────────────────────────

const buildShips = memo((ships: Ship[], zoom: number, theme: Theme, flat: boolean, _epoch: number): Layer => {
  const colors = categoryColors(theme)
  // deck.gl's globe view cannot draw IconLayer, so the globe shows dots.
  if (zoom < 5.5 || !flat) {
    return new ScatterplotLayer<Ship>({
      id: 'ships',
      data: ships,
      getPosition: s => [s.lon, s.lat],
      getFillColor: s => colors[s.category],
      getRadius: zoomRamp(zoom, [[1, 1.1], [3, 1.5], [5.5, 2.6], [10, 4]]),
      radiusUnits: 'pixels',
      stroked: false,
      pickable: true,
      parameters: DRAW_ON_TOP,
    })
  }
  const base = zoomRamp(zoom, [[5.5, 10], [9, 15], [13, 20], [16, 26]])
  const { url, mapping } = iconAtlas()
  return new IconLayer<Ship>({
    id: 'ships',
    data: ships,
    iconAtlas: url,
    iconMapping: mapping,
    getIcon: s => (isStationary(s.speed, s.nav_status) ? 'shipStill' : 'ship'),
    getPosition: s => [s.lon, s.lat],
    getSize: s => (isStationary(s.speed, s.nav_status) ? base * 0.55 : base),
    getAngle: s => -(s.heading ?? s.course ?? 0),
    getColor: s => colors[s.category],
    sizeUnits: 'pixels',
    billboard: false,
    pickable: true,
    parameters: DRAW_ON_TOP,
  })
})

const buildHistory = memo((points: HistoryPoint[], zoom: number, theme: Theme, _epoch: number): Layer => {
  const colors = categoryColors(theme)
  return new ScatterplotLayer<HistoryPoint>({
    id: 'history-ships',
    data: points,
    getPosition: p => [p.lon, p.lat],
    getFillColor: p => colors[shipCategory(p.ship_type)],
    getRadius: zoomRamp(zoom, [[1, 1.3], [6, 3], [12, 5]]),
    radiusUnits: 'pixels',
    pickable: false,
    parameters: DRAW_ON_TOP,
  })
})

const buildFlights = (flights: Flight[], zoom: number, tick: number, flat: boolean): Layer => {
  const now = tick / 1000
  const fallback = flightsDataTime()
  if (!flat) {
    return new ScatterplotLayer<Flight>({
      id: 'flights',
      data: flights,
      getPosition: f => extrapolate(f, now, fallback),
      getFillColor: f => altitudeColor(f.altitude, f.on_ground),
      getRadius: zoomRamp(zoom, [[1, 1.4], [4, 2.2], [8, 3.5]]),
      radiusUnits: 'pixels',
      pickable: true,
      updateTriggers: { getPosition: tick },
    })
  }
  const { url, mapping } = iconAtlas()
  return new IconLayer<Flight>({
    id: 'flights',
    data: flights,
    iconAtlas: url,
    iconMapping: mapping,
    getIcon: () => 'plane',
    getPosition: f => extrapolate(f, now, fallback),
    getSize: zoomRamp(zoom, [[1, 7], [3, 9], [5, 12], [8, 17], [12, 24]]),
    getAngle: f => -(f.track ?? 0),
    getColor: f => altitudeColor(f.altitude, f.on_ground),
    sizeUnits: 'pixels',
    billboard: false,
    pickable: true,
    parameters: DRAW_ON_TOP,
    updateTriggers: { getPosition: tick },
  })
}

const buildWeather = memo((points: WeatherPoint[], zoom: number, theme: Theme, flat: boolean, _epoch: number): Layer[] => {
  const { url, mapping } = iconAtlas()
  const scale = zoomRamp(zoom, [[1, 0.8], [5, 1], [10, 1.25]])
  const wind = points.filter(p => p.wind_speed != null && p.wind_direction != null)
  const text: RGBA = theme === 'dark' ? [232, 238, 240, 255] : [22, 34, 42, 255]
  const background: RGBA = theme === 'dark' ? [15, 23, 28, 215] : [255, 255, 255, 225]
  if (!flat) {
    return [
      new ScatterplotLayer<WeatherPoint>({
        id: 'weather-wind',
        data: wind,
        getPosition: p => [p.lon, p.lat],
        getFillColor: p => rampColor(WIND_STOPS, (p.wind_speed ?? 0) / WIND_MAX, 235),
        getRadius: p => 3 + Math.min(5, (p.wind_speed ?? 0) / 4),
        radiusUnits: 'pixels',
        stroked: true,
        getLineColor: text,
        lineWidthMinPixels: 1,
        pickable: true,
      }),
    ]
  }
  return [
    new IconLayer<WeatherPoint>({
      id: 'weather-wind',
      data: wind,
      iconAtlas: url,
      iconMapping: mapping,
      getIcon: () => 'wind',
      getPosition: p => [p.lon, p.lat],
      getSize: p => Math.min(34, 15 + (p.wind_speed ?? 0) * 1.1) * scale,
      // Arrows point where the wind blows to (meteorological direction + 180°).
      getAngle: p => -((p.wind_direction ?? 0) + 180),
      getColor: p => rampColor(WIND_STOPS, (p.wind_speed ?? 0) / WIND_MAX, 245),
      sizeUnits: 'pixels',
      billboard: false,
      pickable: true,
      parameters: DRAW_ON_TOP,
    }),
    new TextLayer<WeatherPoint>({
      id: 'weather-temperature',
      data: points.filter(p => p.temperature != null),
      getPosition: p => [p.lon, p.lat],
      getText: p => `${Math.round(p.temperature ?? 0)}°`,
      getSize: 11.5 * Math.min(1.15, scale),
      getColor: text,
      getPixelOffset: [0, 20 * scale],
      characterSet: '0123456789-−°',
      fontFamily: 'system-ui, "Segoe UI", sans-serif',
      fontWeight: 600,
      background: true,
      getBackgroundColor: background,
      backgroundPadding: [4, 2],
      pickable: false,
      parameters: DRAW_ON_TOP,
    }),
  ]
})

const buildEvents = memo((events: MapEvent[], selectedId: number | null, before: string | undefined, theme: Theme, flat: boolean, _epoch: number): Layer[] => {
  const active = events.filter(e => e.active)
  if (!active.length) return []
  const color = (e: MapEvent, alpha: number) => hexToRgba(EVENT_COLOR[e.event_type] ?? '#5ea8f2', alpha)
  return [
    new ScatterplotLayer<MapEvent>({
      id: 'event-areas',
      data: active,
      getPosition: e => [e.lon, e.lat],
      getRadius: e => e.radius_km * 1000,
      radiusUnits: 'meters',
      getFillColor: e => color(e, e.id === selectedId ? 60 : 34),
      getLineColor: e => color(e, 220),
      getLineWidth: e => (e.id === selectedId ? 3 : 1.5),
      lineWidthUnits: 'pixels',
      stroked: true,
      filled: true,
      pickable: false,
      // Interleaved deck layers accept MapLibre's beforeId (not in the typings).
      ...({ beforeId: before } as object),
      updateTriggers: { getFillColor: selectedId, getLineWidth: selectedId },
    }),
    flat &&
    new TextLayer<MapEvent>({
      id: 'event-labels',
      data: active,
      getPosition: e => [e.lon, e.lat],
      getText: e => e.name,
      getSize: 12,
      getColor: e => color(e, 255),
      fontFamily: 'system-ui, "Segoe UI", sans-serif',
      fontWeight: 600,
      characterSet: 'auto',
      outlineWidth: 3,
      outlineColor: theme === 'dark' ? [14, 23, 28, 255] : [255, 255, 255, 255],
      fontSettings: { sdf: true },
      pickable: false,
    }),
  ].filter((l): l is ScatterplotLayer<MapEvent> | TextLayer<MapEvent> => !!l)
})

const buildFlightTrack = memo((track: FlightTrack, _epoch: number): Layer => {
  const segments = track.path.slice(1).map((p, i) => ({ path: [track.path[i], p], alt: track.path[i][2] }))
  return new PathLayer<{ path: [number, number, number][]; alt: number }>({
    id: 'flight-track',
    data: segments,
    getPath: d => d.path.map(p => [p[0], p[1]] as [number, number]),
    getColor: d => altitudeColor(d.alt, d.alt <= 0, 230),
    getWidth: 3,
    widthUnits: 'pixels',
    capRounded: true,
    jointRounded: true,
    pickable: false,
  })
})

const buildShipTrack = memo((track: ShipTrack, theme: Theme, _epoch: number): Layer[] => {
  const color = ACCENT[theme]
  return [
    new PathLayer<ShipTrack>({
      id: 'ship-track',
      data: [track],
      getPath: t => t.points.map(p => [p[0], p[1]] as [number, number]),
      getColor: [color[0], color[1], color[2], 210],
      getWidth: 2.5,
      widthUnits: 'pixels',
      jointRounded: true,
      capRounded: true,
      pickable: false,
    }),
    new ScatterplotLayer<[number, number, number]>({
      id: 'ship-track-points',
      data: track.points,
      getPosition: p => [p[0], p[1]],
      getRadius: 2.5,
      radiusUnits: 'pixels',
      getFillColor: color,
      pickable: false,
    }),
  ]
})

function selectionPosition(sel: Selection, now: number): [number, number] | null {
  if (sel.kind === 'flight') {
    const f = getFlight(sel.id)
    return f ? extrapolate(f, now, flightsDataTime()) : null
  }
  if (sel.kind === 'ship') {
    const s = getShip(sel.id)
    return s ? [s.lon, s.lat] : null
  }
  return sel.kind === 'hvline' || sel.kind === 'pipeline' || sel.kind === 'grid' ? null : sel.lngLat
}

function buildSelection(sel: Selection | null, tick: number, theme: Theme): Layer | null {
  if (!sel) return null
  const position = selectionPosition(sel, tick / 1000)
  if (!position) return null
  return new ScatterplotLayer<[number, number]>({
    id: 'selection-ring',
    data: [position],
    getPosition: p => p,
    getRadius: 13,
    radiusUnits: 'pixels',
    stroked: true,
    filled: true,
    getFillColor: [...ACCENT[theme].slice(0, 3), 40] as RGBA,
    getLineColor: ACCENT[theme],
    getLineWidth: 2,
    lineWidthUnits: 'pixels',
    pickable: false,
    parameters: DRAW_ON_TOP,
    updateTriggers: { getPosition: tick },
  })
}

// ── render loop ─────────────────────────────────────────────────────────────

let tick = Date.now()
// Layer instances belong to one Deck; a new overlay (e.g. after switching to the
// globe) must receive fresh instances, so the memo caches are keyed by epoch.
let epoch = 0
let lastOverlay: unknown = null

function render() {
  const overlay = getOverlay()
  if (!overlay) return
  if (overlay !== lastOverlay) {
    lastOverlay = overlay
    epoch++
  }
  const { enabled } = useLayers.getState()
  const zoom = zoomBucket(useViewport.getState().zoom)
  const theme = useUi.getState().theme
  // The globe uses deck.gl's GlobeView, which draws dots instead of icons.
  const flat = useUi.getState().projection !== 'globe'
  const selected = useSelection.getState().selected
  const history = useHistory.getState()
  const events = useEvents.getState()

  const layers: (Layer | null)[] = [...buildEvents(events.events, events.selectedId, getLabelLayer(), theme, flat, epoch)]
  if (enabled.weather && getWeather().length) layers.push(...buildWeather(getWeather(), zoom, theme, flat, epoch))
  if (history.enabled) {
    if (history.positions.length) layers.push(buildHistory(history.positions, zoom, theme, epoch))
  } else if (enabled.ships && getShips().length) {
    layers.push(buildShips(getShips(), zoom, theme, flat, epoch))
  }
  const shipTrack = getShipTrack()
  if (shipTrack && shipTrack.points.length > 1) layers.push(...buildShipTrack(shipTrack, theme, epoch))
  const flightTrack = getFlightTrack()
  if (flightTrack && flightTrack.path.length > 1) layers.push(buildFlightTrack(flightTrack, epoch))
  if (enabled.flights && getFlights().length) layers.push(buildFlights(getFlights(), zoom, tick, flat))
  if (useUi.getState().panel === 'details') layers.push(buildSelection(selected, tick, theme))

  overlay.setProps({
    layers: layers.filter((l): l is Layer => l !== null),
  })
}

const scheduler = createRenderScheduler(render)

setRedraw(scheduler.invalidate)

let ticker: ReturnType<typeof setInterval> | undefined
const unsubscribers: (() => void)[] = []

export function startDeckRendering() {
  scheduler.start()
  // Advance extrapolated aircraft once per second while flights are shown.
  ticker = setInterval(() => {
    if (document.hidden) return
    const { enabled } = useLayers.getState()
    const sel = useSelection.getState().selected
    if ((enabled.flights && getFlights().length) || sel?.kind === 'flight') {
      tick = Date.now()
      scheduler.invalidate()
    }
  }, 1000)
  for (const store of [useLayers, useSelection, useEvents, useHistory] as { subscribe: (fn: () => void) => () => void }[]) {
    unsubscribers.push(store.subscribe(scheduler.invalidate))
  }
  // The viewport store also tracks the pointer; only zoom changes matter here.
  unsubscribers.push(useViewport.subscribe((s, prev) => s.zoom !== prev.zoom && scheduler.invalidate()))
  unsubscribers.push(useUi.subscribe((s, prev) => (s.theme !== prev.theme || s.panel !== prev.panel || s.projection !== prev.projection) && scheduler.invalidate()))
}

export function stopDeckRendering() {
  scheduler.stop()
  clearInterval(ticker)
  unsubscribers.splice(0).forEach(u => u())
}
