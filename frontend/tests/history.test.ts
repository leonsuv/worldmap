import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useHistoryStore } from '../src/store/history'
const response = (value: unknown) => ({ ok: true, json: async () => value })

describe('historical replay', () => {
  beforeEach(() => useHistoryStore.setState({ enabled: false, timestamps: [], positions: [], currentTs: null, loading: false, error: null }))
  afterEach(() => { if (useHistoryStore.getState().enabled) useHistoryStore.getState().toggle(); vi.unstubAllGlobals() })
  it('loads the newest actual snapshot when opened', async () => {
    const fetcher = vi.fn().mockResolvedValueOnce(response({ timestamps: [600, 300] })).mockResolvedValueOnce(response([{ mmsi: 1, speed: null }]))
    vi.stubGlobal('fetch', fetcher)
    useHistoryStore.getState().toggle()
    await vi.waitFor(() => expect(useHistoryStore.getState().loading).toBe(false))
    expect(useHistoryStore.getState().currentTs).toBe(600)
    expect(fetcher.mock.calls[1][0]).toBe('/api/history/ships?from=600&to=600')
    expect(useHistoryStore.getState().positions).toHaveLength(1)
  })
  it('ignores a slower old request after a newer seek', async () => {
    let resolveOld!: (value: unknown) => void
    vi.stubGlobal('fetch', vi.fn().mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve })).mockResolvedValueOnce(response([{ mmsi: 2 }])))
    useHistoryStore.setState({ enabled: true })
    const old = useHistoryStore.getState().seek(300)
    await useHistoryStore.getState().seek(600)
    resolveOld(response([{ mmsi: 1 }]))
    await old
    expect(useHistoryStore.getState().positions[0].mmsi).toBe(2)
    expect(useHistoryStore.getState().currentTs).toBe(600)
  })
  it('does not restore historical positions after closing', async () => {
    let resolve!: (value: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(r => { resolve = r })))
    useHistoryStore.setState({ enabled: true })
    const pending = useHistoryStore.getState().seek(300)
    useHistoryStore.getState().toggle()
    resolve(response([{ mmsi: 1 }]))
    await pending
    expect(useHistoryStore.getState().positions).toEqual([])
    expect(useHistoryStore.getState().loading).toBe(false)
  })
  it('reports a network error without an unhandled rejection', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')))
    useHistoryStore.setState({ enabled: true })
    await useHistoryStore.getState().seek(300)
    expect(useHistoryStore.getState().error).toBeTruthy()
    expect(useHistoryStore.getState().loading).toBe(false)
  })
})
