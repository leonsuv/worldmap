import { create } from 'zustand'

export interface EventItem {
  id: number
  name: string
  event_type: string
  lat: number
  lon: number
  radius_km: number
  description: string
  started_at: number
  ended_at: number | null
  active: boolean
}

export interface AffectedAssets {
  ships: Record<string, unknown>[]
  airports: Record<string, unknown>[]
  seaports: Record<string, unknown>[]
  reactors: Record<string, unknown>[]
  total: number
}

interface EventState {
  events: EventItem[]
  open: boolean
  selectedId: number | null
  affected: AffectedAssets | null
  loading: boolean
  toggle: () => void
  fetch: () => Promise<void>
  create: (e: Omit<EventItem, 'id' | 'started_at' | 'ended_at' | 'active'>) => Promise<void>
  close: (id: number) => Promise<void>
  remove: (id: number) => Promise<void>
  select: (id: number | null) => void
  fetchAffected: (id: number) => Promise<void>
}

export const useEventStore = create<EventState>((set, get) => ({
  events: [],
  open: false,
  selectedId: null,
  affected: null,
  loading: false,
  toggle: () => set(s => ({ open: !s.open })),
  fetch: async () => {
    set({ loading: true })
    try {
      const r = await fetch('/api/events')
      if (r.ok) set({ events: await r.json() })
    } finally {
      set({ loading: false })
    }
  },
  create: async (e) => {
    const r = await fetch('/api/events', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(e),
    })
    if (r.ok) get().fetch()
  },
  close: async (id) => {
    await fetch(`/api/events/${id}/close`, { method: 'POST' })
    get().fetch()
  },
  remove: async (id) => {
    await fetch(`/api/events/${id}`, { method: 'DELETE' })
    set(s => ({ events: s.events.filter(e => e.id !== id), selectedId: s.selectedId === id ? null : s.selectedId, affected: s.selectedId === id ? null : s.affected }))
  },
  select: (id) => {
    set({ selectedId: id, affected: null })
    if (id) get().fetchAffected(id)
  },
  fetchAffected: async (id) => {
    try {
      const r = await fetch(`/api/events/affected?event_id=${id}`)
      if (r.ok) set({ affected: await r.json() })
    } catch { /* ignore */ }
  },
}))
