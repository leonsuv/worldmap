import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { parseCapabilities, sourceAvailable, useSourceAvailability } from '../src/store/sourceAvailability'
const configured = { tiles: ['pipelines', 'power-grid', 'hv-lines'], traffic_configured: true, ships_configured: true }
beforeEach(() => useSourceAvailability.setState({ data: null, checking: false, error: false }))
afterEach(() => vi.unstubAllGlobals())
describe('source availability', () => {
  it('prevents network-dependent layers from starting before setup is known', () => {
    for (const key of ['traffic', 'ships', 'aton', 'pipelines', 'powerGrid', 'hvLines']) expect(sourceAvailable(key, null)).toBe(false)
    expect(sourceAvailable('reactors', null)).toBe(true)
  })
  it('maps tile IDs and credentials to the correct layers', () => {
    for (const key of ['traffic', 'ships', 'aton', 'pipelines', 'powerGrid', 'hvLines']) expect(sourceAvailable(key, configured)).toBe(true)
    expect(sourceAvailable('powerGrid', { ...configured, tiles: ['pipelines'] })).toBe(false)
    expect(sourceAvailable('traffic', { ...configured, traffic_configured: false })).toBe(false)
    expect(sourceAvailable('aton', { ...configured, ships_configured: false })).toBe(false)
  })
  it('rejects malformed status responses', () => {
    for (const data of [null, {}, { ...configured, tiles: [1] }, { ...configured, ships_configured: 'yes' }]) expect(() => parseCapabilities(data)).toThrow()
  })
  it('rechecks configuration after installation and retains known data on outage', async () => {
    const fetch = vi.fn().mockResolvedValueOnce({ ok: true, json: async () => ({ ...configured, tiles: [] }) }).mockResolvedValueOnce({ ok: true, json: async () => configured }).mockRejectedValueOnce(new Error('offline'))
    vi.stubGlobal('fetch', fetch)
    await useSourceAvailability.getState().refresh()
    expect(sourceAvailable('pipelines', useSourceAvailability.getState().data)).toBe(false)
    await useSourceAvailability.getState().refresh()
    expect(sourceAvailable('pipelines', useSourceAvailability.getState().data)).toBe(true)
    await useSourceAvailability.getState().refresh()
    expect(useSourceAvailability.getState().error).toBe(true)
    expect(useSourceAvailability.getState().data).toEqual(configured)
    expect(useSourceAvailability.getState().checking).toBe(false)
  })
  it('deduplicates simultaneous source checks', async () => {
    let resolve!: (value: unknown) => void
    const fetch = vi.fn(() => new Promise(r => { resolve = r }))
    vi.stubGlobal('fetch', fetch)
    const first = useSourceAvailability.getState().refresh()
    await useSourceAvailability.getState().refresh()
    expect(fetch).toHaveBeenCalledTimes(1)
    resolve({ ok: true, json: async () => configured })
    await first
  })
})
