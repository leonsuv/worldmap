import { useEffect, useRef } from 'react'
import { useLayerStore } from '../store/layers'
import { useViewportStore } from '../store/viewport'
import { usePopupStore } from '../store/popup'
import { deckOverlay, mapInstance } from '../map/MapContainer'
import { buildFlightsLayer, buildTrackLayer } from './FlightsLayer'
import { buildShipsLayer, startShipsWs, stopShipsWs } from './ShipsLayer'
import { buildWeatherLayer } from './WeatherLayer'
import { buildReactorsLayer } from './ReactorsLayer'
import { buildTrafficLayer } from './TrafficLayer'
import { buildAirportsLayer } from './AirportsLayer'
import { buildSeaportsLayer } from './SeaportsLayer'
import { syncPipelinesLayer } from './PipelinesLayer'
import { syncBuildings3DLayer } from './Buildings3DLayer'
import { syncPowerGridLayer } from './PowerGridLayer'
import { buildAtoNLayer } from './AtoNLayer'
import { buildEventRadiusLayers } from './EventRadiusLayer'
import { buildHistoryShipsLayer } from './HistoryShipsLayer'
import { useFlightDetailStore } from '../store/flightDetail'
import { useShipDetailStore } from '../store/shipDetail'
import { useEventStore } from '../store/events'
import { useHistoryStore } from '../store/history'
import type { ShipDetail } from '../store/shipDetail'
import type { FlightRecord } from '../store/flightDetail'
import type { Layer } from '@deck.gl/core'

const clamp = (n: number, min: number, max: number) => Math.max(min, Math.min(max, n))

// ── Module-level render loop (decoupled from React) ──

let rafId = 0
let loopRunning = false

// Mutable refs read by the RAF loop — never cause React re-renders
const layerData = {
  flights: null as GeoJSON.FeatureCollection | null,
  shipsArr: [] as GeoJSON.Feature[],
  weather: [] as { lon: number; lat: number; speed: number; dir: number }[],
  reactors: null as GeoJSON.FeatureCollection | null,
  traffic: [] as { coordinates: [number, number][]; color: [number, number, number] }[],
  airports: null as GeoJSON.FeatureCollection | null,
  seaports: null as GeoJSON.FeatureCollection | null,
  aton: null as GeoJSON.FeatureCollection | null,
  flightTrack: null as [number, number, number][] | null,
  eventRadii: [] as import('../store/events').EventItem[],
  historyPositions: [] as import('../store/history').HistoryPoint[],
}

// Snapshot of store booleans + zoom, written from React, read from RAF
const flags = {
  flights: false,
  ships: false,
  weather: false,
  reactors: false,
  traffic: false,
  airports: false,
  seaports: false,
  aton: false,
  zoom: 2,
  openPopup: null as ((lng: [number, number], id: string, props: Record<string, unknown>) => void) | null,
}

// ── Flight detail: fetch track + recent flights when a flight is clicked ──

async function selectFlight(icao24: string, callsign: string, origin_country: string) {
  if (!icao24) return
  const store = useFlightDetailStore.getState()
  store.select(icao24, callsign, origin_country)

  // Fetch live track (time=0 means current flight)
  try {
    const r = await fetch(`/api/flights/track?icao24=${encodeURIComponent(icao24)}&time=0`)
    if (r.ok) {
      const data = await r.json()
      const path: [number, number, number][] = (data.path ?? []).map(
        (wp: (number | null)[]) => [wp[2] ?? 0, wp[1] ?? 0, wp[3] ?? 0]  // [lon, lat, alt]
      )
      store.setTrack(path)
      layerData.flightTrack = path
    } else {
      store.setTrack([])
      layerData.flightTrack = null
    }
  } catch {
    store.setTrack([])
    layerData.flightTrack = null
  }

  // Fetch recent flights for this aircraft (yesterday — batch data only)
  try {
    const now = Math.floor(Date.now() / 1000)
    // Flights are batch-processed overnight, so query yesterday 00:00 → today 00:00
    const endOfToday = now - (now % 86400)        // start of current UTC day
    const begin = endOfToday - 86400              // 24h before that
    const r = await fetch(
      `/api/flights/aircraft?icao24=${encodeURIComponent(icao24)}&begin=${begin}&end=${endOfToday}`
    )
    if (r.ok) {
      const data = await r.json()
      const flights: FlightRecord[] = (data as Record<string, unknown>[]).map((f) => ({
        icao24: String(f.icao24 ?? ''),
        firstSeen: Number(f.firstSeen ?? 0),
        lastSeen: Number(f.lastSeen ?? 0),
        estDepartureAirport: f.estDepartureAirport as string | null,
        estArrivalAirport: f.estArrivalAirport as string | null,
        callsign: f.callsign as string | null,
      }))
      store.setFlights(flights)
    } else {
      store.setFlights([])
    }
  } catch {
    store.setFlights([])
  }

  store.setLoading(false)
}

