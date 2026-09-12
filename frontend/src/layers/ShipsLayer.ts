import { IconLayer } from '@deck.gl/layers'
import { useDataStatus } from '../store/dataStatus'

let cleanup: (() => void) | null = null

export function startShipsWs(store: Map<number, GeoJSON.Feature>, onUpdate: () => void) {
  stopShipsWs()
  const ac = new AbortController()
  let socket: WebSocket | null = null
  let reconnect: ReturnType<typeof setTimeout> | undefined
  let publish: ReturnType<typeof setTimeout> | undefined
  let attempt = 0
  let stopped = false
  const seen = new Map<number, number>()
  const streamed = new Set<number>()
  const notify = () => {
    if (publish || stopped) return
    publish = setTimeout(() => {
      publish = undefined
      if (!stopped) {
        onUpdate()
        useDataStatus.getState().set('ships', { state: socket?.readyState === WebSocket.OPEN ? 'ready' : 'loading', count: store.size })
      }
    }, 500)
  }
  const receive = (feature: GeoJSON.Feature, snapshot = false) => {
    const mmsi = Number(feature.properties?.mmsi)
    if (!Number.isFinite(mmsi) || feature.geometry?.type !== 'Point') return
    const [lon, lat] = feature.geometry.coordinates
    if (!Number.isFinite(lon) || !Number.isFinite(lat) || Math.abs(lat) > 90 || Math.abs(lon) > 180) return
    // A late snapshot must never overwrite a newer streamed position.
    if (snapshot && streamed.has(mmsi)) return
    store.set(mmsi, feature)
    const timestamp = Number(feature.properties?.timestamp)
    seen.set(mmsi, Number.isFinite(timestamp) && timestamp > 0 ? timestamp * 1000 : Date.now())
    if (!snapshot) streamed.add(mmsi)
    notify()
  }
  const connect = () => {
    if (stopped) return
    useDataStatus.getState().set('ships', { state: 'loading', count: store.size })
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
    const ws = new WebSocket(`${proto}//${location.host}/api/ships/ws`)
    socket = ws
    ws.onopen = () => {
      attempt = 0
      useDataStatus.getState().set('ships', { state: 'ready', count: store.size })
      // Resynchronize after reconnect; keep messages arriving during the fetch.
      streamed.clear()
      fetch('/api/ships/snapshot', { signal: ac.signal })
        .then(r => { if (!r.ok) throw new Error('Snapshot unavailable'); return r.json() as Promise<GeoJSON.FeatureCollection> })
        .then(fc => {
          if (stopped || socket !== ws) return
          const snapshotIds = new Set(fc.features.map(feature => Number(feature.properties?.mmsi)))
          for (const mmsi of store.keys()) if (!snapshotIds.has(mmsi) && !streamed.has(mmsi)) { store.delete(mmsi); seen.delete(mmsi) }
          for (const feature of fc.features) receive(feature, true)
          notify()
        })
        .catch(() => { if (!stopped && socket === ws) useDataStatus.getState().set('ships', { state: 'error', count: store.size, message: 'Snapshot unavailable' }) })
    }
    ws.onmessage = (event) => {
      if (stopped || socket !== ws) return
      try { receive(JSON.parse(event.data)) } catch { /* Ignore malformed messages. */ }
    }
    ws.onclose = () => {
      if (stopped || socket !== ws) return
      useDataStatus.getState().set('ships', { state: 'error', count: store.size, message: 'Reconnecting…' })
      reconnect = setTimeout(connect, Math.min(30_000, 1000 * 2 ** attempt++))
    }
    ws.onerror = () => ws.close()
  }
  const prune = setInterval(() => {
    let changed = false
    const cutoff = Date.now() - 30 * 60_000
    for (const [mmsi, timestamp] of seen) {
      if (timestamp < cutoff) { seen.delete(mmsi); store.delete(mmsi); changed = true }
    }
    if (changed) notify()
  }, 60_000)
  cleanup = () => {
    stopped = true
    ac.abort()
    clearTimeout(reconnect)
    clearTimeout(publish)
    clearInterval(prune)
    if (socket) {
      socket.onclose = null
      socket.onerror = null
      socket.onmessage = null
      socket.onopen = null
      socket.close()
    }
  }
  connect()
}

export function stopShipsWs() {
  cleanup?.()
  cleanup = null
}

/* ─── AIS ship-type → category mapping (MarineTraffic style) ─── */

type ShipCategory = 'cargo' | 'tanker' | 'passenger' | 'fishing' | 'highspeed'
  | 'tug' | 'pleasure' | 'military' | 'sailing' | 'unknown'

