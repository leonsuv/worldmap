import { create } from 'zustand'
import { LAYER_BY_KEY, type LayerKey, type Requirement } from '../catalog'

export interface Capabilities {
  version: string
  tiles: string[]
  tile_sources: { id: string; minzoom: number; maxzoom: number; attribution?: string | null }[]
  ships_configured: boolean
  traffic_configured: boolean
  flights_authenticated: boolean
  flights_refresh_secs: number
  ais: { connected: boolean; last_message_at: number | null; error: string | null; vessels: number; aids: number }
  datasets: { airports: number; seaports: number; reactors: number }
}

export function parseCapabilities(data: unknown): Capabilities {
  const v = data as Partial<Capabilities> | null
  if (
    !v || !Array.isArray(v.tiles) || !v.tiles.every(t => typeof t === 'string') ||
    typeof v.ships_configured !== 'boolean' || typeof v.traffic_configured !== 'boolean' ||
    !v.datasets || typeof v.datasets !== 'object'
  ) {
    throw new Error('Unexpected /api/status response')
  }
  return {
    version: String(v.version ?? ''),
    tiles: v.tiles,
    tile_sources: Array.isArray(v.tile_sources) ? v.tile_sources : v.tiles.map(id => ({ id, minzoom: 0, maxzoom: 14 })),
    ships_configured: v.ships_configured,
    traffic_configured: v.traffic_configured,
    flights_authenticated: !!v.flights_authenticated,
    flights_refresh_secs: Number(v.flights_refresh_secs ?? 120),
    ais: { connected: false, last_message_at: null, error: null, vessels: 0, aids: 0, ...(v.ais ?? {}) },
    datasets: { airports: 0, seaports: 0, reactors: 0, ...(v.datasets as Partial<Capabilities['datasets']>) },
  }
}

export interface Availability {
  ok: boolean
  /** Short reason shown under the layer name. */
  reason?: string
  /** What to do, e.g. a command or where to put a key. */
  fix?: string
}

export function requirementMet(req: Requirement, caps: Capabilities): boolean {
  switch (req.kind) {
    case 'key':
      return req.env === 'TOMTOM_API_KEY' ? caps.traffic_configured : caps.ships_configured
    case 'tiles':
      return caps.tiles.includes(req.tileset)
    case 'dataset':
      return caps.datasets[req.dataset] > 0
  }
}

export function availability(key: LayerKey, caps: Capabilities | null, error: boolean): Availability {
  const req = LAYER_BY_KEY[key].requires
  if (!req) return { ok: true }
  if (!caps) return { ok: false, reason: error ? 'Server not reachable' : 'Checking setup…' }
  if (requirementMet(req, caps)) return { ok: true }
  switch (req.kind) {
    case 'key':
      return { ok: false, reason: `Needs a free ${req.service} API key`, fix: `Add ${req.env}=… to backend/.env (key from ${req.url}), then restart the backend.` }
    case 'tiles':
      return { ok: false, reason: 'Tiles not built yet', fix: `Run: ${req.command}` }
    case 'dataset':
      return { ok: false, reason: 'Dataset not imported', fix: `Run: ${req.command}` }
  }
}

interface StatusState {
  caps: Capabilities | null
  error: boolean
  checking: boolean
  refresh: () => Promise<void>
}

export const useStatus = create<StatusState>((set, get) => ({
  caps: null,
  error: false,
  checking: false,
  refresh: async () => {
    if (get().checking) return
    set({ checking: true })
    try {
      const response = await fetch('/api/status', { cache: 'no-store', signal: AbortSignal.timeout(10_000) })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      set({ caps: parseCapabilities(await response.json()), error: false, checking: false })
    } catch {
      set({ error: true, checking: false })
    }
  },
}))

// ── Per-layer feed state (loading / ready / error + counts) ─────────────────

export interface FeedStatus {
  state: 'loading' | 'ready' | 'error'
  count?: number
  message?: string
  updatedAt?: number
}

export const useFeeds = create<{ feeds: Partial<Record<LayerKey, FeedStatus>>; set: (key: LayerKey, status: FeedStatus | null) => void }>(set => ({
  feeds: {},
  set: (key, status) =>
    set(s => {
      const feeds = { ...s.feeds }
      if (status) feeds[key] = status
      else delete feeds[key]
      return { feeds }
    }),
}))

export const setFeed = (key: LayerKey, status: FeedStatus | null) => useFeeds.getState().set(key, status)
