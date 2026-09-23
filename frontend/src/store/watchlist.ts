import { create } from 'zustand'
import { api, postJson } from '../lib/api'
import { attempt } from './notice'
import { useAlerts } from './alerts'

export type WatchType = 'vessel' | 'port' | 'airport' | 'reactor' | 'area' | 'pipeline'

export interface WatchItem {
  id: number
  wtype: WatchType
  name: string
  params: { mmsi?: number; lat?: number; lon?: number; radius_km?: number }
  created_at: number
  live?: { lat: number; lon: number; speed: number | null; age_secs: number } | null
}

interface WatchlistState {
  items: WatchItem[]
  loading: boolean
  loaded: boolean
  fetch: () => Promise<void>
  add: (wtype: WatchType, name: string, params: WatchItem['params']) => Promise<boolean>
  remove: (id: number) => Promise<boolean>
}

export const useWatchlist = create<WatchlistState>((set, get) => ({
  items: [],
  loading: false,
  loaded: false,
  fetch: async () => {
    set({ loading: true })
    await attempt(async () => set({ items: await api<WatchItem[]>('/api/watchlist'), loaded: true }))
    set({ loading: false })
  },
  add: (wtype, name, params) =>
    attempt(async () => {
      await postJson('/api/watchlist', { wtype, name, params })
      await get().fetch()
      await useAlerts.getState().fetchCount()
    }),
  remove: id =>
    attempt(async () => {
      await api(`/api/watchlist/${id}`, { method: 'DELETE' })
      set(s => ({ items: s.items.filter(i => i.id !== id) }))
    }),
}))
