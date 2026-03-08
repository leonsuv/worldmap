import { IconLayer } from '@deck.gl/layers'

let ws: WebSocket | null = null
let snapshotLoaded = false

export function startShipsWs(store: Map<number, GeoJSON.Feature>) {
  if (ws) return

  // Load snapshot first
  if (!snapshotLoaded) {
    fetch('/api/ships/snapshot')
      .then(r => r.ok ? r.json() as Promise<GeoJSON.FeatureCollection> : null)
      .then(fc => {
        if (!fc) return
        for (const f of fc.features) {
          const mmsi = f.properties?.mmsi
          if (mmsi != null) store.set(mmsi, f)
        }
        snapshotLoaded = true
      })
  }

  // Open WebSocket
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${proto}//${location.host}/api/ships/ws`)

  ws.onmessage = (ev) => {
    try {
      const f: GeoJSON.Feature = JSON.parse(ev.data)
      const mmsi = f.properties?.mmsi
      if (mmsi != null) store.set(mmsi, f)
    } catch { /* ignore malformed */ }
  }

  ws.onclose = () => { ws = null }
  ws.onerror = () => { ws?.close(); ws = null }
}

export function stopShipsWs() {
  if (ws) {
    const sock = ws
    ws = null
    sock.onclose = null
    sock.onerror = null
    sock.onmessage = null
    if (sock.readyState === WebSocket.OPEN || sock.readyState === WebSocket.CONNECTING) {
      sock.close()
    }
  }
  snapshotLoaded = false
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

// Reusable typed-array buffer for ship positions
let shipPosBuf = new Float32Array(0)

function ensureShipPositions(features: GeoJSON.Feature[]): Float32Array {
  const needed = features.length * 2
  if (shipPosBuf.length < needed) shipPosBuf = new Float32Array(needed)
  for (let i = 0; i < features.length; i++) {
    const coords = (features[i].geometry as GeoJSON.Point).coordinates
    shipPosBuf[i * 2] = coords[0]
    shipPosBuf[i * 2 + 1] = coords[1]
  }
  return shipPosBuf.subarray(0, needed)
}

export function buildShipsLayer(data: GeoJSON.Feature[]): IconLayer {
  const positions = ensureShipPositions(data)
  return new IconLayer({
    id: 'ships',
    data,
    getPosition: (_d: GeoJSON.Feature, { index }: { index: number }) =>
      [positions[index * 2], positions[index * 2 + 1]] as [number, number],
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
