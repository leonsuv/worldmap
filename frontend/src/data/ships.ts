import { rowsToObjects } from '../lib/api'
import { isNum } from '../lib/format'
import { shipCategory, type ShipCategory } from '../lib/ais'
import { setFeed } from '../store/status'
import { createSignal, requestRedraw } from '../map/scheduler'

export interface Ship {
  mmsi: number
  lon: number
  lat: number
  course: number | null
  speed: number | null
  heading: number | null
  ship_type: number | null
  nav_status: number | null
  timestamp: number
  name: string
  sar: boolean
  category: ShipCategory
}

export interface ShipMessage {
  type: 'snapshot' | 'ships'
  ts: number
  fields: string[]
  rows: unknown[][]
  removed?: number[]
}

type RawShip = Omit<Ship, 'sar' | 'category'> & { kind: number }

export function parseShips(message: ShipMessage): Ship[] {
  return rowsToObjects<RawShip>(message.fields, message.rows)
    .filter(s => isNum(s.mmsi) && isNum(s.lon) && isNum(s.lat) && Math.abs(s.lat) <= 90 && Math.abs(s.lon) <= 180)
    .map(({ kind, ...s }) => ({ ...s, name: s.name ?? '', sar: kind === 1, category: shipCategory(s.ship_type, kind === 1) }))
}

const MAX_AGE_S = 30 * 60

/** Apply a WebSocket message to the vessel map. Returns true when anything changed. */
export function applyMessage(store: Map<number, Ship>, message: ShipMessage): boolean {
  const ships = parseShips(message)
  if (message.type === 'snapshot') {
    store.clear()
    for (const s of ships) store.set(s.mmsi, s)
    return true
  }
  let changed = false
  for (const s of ships) {
    const existing = store.get(s.mmsi)
    // Never let an older report overwrite a newer one.
    if (existing && existing.timestamp > s.timestamp) continue
    store.set(s.mmsi, s)
    changed = true
  }
  for (const id of message.removed ?? []) changed = store.delete(id) || changed
  return changed
}

const store = new Map<number, Ship>()
let list: Ship[] = []
let socket: WebSocket | null = null
let running = false
let attempt = 0
let reconnectTimer: ReturnType<typeof setTimeout> | undefined
let publishTimer: ReturnType<typeof setTimeout> | undefined

export const shipsSignal = createSignal()
export const getShips = () => list
export const getShip = (mmsi: number) => store.get(mmsi)

function publishSoon() {
  if (publishTimer) return
  publishTimer = setTimeout(() => {
    publishTimer = undefined
    const cutoff = Date.now() / 1000 - MAX_AGE_S
    for (const [id, s] of store) if (s.timestamp < cutoff) store.delete(id)
    list = Array.from(store.values())
    setFeed('ships', { state: socket?.readyState === WebSocket.OPEN ? 'ready' : 'loading', count: list.length, updatedAt: Date.now() / 1000 })
    shipsSignal.emit()
    requestRedraw()
  }, 700)
}

function connect() {
  if (!running) return
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(`${proto}//${location.host}/api/ships/ws`)
  socket = ws
  if (!store.size) setFeed('ships', { state: 'loading' })
  ws.onopen = () => {
    attempt = 0
  }
  ws.onmessage = event => {
    if (socket !== ws) return
    try {
      if (applyMessage(store, JSON.parse(event.data) as ShipMessage)) publishSoon()
      else if (!list.length) setFeed('ships', { state: 'ready', count: 0, message: 'Waiting for vessel reports…' })
    } catch {
      /* ignore malformed frames */
    }
  }
  ws.onclose = () => {
    if (socket !== ws || !running) return
    setFeed('ships', { state: 'error', count: list.length, message: 'Connection lost — reconnecting…' })
    reconnectTimer = setTimeout(connect, Math.min(30_000, 1000 * 2 ** attempt++))
  }
  ws.onerror = () => ws.close()
}

export function startShips() {
  if (running) return
  running = true
  connect()
}

export function stopShips() {
  running = false
  clearTimeout(reconnectTimer)
  clearTimeout(publishTimer)
  publishTimer = undefined
  if (socket) {
    socket.onclose = null
    socket.onmessage = null
    socket.close()
    socket = null
  }
  store.clear()
  list = []
  setFeed('ships', null)
  shipsSignal.emit()
  requestRedraw()
}
