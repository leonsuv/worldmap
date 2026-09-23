import { create } from 'zustand'
import { api, postJson } from '../lib/api'
import { attempt } from './notice'
import { useAlerts } from './alerts'

export const EVENT_TYPES = [
  { key: 'storm', label: 'Storm', color: '#ef6b6b' },
  { key: 'outage', label: 'Outage', color: '#f29a45' },
  { key: 'closure', label: 'Closure', color: '#b98ae6' },
  { key: 'geopolitical', label: 'Geopolitical', color: '#ec6aa3' },
  { key: 'custom', label: 'Other', color: '#5ea8f2' },
] as const

export type EventType = (typeof EVENT_TYPES)[number]['key']
export const EVENT_COLOR = Object.fromEntries(EVENT_TYPES.map(t => [t.key, t.color])) as Record<string, string>

export interface MapEvent {
  id: number
  name: string
  event_type: EventType
  lat: number
  lon: number
  radius_km: number
  description: string
  started_at: number
  ended_at: number | null
  active: boolean
}

export interface AffectedAssets {
  ships: { mmsi: number; name: string; ship_type: number | null; lat: number; lon: number; speed: number | null; distance_km: number }[]
  flights: { icao24: string; callsign: string | null; lat: number; lon: number; altitude: number | null; distance_km: number }[]
  airports: { ident: string; iata: string | null; name: string; lat: number; lon: number; distance_km: number }[]
  seaports: { name: string; locode: string | null; lat: number; lon: number; distance_km: number }[]
  reactors: { name: string; country: string; lat: number; lon: number; capacity_mw: number; distance_km: number }[]
  total: number
}

export type NewEvent = Pick<MapEvent, 'name' | 'event_type' | 'lat' | 'lon' | 'radius_km' | 'description'>

/** A prefilled "new event" form requested from elsewhere (e.g. a place panel). */
export interface EventDraft {
  lat: number
  lon: number
  name?: string
}

interface EventState {
  events: MapEvent[]
  loading: boolean
  selectedId: number | null
  affected: AffectedAssets | null
  affectedLoading: boolean
  draft: EventDraft | null
  setDraft: (draft: EventDraft | null) => void
  fetch: () => Promise<void>
  create: (event: NewEvent) => Promise<boolean>
  close: (id: number) => Promise<boolean>
  remove: (id: number) => Promise<boolean>
  select: (id: number | null) => void
}

export const useEvents = create<EventState>((set, get) => ({
  events: [],
  loading: false,
  selectedId: null,
  affected: null,
  affectedLoading: false,
  draft: null,
  setDraft: draft => set({ draft }),
  fetch: async () => {
    set({ loading: true })
    await attempt(async () => set({ events: await api<MapEvent[]>('/api/events') }))
    set({ loading: false })
  },
  create: event =>
    attempt(async () => {
      const created = await postJson<MapEvent>('/api/events', event)
      await get().fetch()
      get().select(created.id)
      await useAlerts.getState().fetchCount()
    }),
  close: id =>
    attempt(async () => {
      await api(`/api/events/${id}/close`, { method: 'POST' })
      await get().fetch()
    }),
  remove: id =>
    attempt(async () => {
      await api(`/api/events/${id}`, { method: 'DELETE' })
      set(s => ({
        events: s.events.filter(e => e.id !== id),
        selectedId: s.selectedId === id ? null : s.selectedId,
        affected: s.selectedId === id ? null : s.affected,
      }))
    }),
  select: id => {
    set({ selectedId: id, affected: null, affectedLoading: id !== null })
    if (id === null) return
    void api<AffectedAssets>(`/api/events/affected?event_id=${id}`)
      .then(affected => {
        if (get().selectedId === id) set({ affected, affectedLoading: false })
      })
      .catch(() => {
        if (get().selectedId === id) set({ affectedLoading: false })
      })
  },
}))