function renderLoop() {
  if (!loopRunning) return
  if (deckOverlay) {
    const dl: Layer[] = []
    if (flags.flights && layerData.flights) dl.push(buildFlightsLayer(layerData.flights))
    if (flags.ships && layerData.shipsArr.length) dl.push(buildShipsLayer(layerData.shipsArr))
    if (flags.weather && layerData.weather.length) dl.push(...buildWeatherLayer(layerData.weather))
    if (flags.reactors && layerData.reactors) dl.push(buildReactorsLayer(layerData.reactors))
    if (flags.traffic && layerData.traffic.length) dl.push(buildTrafficLayer(layerData.traffic))
    if (flags.airports && layerData.airports) dl.push(buildAirportsLayer(layerData.airports))
    if (flags.seaports && layerData.seaports) dl.push(buildSeaportsLayer(layerData.seaports))
    if (flags.aton && layerData.aton) dl.push(buildAtoNLayer(layerData.aton))
    if (layerData.flightTrack && layerData.flightTrack.length > 1) dl.push(buildTrackLayer(layerData.flightTrack))
    if (layerData.eventRadii.length) dl.push(...buildEventRadiusLayers(layerData.eventRadii))
    if (layerData.historyPositions.length) dl.push(buildHistoryShipsLayer(layerData.historyPositions))

    deckOverlay.setProps({
      layers: dl,
      onClick: (info: Record<string, unknown>) => {
        const obj = info.object as { properties?: Record<string, unknown> } | undefined
        const coord = info.coordinate as number[] | undefined
        const layer = info.layer as { id?: string } | null | undefined
        if (obj?.properties && coord && layer?.id) {
          if (layer.id === 'flights') {
            const p = obj.properties as Record<string, unknown>
            selectFlight(
              String(p.icao24 ?? ''),
              String(p.callsign ?? ''),
              String(p.origin_country ?? ''),
            )
          } else if (layer.id === 'ships') {
            const p = obj.properties as Record<string, unknown>
            useShipDetailStore.getState().select({
              mmsi: Number(p.mmsi ?? 0),
              ship_name: String(p.ship_name ?? ''),
              ship_type: p.ship_type != null ? Number(p.ship_type) : null,
              course: p.course != null ? Number(p.course) : null,
              speed: p.speed != null ? Number(p.speed) : null,
              heading: p.heading != null ? Number(p.heading) : null,
              imo: p.imo != null ? Number(p.imo) : null,
              callsign: p.callsign != null ? String(p.callsign) : null,
              destination: p.destination != null ? String(p.destination) : null,
              eta: p.eta != null ? String(p.eta) : null,
              draught: p.draught != null ? Number(p.draught) : null,
              length: p.length != null ? Number(p.length) : null,
              beam: p.beam != null ? Number(p.beam) : null,
              nav_status: p.nav_status != null ? Number(p.nav_status) : null,
            } as ShipDetail)
          } else {
            flags.openPopup?.([coord[0], coord[1]], layer.id, obj.properties)
          }
        }
      },
    })
  }
  rafId = requestAnimationFrame(renderLoop)
}

function startLoop() {
  if (loopRunning) return
  loopRunning = true
  rafId = requestAnimationFrame(renderLoop)
}

function stopLoop() {
  loopRunning = false
  cancelAnimationFrame(rafId)
}

// ── React hook: manages data fetching, writes into module-level refs ──

