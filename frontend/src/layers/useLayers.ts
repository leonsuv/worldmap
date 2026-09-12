import { useEffect, useRef, useState } from 'react'
import { createRenderScheduler, memoizeLayer } from './renderScheduler'
import { weatherSamplePoints } from './weatherSampling'
import { useDataStatus } from '../store/dataStatus'
import { useLayerStore } from '../store/layers'
import { useViewportStore } from '../store/viewport'
import { usePopupStore } from '../store/popup'
import { deckOverlay, mapInstance } from '../map/runtime'
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

type WeatherPoint = {
  lon: number
  lat: number
  speed: number
  dir: number
  gust?: number
  temperature?: number
  apparent_temperature?: number
  humidity?: number
  precipitation?: number
  weather_code?: number
  cloud_cover?: number
  pressure_msl?: number
  visibility?: number
  wave_height?: number
  wave_direction?: number
  wave_period?: number
}

// ── Module-level render loop (decoupled from React) ──

const scheduler = createRenderScheduler(renderLoop)
const builders = {
  flights: memoizeLayer(buildFlightsLayer), ships: memoizeLayer(buildShipsLayer),
  weather: memoizeLayer(buildWeatherLayer), reactors: memoizeLayer(buildReactorsLayer),
  traffic: memoizeLayer(buildTrafficLayer), airports: memoizeLayer(buildAirportsLayer),
  seaports: memoizeLayer(buildSeaportsLayer), aton: memoizeLayer(buildAtoNLayer),
  track: memoizeLayer(buildTrackLayer), events: memoizeLayer(buildEventRadiusLayers),
  history: memoizeLayer(buildHistoryShipsLayer),
}

// Mutable refs read by the RAF loop — never cause React re-renders
const layerData = new Proxy({
  flights: null as GeoJSON.FeatureCollection | null,
  shipsArr: [] as GeoJSON.Feature[],
  weather: [] as WeatherPoint[],
  reactors: null as GeoJSON.FeatureCollection | null,
  traffic: [] as { coordinates: [number, number][]; color: [number, number, number] }[],
  airports: null as GeoJSON.FeatureCollection | null,
  seaports: null as GeoJSON.FeatureCollection | null,
  aton: null as GeoJSON.FeatureCollection | null,
  flightTrack: null as [number, number, number][] | null,
  eventRadii: [] as import('../store/events').EventItem[],
  historyPositions: [] as import('../store/history').HistoryPoint[],
}, {
  set(target, key, value) {
    const changed = Reflect.get(target, key) !== value
    Reflect.set(target, key, value)
    if (changed) scheduler.invalidate()
    return true
  },
})

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

let flightSelection = 0
let flightController: AbortController | null = null

async function selectFlight(icao24: string, callsign: string, origin_country: string) {
  if (!icao24) return
  const selection = ++flightSelection
  flightController?.abort()
  const ac = new AbortController()
  flightController = ac
  layerData.flightTrack = null
  const current = () => selection === flightSelection && !ac.signal.aborted && useFlightDetailStore.getState().detail?.icao24 === icao24
  const store = useFlightDetailStore.getState()
  store.select(icao24, callsign, origin_country)

  // Fetch live track (time=0 means current flight)
  try {
    const r = await fetch(`/api/flights/track?icao24=${encodeURIComponent(icao24)}&time=0`, { signal: ac.signal })
    if (r.ok) {
      const data = await r.json()
      if (!current()) return
      const path: [number, number, number][] = (data.path ?? []).filter((wp: (number | null)[]) => typeof wp[1] === 'number' && typeof wp[2] === 'number' && Number.isFinite(wp[1]) && Number.isFinite(wp[2])).map(
        (wp: (number | null)[]) => [wp[2] ?? 0, wp[1] ?? 0, wp[3] ?? 0]  // [lon, lat, alt]
      )
      store.setTrack(path)
      layerData.flightTrack = path
    } else {
      if (!current()) return
      store.setTrack([])
      layerData.flightTrack = null
    }
  } catch {
    if (!current()) return
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
      `/api/flights/aircraft?icao24=${encodeURIComponent(icao24)}&begin=${begin}&end=${endOfToday}`, { signal: ac.signal }
    )
    if (r.ok) {
      const data = await r.json()
      if (!current()) return
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
      if (!current()) return
      store.setFlights([])
    }
  } catch {
    if (!current()) return
    store.setFlights([])
  }

  if (current()) store.setLoading(false)
}

