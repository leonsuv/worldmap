import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
vi.mock('@deck.gl/layers', () => ({ IconLayer: class {} }))
class FakeSocket {
  static OPEN = 1
  static instances: FakeSocket[] = []
  readyState = 0
  onopen: (() => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  onmessage: ((event: { data: string }) => void) | null = null
  constructor() { FakeSocket.instances.push(this) }
  open() { this.readyState = 1; this.onopen?.() }
  close() { this.readyState = 3; this.onclose?.() }
}
const feature = (mmsi: number, lon = 1): GeoJSON.Feature => ({ type: 'Feature', geometry: { type: 'Point', coordinates: [lon, 50] }, properties: { mmsi } })
let start: typeof import('../src/layers/ShipsLayer').startShipsWs
let stop: typeof import('../src/layers/ShipsLayer').stopShipsWs
beforeEach(async () => {
  vi.useFakeTimers()
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
  vi.stubGlobal('location', { protocol: 'http:', host: 'localhost' })
  vi.stubGlobal('document', { createElement: () => ({ width: 0, height: 0, getContext: () => ({ beginPath() {}, moveTo() {}, lineTo() {}, closePath() {}, fill() {}, arc() {} }), toDataURL: () => '' }) })
  const module = await import('../src/layers/ShipsLayer')
  start = module.startShipsWs; stop = module.stopShipsWs
})
afterEach(() => { stop(); vi.useRealTimers(); vi.unstubAllGlobals() })
describe('ship connection lifecycle', () => {
  it('reconnects after disconnection but never after being stopped', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ features: [] }) }))
    start(new Map(), vi.fn())
    FakeSocket.instances[0].close()
    await vi.advanceTimersByTimeAsync(1000)
    expect(FakeSocket.instances).toHaveLength(2)
    stop()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(FakeSocket.instances).toHaveLength(2)
  })
  it('does not replace a newer streamed position with a late snapshot', async () => {
    let resolve!: (value: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(r => { resolve = r })))
    const store = new Map<number, GeoJSON.Feature>()
    const publish = vi.fn()
    start(store, publish)
    const socket = FakeSocket.instances[0]
    socket.open()
    socket.onmessage?.({ data: JSON.stringify(feature(123, 20)) })
    resolve({ ok: true, json: async () => ({ features: [feature(123, 10)] }) })
    await vi.advanceTimersByTimeAsync(500)
    expect((store.get(123)?.geometry as GeoJSON.Point).coordinates[0]).toBe(20)
    expect(publish).toHaveBeenCalledTimes(1)
  })
  it('ignores a snapshot response arriving after disable', async () => {
    let resolve!: (value: unknown) => void
    vi.stubGlobal('fetch', vi.fn(() => new Promise(r => { resolve = r })))
    const store = new Map<number, GeoJSON.Feature>()
    start(store, vi.fn()); FakeSocket.instances[0].open(); stop()
    resolve({ ok: true, json: async () => ({ features: [feature(123)] }) })
    await vi.advanceTimersByTimeAsync(1000)
    expect(store.size).toBe(0)
  })
})
