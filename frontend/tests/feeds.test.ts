import { describe, expect, it } from 'vitest'
import { extrapolate, parseFlights, type Flight, type FlightResponse } from '../src/data/flights'
import { applyMessage, parseShips, type Ship, type ShipMessage } from '../src/data/ships'
import { bboxParam } from '../src/data/weather'
import { rowsToObjects } from '../src/lib/api'

const FLIGHT_FIELDS = ['icao24', 'callsign', 'country', 'lon', 'lat', 'altitude', 'velocity', 'track', 'vertical_rate', 'on_ground', 'category', 'time_position', 'squawk']

describe('flights', () => {
  const response = (rows: unknown[][]): FlightResponse => ({ time: 1000, fetched_at: 1000, stale: false, refresh_in: 30, authenticated: false, fields: FLIGHT_FIELDS, rows })

  it('parses compact rows and drops invalid positions', () => {
    const flights = parseFlights(response([
      ['3c6444', 'DLH1', 'Germany', 8.5, 50, 11000, 230, 90, 0, false, 4, 995, '1000'],
      ['bad', null, null, null, 50, null, null, null, null, false, 0, null, null],
      ['bad2', null, null, 200, 50, null, null, null, null, false, 0, null, null],
    ]))
    expect(flights).toHaveLength(1)
    expect(flights[0]).toMatchObject({ icao24: '3c6444', callsign: 'DLH1', altitude: 11000, on_ground: false })
  })

  const flight = (overrides: Partial<Flight> = {}): Flight => ({
    icao24: 'abc123', callsign: null, country: null, lon: 0, lat: 0, altitude: 10000, velocity: 250, track: 90,
    vertical_rate: 0, on_ground: false, category: 0, time_position: 1000, squawk: null, ...overrides,
  })

  it('dead-reckons along the track', () => {
    const [lon, lat] = extrapolate(flight(), 1060, 1000)
    expect(lon).toBeCloseTo((250 * 60) / 111_320, 5)
    expect(lat).toBeCloseTo(0, 6)
    const [, north] = extrapolate(flight({ track: 0 }), 1060, 1000)
    expect(north).toBeGreaterThan(0.13)
  })

  it('never moves aircraft on the ground and caps stale extrapolation', () => {
    expect(extrapolate(flight({ on_ground: true }), 2000, 1000)).toEqual([0, 0])
    expect(extrapolate(flight({ velocity: null }), 2000, 1000)).toEqual([0, 0])
    const capped = extrapolate(flight(), 100_000, 1000)
    const fourMinutes = extrapolate(flight(), 1240, 1000)
    expect(capped).toEqual(fourMinutes)
  })

  it('wraps across the antimeridian', () => {
    const [lon] = extrapolate(flight({ lon: 179.99, track: 90 }), 1200, 1000)
    expect(lon).toBeLessThan(-179)
  })
})

const SHIP_FIELDS = ['mmsi', 'lon', 'lat', 'course', 'speed', 'heading', 'ship_type', 'nav_status', 'timestamp', 'name', 'kind']
const row = (mmsi: number, lon: number, ts: number, extra: Partial<Record<string, unknown>> = {}) =>
  SHIP_FIELDS.map(f => (f === 'mmsi' ? mmsi : f === 'lon' ? lon : f === 'lat' ? 54 : f === 'timestamp' ? ts : f === 'kind' ? 0 : f === 'name' ? `S${mmsi}` : extra[f] ?? null))

describe('vessel stream', () => {
  const message = (type: ShipMessage['type'], rows: unknown[][], removed: number[] = []): ShipMessage => ({ type, ts: 0, fields: SHIP_FIELDS, rows, removed })

  it('classifies rows, including SAR aircraft', () => {
    const [ship, sar] = parseShips(message('ships', [row(1, 10, 5, { ship_type: 80 }), [...row(2, 11, 5).slice(0, 10), 1]]))
    expect(ship.category).toBe('tanker')
    expect(sar.sar).toBe(true)
    expect(sar.category).toBe('sar')
  })

  it('replaces everything on a snapshot', () => {
    const store = new Map<number, Ship>()
    applyMessage(store, message('ships', [row(1, 10, 5), row(2, 11, 5)]))
    applyMessage(store, message('snapshot', [row(3, 12, 6)]))
    expect([...store.keys()]).toEqual([3])
  })

  it('keeps the newer report and applies removals', () => {
    const store = new Map<number, Ship>()
    applyMessage(store, message('ships', [row(1, 10, 100), row(2, 11, 100)]))
    expect(applyMessage(store, message('ships', [row(1, 99, 50)]))).toBe(false)
    expect(store.get(1)?.lon).toBe(10)
    expect(applyMessage(store, message('ships', [row(1, 12, 150)], [2]))).toBe(true)
    expect(store.get(1)?.lon).toBe(12)
    expect(store.has(2)).toBe(false)
  })

  it('ignores invalid coordinates', () => {
    const store = new Map<number, Ship>()
    applyMessage(store, message('ships', [row(1, 500, 1)]))
    expect(store.size).toBe(0)
  })
})

describe('helpers', () => {
  it('rounds and clamps the weather bbox so identical views share a request', () => {
    expect(bboxParam([5.123456, -89, 15.987, 89])).toBe('5.12,-85,15.99,85')
  })
  it('maps rows to objects', () => {
    expect(rowsToObjects<{ a: number; b: string | null }>(['a', 'b'], [[1], [2, 'x']])).toEqual([
      { a: 1, b: null },
      { a: 2, b: 'x' },
    ])
  })
})
