import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { availability, parseCapabilities, type Capabilities } from '../src/store/status'
import { DEFAULT_LAYERS, sanitize } from '../src/store/layers'
import { useEvents, type MapEvent } from '../src/store/events'
import { useAlerts } from '../src/store/alerts'
import { useWatchlist } from '../src/store/watchlist'
import { useNotices } from '../src/store/notice'
import { useHistory } from '../src/store/history'
import { ApiError, api } from '../src/lib/api'

const json = (status: number, body: unknown) => ({ ok: status < 400, status, json: async () => body })

const caps: Capabilities = parseCapabilities({
  version: '0.2.0',
  tiles: ['hv-lines'],
  tile_sources: [{ id: 'hv-lines', minzoom: 2, maxzoom: 13 }],
  ships_configured: false,
  traffic_configured: true,
  datasets: { airports: 5000, seaports: 0, reactors: 190 },
})

describe('source availability', () => {
  it('rejects malformed status responses', () => {
    for (const bad of [null, {}, { tiles: [1], ships_configured: true, traffic_configured: true, datasets: {} }]) {
      expect(() => parseCapabilities(bad)).toThrow()
    }
  })
  it('explains exactly what is missing', () => {
    expect(availability('flights', caps, false)).toEqual({ ok: true })
    expect(availability('traffic', caps, false).ok).toBe(true)
    expect(availability('hvLines', caps, false).ok).toBe(true)
    const ships = availability('ships', caps, false)
    expect(ships.ok).toBe(false)
    expect(ships.fix).toContain('AISSTREAM_API_KEY')
    expect(availability('pipelines', caps, false).fix).toBe('Run: python scripts/build_tiles.py pipelines')
    expect(availability('seaports', caps, false).fix).toBe('Run: python scripts/ingest.py seaports')
    expect(availability('airports', caps, false).ok).toBe(true)
  })
  it('does not enable keyed layers before the server answered', () => {
    expect(availability('ships', null, false)).toEqual({ ok: false, reason: 'Checking setup…' })
    expect(availability('ships', null, true).reason).toBe('Server not reachable')
    expect(availability('weather', null, true).ok).toBe(true)
  })
})

describe('persisted layer selection', () => {
  it('keeps known flags and drops removed layers from older versions', () => {
    const legacy = { flights: true, ships: true, solar: true, windTurbines: true, powerGrid: 'yes' }
    const clean = sanitize(legacy)
    expect(clean.flights).toBe(true)
    expect(clean.ships).toBe(true)
    expect(clean.powerGrid).toBe(DEFAULT_LAYERS.powerGrid)
    expect('solar' in clean).toBe(false)
    expect(sanitize(null)).toEqual(DEFAULT_LAYERS)
  })
})

describe('mutations and failures', () => {
  beforeEach(() => useNotices.setState({ notices: [] }))
  afterEach(() => vi.unstubAllGlobals())

  it('surfaces the server message from JSON errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json(400, { error: 'radius must be between 0 and 5,000 km' })))
    await expect(api('/x')).rejects.toEqual(new ApiError('radius must be between 0 and 5,000 km', 400))
  })

  it('reports an unreachable server clearly', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))
    await expect(api('/x')).rejects.toMatchObject({ status: 0 })
  })

  it('keeps an event when deletion fails and shows why', async () => {
    const events = [{ id: 1, name: 'Storm', active: true }] as MapEvent[]
    useEvents.setState({ events, selectedId: 1 })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json(500, { error: 'Internal server error' })))
    expect(await useEvents.getState().remove(1)).toBe(false)
    expect(useEvents.getState().events).toBe(events)
    expect(useNotices.getState().notices[0].message).toBe('Internal server error')
  })

  it('does not acknowledge alerts when the request fails', async () => {
    useAlerts.setState({ alerts: [{ id: 1, event_id: null, watch_id: null, title: 't', message: '', severity: 'warning', acknowledged: false, created_at: 1, distance_km: null }], count: 1 })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json(503, {})))
    expect(await useAlerts.getState().ack(1)).toBe(false)
    expect(useAlerts.getState().alerts[0].acknowledged).toBe(false)
    expect(useAlerts.getState().count).toBe(1)
  })

  it('sends watchlist parameters as an object', async () => {
    const fetcher = vi.fn().mockResolvedValueOnce(json(201, {})).mockResolvedValueOnce(json(200, [])).mockResolvedValueOnce(json(200, { count: 0 }))
    vi.stubGlobal('fetch', fetcher)
    expect(await useWatchlist.getState().add('vessel', 'Ship', { mmsi: 211234567 })).toBe(true)
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ wtype: 'vessel', name: 'Ship', params: { mmsi: 211234567 } })
  })

  it('ignores affected-asset responses for an event that is no longer selected', async () => {
    let resolve!: (v: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(r => (resolve = r))))
    useEvents.getState().select(1)
    useEvents.setState({ selectedId: 2, affected: null })
    resolve(json(200, { total: 99 }))
    await new Promise(r => setTimeout(r, 0))
    expect(useEvents.getState().affected).toBeNull()
  })
})

describe('historical replay', () => {
  beforeEach(() => useHistory.getState().close())
  afterEach(() => {
    useHistory.getState().close()
    vi.unstubAllGlobals()
  })

  const snapshot = (mmsi: number) => json(200, { fields: ['mmsi', 'lon', 'lat', 'course', 'speed', 'heading', 'ship_type', 'name'], rows: [[mmsi, 1, 2, null, null, null, null, 'X']] })

  it('opens on the newest recorded snapshot', async () => {
    const fetcher = vi.fn().mockResolvedValueOnce(json(200, { timestamps: [600, 300] })).mockResolvedValueOnce(snapshot(7))
    vi.stubGlobal('fetch', fetcher)
    await useHistory.getState().open()
    expect(fetcher.mock.calls[1][0]).toBe('/api/history/ships?at=600')
    expect(useHistory.getState().index).toBe(1)
    expect(useHistory.getState().positions[0].mmsi).toBe(7)
  })

  it('lets a newer seek win over a slower older one', async () => {
    let resolveOld!: (v: unknown) => void
    vi.stubGlobal('fetch', vi.fn().mockImplementationOnce(() => new Promise(r => (resolveOld = r))).mockResolvedValueOnce(snapshot(2)))
    useHistory.setState({ enabled: true, timestamps: [300, 600] })
    const old = useHistory.getState().seek(0)
    await useHistory.getState().seek(1)
    resolveOld(snapshot(1))
    await old
    expect(useHistory.getState().positions[0].mmsi).toBe(2)
  })

  it('does not restore positions after closing', async () => {
    let resolve!: (v: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(r => (resolve = r))))
    useHistory.setState({ enabled: true, timestamps: [300] })
    const pending = useHistory.getState().seek(0)
    useHistory.getState().close()
    resolve(snapshot(1))
    await pending
    expect(useHistory.getState().positions).toEqual([])
  })
})