function shipCategory(type: number | null | undefined): ShipCategory {
  if (type == null) return 'unknown'
  if (type >= 70 && type <= 79) return 'cargo'
  if (type >= 80 && type <= 89) return 'tanker'
  if (type >= 60 && type <= 69) return 'passenger'
  if (type === 30) return 'fishing'
  if (type >= 40 && type <= 49) return 'highspeed'
  if (type === 31 || type === 32) return 'tug'        // towing
  if (type === 52) return 'tug'                         // tug
  if (type === 50) return 'tug'                         // pilot vessel
  if (type === 53) return 'tug'                         // port tender
  if (type === 51) return 'military'                    // SAR
  if (type === 35) return 'military'                    // military ops
  if (type === 55) return 'military'                    // law enforcement
  if (type === 36 || type === 37) return 'pleasure'     // sailing / pleasure
  return 'unknown'
}

/* MarineTraffic-like colors per category */
const CATEGORY_COLORS: Record<ShipCategory, [number, number, number, number]> = {
  cargo:     [76, 175, 80, 230],     // green
  tanker:    [229, 57, 53, 230],     // red
  passenger: [33, 150, 243, 230],    // blue
  fishing:   [255, 152, 0, 230],     // orange
  highspeed: [255, 235, 59, 230],    // yellow
  tug:       [0, 188, 212, 230],     // cyan
  pleasure:  [171, 71, 188, 230],    // purple
  military:  [120, 144, 156, 230],   // blue-grey
  sailing:   [206, 147, 216, 230],   // light purple
  unknown:   [0, 160, 255, 220],     // blue (default until type known)
}

/* Small vessels get a dot (circle); others get the directional arrow */
const SMALL_CATEGORIES = new Set<ShipCategory>(['tug', 'fishing', 'pleasure', 'sailing'])

function iconForCategory(cat: ShipCategory): string {
  return SMALL_CATEGORIES.has(cat) ? 'dot' : 'arrow'
}

function sizeForCategory(cat: ShipCategory): number {
  return SMALL_CATEGORIES.has(cat) ? 10 : 18
}

/* ─── Icon atlas with both shapes ─── */

const ICON_SIZE = 64
const SHIP_ATLAS = createShipAtlas()
const SHIP_MAPPING: Record<string, { x: number; y: number; width: number; height: number; anchorY: number; mask: boolean }> = {
  arrow: { x: 0,         y: 0, width: ICON_SIZE, height: ICON_SIZE, anchorY: 32, mask: true },
  dot:   { x: ICON_SIZE, y: 0, width: ICON_SIZE, height: ICON_SIZE, anchorY: 32, mask: true },
}

export function buildShipsLayer(data: GeoJSON.Feature[]): IconLayer {
  return new IconLayer({
    id: 'ships',
    data,
    getPosition: (d: GeoJSON.Feature) => (d.geometry as GeoJSON.Point).coordinates as [number, number],
    getIcon: (d: GeoJSON.Feature) => iconForCategory(shipCategory(d.properties?.ship_type)),
    getSize: (d: GeoJSON.Feature) => sizeForCategory(shipCategory(d.properties?.ship_type)),
    getAngle: (d: GeoJSON.Feature) => {
      const cat = shipCategory(d.properties?.ship_type)
      if (SMALL_CATEGORIES.has(cat)) return 0
      // AIS sentinel: heading 511 = unavailable, COG 360 = unavailable
      const h = d.properties?.heading
      const c = d.properties?.course
      const heading = (typeof h === 'number' && h < 360) ? h : null
      const course = (typeof c === 'number' && c < 360) ? c : null
      return -(heading ?? course ?? 0)
    },
    getColor: (d: GeoJSON.Feature) => CATEGORY_COLORS[shipCategory(d.properties?.ship_type)],
    iconAtlas: SHIP_ATLAS,
    iconMapping: SHIP_MAPPING,
    sizeScale: 1,
    pickable: true,
  })
}

function createShipAtlas(): string {
  const c = document.createElement('canvas')
  c.width = ICON_SIZE * 2; c.height = ICON_SIZE
  const ctx = c.getContext('2d')!
  ctx.fillStyle = 'white'

  // ── Arrow (left half) — directional ship shape pointing up ──
  ctx.beginPath()
  ctx.moveTo(32, 4)
  ctx.lineTo(48, 52)
  ctx.lineTo(32, 44)
  ctx.lineTo(16, 52)
  ctx.closePath()
  ctx.fill()

  // ── Dot (right half) — small circle for minor vessels ──
  ctx.beginPath()
  ctx.arc(ICON_SIZE + 32, 32, 14, 0, Math.PI * 2)
  ctx.fill()

  return c.toDataURL()
}