function renderLoop() {
  if (deckOverlay) {
    const dl: Layer[] = []
    if (flags.flights && layerData.flights) dl.push(builders.flights(layerData.flights))
    if (flags.ships && !useHistoryStore.getState().enabled && layerData.shipsArr.length) dl.push(builders.ships(layerData.shipsArr))
    if (flags.weather && layerData.weather.length) dl.push(...builders.weather(layerData.weather))
    if (flags.reactors && layerData.reactors) dl.push(builders.reactors(layerData.reactors))
    if (flags.traffic && layerData.traffic.length) dl.push(builders.traffic(layerData.traffic))
    if (flags.airports && layerData.airports) dl.push(builders.airports(layerData.airports))
    if (flags.seaports && layerData.seaports) dl.push(builders.seaports(layerData.seaports))
    if (flags.aton && layerData.aton) dl.push(builders.aton(layerData.aton))
    if (layerData.flightTrack && layerData.flightTrack.length > 1) dl.push(builders.track(layerData.flightTrack))
    if (layerData.eventRadii.length) dl.push(...builders.events(layerData.eventRadii))
    if (layerData.historyPositions.length) dl.push(builders.history(layerData.historyPositions))

    deckOverlay.setProps({
      layers: dl,
      onClick: (info: Record<string, unknown>) => {
        const obj = info.object as { properties?: Record<string, unknown> } | undefined
        const coord = info.coordinate as number[] | undefined
        const layer = info.layer as { id?: string } | null | undefined
        if (obj?.properties && coord && layer?.id) {
          if (layer.id === 'flights') {
            useShipDetailStore.getState().close()
            usePopupStore.getState().close()
            const p = obj.properties as Record<string, unknown>
            selectFlight(
              String(p.icao24 ?? ''),
              String(p.callsign ?? ''),
              String(p.origin_country ?? ''),
            )
          } else if (layer.id === 'ships') {
            useFlightDetailStore.getState().close()
            usePopupStore.getState().close()
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
            useFlightDetailStore.getState().close()
            useShipDetailStore.getState().close()
            flags.openPopup?.([coord[0], coord[1]], layer.id, obj.properties)
          }
        }
      },
    })
  }
}

// ── React hook: manages data fetching, writes into module-level refs ──

