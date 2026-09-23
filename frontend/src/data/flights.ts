import { api, isAbort, rowsToObjects } from '../lib/api'
import { isNum } from '../lib/format'
import { clamp } from '../lib/scale'
import { setFeed } from '../store/status'
import { createSignal, requestRedraw } from '../map/scheduler'

export interface Flight {
  icao24: string
  callsign: string | null
  country: string | null
  lon: number
  lat: number
  altitude: number | null
  velocity: number | null
  track: number | null
  vertical_rate: number | null
  on_ground: boolean
  category: number
  time_position: number | null
  squawk: string | null
}

export interface FlightResponse {
  time: number | null
  fetched_at: number
  stale: boolean
  refresh_in: number
  authenticated: boolean
  fields: string[]
  rows: unknown[][]
}

export function parseFlights(data: FlightResponse): Flight[] {
  return rowsToObjects<Flight>(data.fields, data.rows).filter(
    f => typeof f.icao24 === 'string' && isNum(f.lon) && isNum(f.lat) && Math.abs(f.lat) <= 90 && Math.abs(f.lon) <= 180,
  )
}

const MAX_EXTRAPOLATION_S = 240

/** Dead-reckoned position: move along the track at the reported speed. */
export function extrapolate(f: Flight, nowSec: number, fallbackTime: number): [number, number] {
  if (f.on_ground || !isNum(f.velocity) || !isNum(f.track) || f.velocity < 5) return [f.lon, f.lat]
  const t0 = isNum(f.time_position) ? f.time_position : fallbackTime
  const dt = clamp(nowSec - t0, 0, MAX_EXTRAPOLATION_S)
  const distance = f.velocity * dt
  const rad = (f.track * Math.PI) / 180
  const lat = clamp(f.lat + (distance * Math.cos(rad)) / 111_320, -89.9, 89.9)
  const lon = f.lon + (distance * Math.sin(rad)) / (111_320 * Math.max(0.05, Math.cos((f.lat * Math.PI) / 180)))
  return [((((lon + 180) % 360) + 360) % 360) - 180, lat]
}

let list: Flight[] = []
let byId = new Map<string, Flight>()
let dataTime = 0
let running = false
let timer: ReturnType<typeof setTimeout> | undefined
let controller: AbortController | null = null
let lastFetch = 0
let nextDelay = 30_000

// Last known state of aircraft shown in a details panel, kept after they drop out.
const followed = new Map<string, Flight | undefined>()

export const flightsSignal = createSignal()
export const getFlights = () => list
export const getFlight = (icao24: string) => byId.get(icao24)
export const flightsDataTime = () => dataTime

/** Keep the last known state of an aircraft while a panel shows it. Returns a release function. */
export function followFlight(icao24: string): () => void {
  followed.set(icao24, byId.get(icao24) ?? followed.get(icao24))
  return () => {
    followed.delete(icao24)
  }
}

export function getFlightState(icao24: string): { flight: Flight | undefined; live: boolean } {
  const live = byId.get(icao24)
  return { flight: live ?? followed.get(icao24), live: !!live }
}

function publish() {
  flightsSignal.emit()
  requestRedraw()
}

function schedule(ms: number) {
  clearTimeout(timer)
  nextDelay = ms
  if (running) timer = setTimeout(poll, ms)
}

async function poll() {
  if (!running) return
  if (document.hidden) return schedule(15_000)
  controller?.abort()
  const ac = new AbortController()
  controller = ac
  if (!list.length) setFeed('flights', { state: 'loading' })
  try {
    const data = await api<FlightResponse>('/api/flights', { signal: ac.signal })
    if (!running || ac.signal.aborted) return
    list = parseFlights(data)
    byId = new Map(list.map(f => [f.icao24, f]))
    for (const id of followed.keys()) if (byId.has(id)) followed.set(id, byId.get(id))
    dataTime = data.time ?? data.fetched_at
    lastFetch = Date.now()
    setFeed('flights', {
      state: 'ready',
      count: list.length,
      updatedAt: data.fetched_at,
      message: data.stale ? 'Live feed paused — showing the last positions' : undefined,
    })
    publish()
    schedule(clamp(data.refresh_in, 10, 600) * 1000 + 500)
  } catch (error) {
    if (isAbort(error) || !running) return
    setFeed('flights', { state: 'error', count: list.length, message: error instanceof Error ? error.message : 'Flights unavailable' })
    schedule(60_000)
  }
}

function onVisibility() {
  if (!document.hidden && running && Date.now() - lastFetch > nextDelay) void poll()
}

export function startFlights() {
  if (running) return
  running = true
  document.addEventListener('visibilitychange', onVisibility)
  void poll()
}

export function stopFlights() {
  running = false
  clearTimeout(timer)
  controller?.abort()
  document.removeEventListener('visibilitychange', onVisibility)
  list = []
  byId = new Map()
  setFeed('flights', null)
  publish()
}