export function useLayers() {
  const layers = useLayerStore()
  const viewport = useViewportStore()
  const openPopup = usePopupStore((s) => s.open)

  // Keep flags in sync for the RAF loop
  useEffect(() => {
    flags.flights = layers.flights
    flags.ships = layers.ships
    flags.weather = layers.weather
    flags.reactors = layers.reactors
    flags.traffic = layers.traffic
    flags.airports = layers.airports
    flags.seaports = layers.seaports
    flags.aton = layers.aton
    flags.zoom = viewport.zoom
    flags.openPopup = openPopup
  })

  // Start / stop RAF loop
  useEffect(() => { startLoop(); return stopLoop }, [])

  // ── Clear flight track when detail panel closes ──
  useEffect(() => {
    return useFlightDetailStore.subscribe((s) => {
      if (!s.detail) layerData.flightTrack = null
    })
  }, [])

  // ── Flights polling ──
  const flightsAbort = useRef<AbortController | null>(null)

  useEffect(() => {
    if (!layers.flights) {
      flightsAbort.current?.abort()
      layerData.flights = null
      return
    }
    const ac = new AbortController()
    flightsAbort.current = ac
    const fetchFlights = async () => {
      if (document.visibilityState === 'hidden') return
      try {
        const r = await fetch('/api/flights', { signal: ac.signal })
        if (r.ok && !ac.signal.aborted) layerData.flights = await r.json()
      } catch { /* aborted or network error */ }
    }
    fetchFlights()
    const id = setInterval(fetchFlights, 30_000)
    return () => { clearInterval(id); ac.abort() }
  }, [layers.flights])

  // ── Ships WebSocket ──
  const shipsMap = useRef<Map<number, GeoJSON.Feature>>(new Map())

  useEffect(() => {
    if (!layers.ships) {
      stopShipsWs()
      shipsMap.current.clear()
      layerData.shipsArr = []
      return
    }
    startShipsWs(shipsMap.current)
    const id = setInterval(() => {
      layerData.shipsArr = Array.from(shipsMap.current.values())
    }, 2_000)
    return () => { clearInterval(id); stopShipsWs() }
  }, [layers.ships])

  // ── Weather ──
  const weatherAbort = useRef<AbortController | null>(null)
  const weatherDebounce = useRef<number | null>(null)

  useEffect(() => {
    if (!layers.weather) {
      weatherAbort.current?.abort()
      if (weatherDebounce.current) {
        clearTimeout(weatherDebounce.current)
        weatherDebounce.current = null
      }
      layerData.weather = []
      return
    }

    const getWeatherSamplePoints = (
      bbox: [number, number, number, number],
      zoom: number,
    ): { lat: number; lon: number }[] => {
      const [w, s, e, n] = bbox
      const cw = Math.max(-180, w)
      const cs = Math.max(-90, s)
      const ce = Math.min(180, e)
      const cn = Math.min(90, n)

      const cols = zoom < 3.2 ? 5 : zoom < 5.5 ? 6 : 7
      const rows = zoom < 3.2 ? 3 : zoom < 5.5 ? 4 : 5

      const dLon = (ce - cw) / cols
      const dLat = (cn - cs) / rows
      if (!Number.isFinite(dLon) || !Number.isFinite(dLat) || dLon <= 0 || dLat <= 0) return []

      const points: { lat: number; lon: number }[] = []
      for (let r = 0; r <= rows; r++) {
        for (let c = 0; c <= cols; c++) {
          const lon = +(cw + c * dLon).toFixed(2)
          const lat = +(cs + r * dLat).toFixed(2)
          points.push({ lat, lon })
        }
      }

      // Hard upper bound to protect upstream API.
      const limit = zoom < 4 ? 16 : 24
      return points.slice(0, limit)
    }

    const mapWithConcurrency = async <T, R>(
      items: T[],
      concurrency: number,
      fn: (item: T) => Promise<R>,
    ): Promise<R[]> => {
      const out: R[] = new Array(items.length)
      let idx = 0
      const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
        while (true) {
          const i = idx
          idx += 1
          if (i >= items.length) return
          out[i] = await fn(items[i])
        }
      })
      await Promise.all(workers)
      return out
    }

    const ac = new AbortController()
    weatherAbort.current = ac

    const fetchWeather = async () => {
      const points = getWeatherSamplePoints(viewport.bbox, viewport.zoom)
      if (points.length === 0) return

      try {
        const results = await mapWithConcurrency(points, 4, async (p) => {
          const r = await fetch(`/api/weather?lat=${p.lat}&lon=${p.lon}`, { signal: ac.signal })
          if (!r.ok) return null
          const d = await r.json()
          const h = d.hourly
          if (!h) return null
          return {
            lon: p.lon,
            lat: p.lat,
            speed: Number(h.windspeed_10m?.[0] ?? 0),
            dir: Number(h.winddirection_10m?.[0] ?? 0),
          }
        })

        if (!ac.signal.aborted) {
          layerData.weather = results
            .filter((r): r is NonNullable<typeof r> => r !== null)
            .filter((r) => Number.isFinite(r.lon) && Number.isFinite(r.lat))
            .map((r) => ({
              ...r,
              speed: clamp(r.speed, 0, 60),
              dir: ((r.dir % 360) + 360) % 360,
            }))
        }
      } catch { /* aborted */ }
    }

    weatherDebounce.current = window.setTimeout(fetchWeather, 450)
    return () => {
      if (weatherDebounce.current) {
        clearTimeout(weatherDebounce.current)
        weatherDebounce.current = null
      }
      ac.abort()
    }
  }, [layers.weather, viewport.bbox, viewport.zoom])

  // ── Reactors (fetch once) ──
  useEffect(() => {
    if (!layers.reactors || layerData.reactors) return
    const ac = new AbortController()
    fetch('/api/reactors', { signal: ac.signal })
      .then(r => r.ok ? r.json() : null)
      .then(d => { layerData.reactors = d })
      .catch(() => {})
    return () => ac.abort()
  }, [layers.reactors])

  // ── Traffic ──
  const trafficAbort = useRef<AbortController | null>(null)

  useEffect(() => {
    if (!layers.traffic || viewport.zoom < 10) {
      trafficAbort.current?.abort()
      layerData.traffic = []
      return
    }
    const fetchTraffic = async () => {
      trafficAbort.current?.abort()
      const ac = new AbortController()
      trafficAbort.current = ac
      const [w, s, e, n] = viewport.bbox
      try {
        const r = await fetch(`/api/traffic?bbox=${w},${s},${e},${n}`, { signal: ac.signal })
        if (!r.ok) return
        const d = await r.json()
        const seg = d.flowSegmentData
        if (!seg?.coordinates?.coordinate) return
        const coords = seg.coordinates.coordinate.map((c: { latitude: number; longitude: number }) => [c.longitude, c.latitude] as [number, number])
        const ratio = seg.currentSpeed / Math.max(seg.freeFlowSpeed, 1)
        const color: [number, number, number] = ratio > 0.75 ? [0, 200, 0] : ratio > 0.5 ? [255, 200, 0] : ratio > 0.25 ? [255, 80, 0] : [200, 0, 0]
        layerData.traffic = [{ coordinates: coords, color }]
      } catch { /* aborted */ }
    }
    const timer = setTimeout(fetchTraffic, 1000)
    return () => { clearTimeout(timer); trafficAbort.current?.abort() }
  }, [layers.traffic, viewport.bbox, viewport.zoom])

  // ── Airports (fetch once) ──
  useEffect(() => {
    if (!layers.airports || layerData.airports) return
    const ac = new AbortController()
    fetch('/api/airports', { signal: ac.signal })
      .then(r => r.ok ? r.json() : null)
      .then(d => { layerData.airports = d })
      .catch(() => {})
    return () => ac.abort()
  }, [layers.airports])

  // ── Seaports (fetch once) ──
  useEffect(() => {
    if (!layers.seaports || layerData.seaports) return
    const ac = new AbortController()
    fetch('/api/seaports', { signal: ac.signal })
      .then(r => r.ok ? r.json() : null)
      .then(d => { layerData.seaports = d })
      .catch(() => {})
    return () => ac.abort()
  }, [layers.seaports])

  // ── AtoN (Aids to Navigation) — periodic polling ──
  const atonAbort = useRef<AbortController | null>(null)

  useEffect(() => {
    if (!layers.aton) {
      atonAbort.current?.abort()
      layerData.aton = null
      return
    }
    const ac = new AbortController()
    atonAbort.current = ac
    const fetchAton = async () => {
      try {
        const r = await fetch('/api/ships/aton', { signal: ac.signal })
        if (r.ok && !ac.signal.aborted) layerData.aton = await r.json()
      } catch { /* aborted */ }
    }
    fetchAton()
    const id = setInterval(fetchAton, 60_000)
    return () => { clearInterval(id); ac.abort() }
  }, [layers.aton])

  // ── MapLibre native layers (pipelines, power grid, buildings) ──
  useEffect(() => {
    if (!mapInstance) return
    const map = mapInstance
    const onStyleLoad = () => {
      syncPipelinesLayer(map, layers.pipelines)
      syncPowerGridLayer(map, layers.powerGrid, layers.hvLines)
      syncBuildings3DLayer(map, layers.buildings3d, viewport.zoom)
    }
    if (map.isStyleLoaded()) onStyleLoad()
    else map.once('style.load', onStyleLoad)
  }, [layers.pipelines, layers.powerGrid, layers.hvLines, layers.buildings3d, viewport.zoom])

  // ── Event radius circles (sync from event store) ──
  useEffect(() => {
    return useEventStore.subscribe(s => {
      layerData.eventRadii = s.events.filter(e => e.active)
    })
  }, [])

  // ── Historical replay (sync positions from history store) ──
  useEffect(() => {
    return useHistoryStore.subscribe(s => {
      layerData.historyPositions = s.enabled ? s.positions : []
    })
  }, [])
}