export function useLayers() {
  const layers = useLayerStore()
  const viewport = useViewportStore()
  const openPopup = usePopupStore((s) => s.open)
  const [availableTiles, setAvailableTiles] = useState<string[]>([])
  useEffect(() => {
    const ac = new AbortController()
    fetch('/api/status', { signal: ac.signal }).then(r => {
      if (!r.ok) throw new Error('Backend unavailable')
      return r.json()
    }).then(data => {
      if (ac.signal.aborted) return
      setAvailableTiles(data.tiles)
      for (const [key, source] of [['pipelines', 'pipelines'], ['powerGrid', 'power-grid'], ['hvLines', 'hv-lines']]) {
        useDataStatus.getState().set(key, data.tiles.includes(source) ? { state: 'ready' } : { state: 'error', message: 'Local map data not installed' })
      }
    }).catch(() => {
      if (!ac.signal.aborted) for (const key of ['pipelines', 'powerGrid', 'hvLines']) useDataStatus.getState().set(key, { state: 'error', message: 'Backend unavailable' })
    })
    return () => ac.abort()
  }, [])

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
    scheduler.invalidate()
  })

  // Start / stop RAF loop
  useEffect(() => {
    scheduler.start()
    const onVisible = () => { if (!document.hidden) scheduler.invalidate() }
    document.addEventListener('visibilitychange', onVisible)
    return () => { scheduler.stop(); flightController?.abort(); document.removeEventListener('visibilitychange', onVisible) }
  }, [])

  // ── Clear flight track when detail panel closes ──
  useEffect(() => {
    return useFlightDetailStore.subscribe((s) => {
      if (!s.detail) { flightController?.abort(); layerData.flightTrack = null }
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
    let inFlight = false
    const fetchFlights = async () => {
      if (document.hidden || inFlight) return
      inFlight = true
      useDataStatus.getState().set('flights', { state: 'loading' })
      try {
        const r = await fetch('/api/flights', { signal: ac.signal })
        if (!r.ok) throw new Error('Flights unavailable')
        const data = await r.json()
        if (!ac.signal.aborted) {
          layerData.flights = data
          useDataStatus.getState().set('flights', { state: data.stale ? 'error' : 'ready', count: data.features.length, message: data.stale ? 'Cached positions · live feed unavailable' : undefined })
        }
      } catch { if (!ac.signal.aborted) useDataStatus.getState().set('flights', { state: 'error' }) } finally { inFlight = false }
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
    startShipsWs(shipsMap.current, () => {
      layerData.shipsArr = Array.from(shipsMap.current.values())
    })
    return stopShipsWs
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
      useDataStatus.getState().set('weather', { state: 'loading' })
      const points = weatherSamplePoints(viewport.bbox, viewport.zoom)
      if (points.length === 0) return

      try {
        const results = await mapWithConcurrency(points, 4, async (p) => {
          const r = await fetch(`/api/weather?lat=${p.lat}&lon=${p.lon}`, { signal: ac.signal })
          if (!r.ok) return null
          const d = await r.json()
          const h = d.hourly ?? {}
          if (!d.hourly && !d.current) return null
          const current = d.current ?? {}
          return {
            lon: p.lon,
            lat: p.lat,
            speed: Number(current.wind_speed_10m ?? h.wind_speed_10m?.[0] ?? h.windspeed_10m?.[0] ?? 0),
            dir: Number(current.wind_direction_10m ?? h.wind_direction_10m?.[0] ?? h.winddirection_10m?.[0] ?? 0),
            gust: Number(current.wind_gusts_10m ?? h.wind_gusts_10m?.[0] ?? 0),
            temperature: Number(current.temperature_2m ?? h.temperature_2m?.[0] ?? 0),
            apparent_temperature: Number(current.apparent_temperature ?? h.apparent_temperature?.[0] ?? 0),
            humidity: Number(current.relative_humidity_2m ?? h.relative_humidity_2m?.[0] ?? 0),
            precipitation: Number(current.precipitation ?? h.precipitation?.[0] ?? 0),
            weather_code: Number(current.weather_code ?? h.weather_code?.[0] ?? 0),
            cloud_cover: Number(current.cloud_cover ?? h.cloud_cover?.[0] ?? 0),
            pressure_msl: Number(current.pressure_msl ?? h.pressure_msl?.[0] ?? 0),
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
              gust: Number.isFinite(r.gust ?? NaN) ? clamp(r.gust ?? 0, 0, 90) : undefined,
            }))
        }
        if (!ac.signal.aborted) useDataStatus.getState().set('weather', { state: layerData.weather.length ? 'ready' : 'error', count: layerData.weather.length })
      } catch { if (!ac.signal.aborted) useDataStatus.getState().set('weather', { state: 'error' }) }
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
    useDataStatus.getState().set('reactors', { state: 'loading' })
    fetch('/api/reactors', { signal: ac.signal })
      .then(r => { if (!r.ok) throw new Error('Unavailable'); return r.json() })
      .then(d => { if (!ac.signal.aborted) { layerData.reactors = d; useDataStatus.getState().set('reactors', { state: 'ready', count: d.features.length }) } })
      .catch(() => { if (!ac.signal.aborted) useDataStatus.getState().set('reactors', { state: 'error' }) })
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
        if (!r.ok) throw new Error('Traffic unavailable')
        const d = await r.json()
        if (ac.signal.aborted) return
        const seg = d.flowSegmentData
        if (!seg?.coordinates?.coordinate) return
        const coords = seg.coordinates.coordinate.map((c: { latitude: number; longitude: number }) => [c.longitude, c.latitude] as [number, number])
        const ratio = seg.currentSpeed / Math.max(seg.freeFlowSpeed, 1)
        const color: [number, number, number] = ratio > 0.75 ? [0, 200, 0] : ratio > 0.5 ? [255, 200, 0] : ratio > 0.25 ? [255, 80, 0] : [200, 0, 0]
        layerData.traffic = [{ coordinates: coords, color }]
        useDataStatus.getState().set('traffic', { state: 'ready', count: 1 })
      } catch { if (!ac.signal.aborted) { layerData.traffic = []; useDataStatus.getState().set('traffic', { state: 'error' }) } }
    }
    const timer = setTimeout(fetchTraffic, 1000)
    return () => { clearTimeout(timer); trafficAbort.current?.abort() }
  }, [layers.traffic, viewport.bbox, viewport.zoom])

  // ── Airports (fetch once) ──
  useEffect(() => {
    if (!layers.airports || layerData.airports) return
    const ac = new AbortController()
    useDataStatus.getState().set('airports', { state: 'loading' })
    fetch('/api/airports', { signal: ac.signal })
      .then(r => { if (!r.ok) throw new Error('Unavailable'); return r.json() })
      .then(d => { if (!ac.signal.aborted) { layerData.airports = d; useDataStatus.getState().set('airports', { state: 'ready', count: d.features.length }) } })
      .catch(() => { if (!ac.signal.aborted) useDataStatus.getState().set('airports', { state: 'error' }) })
    return () => ac.abort()
  }, [layers.airports])

  // ── Seaports (fetch once) ──
  useEffect(() => {
    if (!layers.seaports || layerData.seaports) return
    const ac = new AbortController()
    useDataStatus.getState().set('seaports', { state: 'loading' })
    fetch('/api/seaports', { signal: ac.signal })
      .then(r => { if (!r.ok) throw new Error('Unavailable'); return r.json() })
      .then(d => { if (!ac.signal.aborted) { layerData.seaports = d; useDataStatus.getState().set('seaports', { state: 'ready', count: d.features.length }) } })
      .catch(() => { if (!ac.signal.aborted) useDataStatus.getState().set('seaports', { state: 'error' }) })
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
    let inFlight = false
    const fetchAton = async () => {
      if (document.hidden || inFlight) return
      inFlight = true
      useDataStatus.getState().set('aton', { state: 'loading' })
      try {
        const r = await fetch('/api/ships/aton', { signal: ac.signal })
        if (!r.ok) throw new Error('Navigation aids unavailable')
        const data = await r.json()
        if (!ac.signal.aborted) { layerData.aton = data; useDataStatus.getState().set('aton', { state: 'ready', count: data.features.length }) }
      } catch { if (!ac.signal.aborted) useDataStatus.getState().set('aton', { state: 'error' }) } finally { inFlight = false }
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
      syncPipelinesLayer(map, layers.pipelines && availableTiles.includes('pipelines'))
      syncPowerGridLayer(map, layers.powerGrid && availableTiles.includes('power-grid'), layers.hvLines && availableTiles.includes('hv-lines'))
      syncBuildings3DLayer(map, layers.buildings3d, viewport.zoom)
      scheduler.invalidate()
    }
    if (map.isStyleLoaded()) onStyleLoad()
    map.on('style.load', onStyleLoad)
    return () => { map.off('style.load', onStyleLoad) }
  }, [layers.pipelines, layers.powerGrid, layers.hvLines, layers.buildings3d, viewport.zoom, availableTiles])

  // ── Event radius circles (sync from event store) ──
  useEffect(() => {
    layerData.eventRadii = useEventStore.getState().events.filter(e => e.active)
    return useEventStore.subscribe((s, previous) => {
      if (s.events !== previous.events) layerData.eventRadii = s.events.filter(e => e.active)
    })
  }, [])

  // ── Historical replay (sync positions from history store) ──
  useEffect(() => {
    return useHistoryStore.subscribe(s => {
      layerData.historyPositions = s.enabled ? s.positions : []
    })
  }, [])
}
