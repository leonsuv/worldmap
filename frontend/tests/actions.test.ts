import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useEventStore, type EventItem } from '../src/store/events'
import { useAlertStore } from '../src/store/alerts'
import { useWatchlistStore } from '../src/store/watchlist'
import { useNoticeStore } from '../src/store/notice'

beforeEach(() => { useNoticeStore.getState().dismiss(); vi.stubGlobal('document', { hidden: false }) })
afterEach(() => vi.unstubAllGlobals())
describe('failed mutations preserve local data', () => {
  it('keeps an event when deletion fails', async () => {
    const events = [{ id: 1, name: 'Test', active: true }] as EventItem[]
    useEventStore.setState({ events, selectedId: 1 })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 500 }))
    await useEventStore.getState().remove(1)
    expect(useEventStore.getState().events).toBe(events)
    expect(useEventStore.getState().selectedId).toBe(1)
    expect(useNoticeStore.getState().message).toContain('500')
  })
  it('does not acknowledge alerts on an HTTP error', async () => {
    useAlertStore.setState({ alerts: [{ id: 1, event_id: null, title: 'Test', message: '', severity: 'warning', acknowledged: false, created_at: 1 }], count: 1 })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 503 }))
    await useAlertStore.getState().ack(1)
    expect(useAlertStore.getState().alerts[0].acknowledged).toBe(false)
    expect(useAlertStore.getState().count).toBe(1)
  })
  it('sends watchlist parameters as an object rather than double-encoded JSON', async () => {
    const fetcher = vi.fn().mockResolvedValueOnce({ ok: true, status: 201, json: async () => ({}) }).mockResolvedValueOnce({ ok: true, status: 200, json: async () => [] })
    vi.stubGlobal('fetch', fetcher)
    await useWatchlistStore.getState().add('vessel', 'Test', { mmsi: 123 })
    const body = JSON.parse(fetcher.mock.calls[0][1].body)
    expect(body.params).toEqual({ mmsi: 123 })
  })
  it('ignores stale affected-assets responses after changing selection', async () => {
    let resolve!: (response: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(r => { resolve = r })))
    useEventStore.setState({ selectedId: 1 })
    const pending = useEventStore.getState().fetchAffected(1)
    useEventStore.setState({ selectedId: 2, affected: null })
    resolve({ ok: true, status: 200, json: async () => ({ total: 99 }) })
    await pending
    expect(useEventStore.getState().affected).toBeNull()
  })
})
