import { create } from 'zustand'
import { apiRequest, reportFailure } from './notice'

export interface WatchlistItem {
  id: number
  wtype: string
  name: string
  params: Record<string, unknown>
  created_at: number
}

interface WatchlistState {
  items: WatchlistItem[]
  open: boolean
  loading: boolean
  toggle: () => void
  fetch: () => Promise<void>
  add: (wtype: string, name: string, params?: Record<string, unknown>) => Promise<boolean>
  remove: (id: number) => Promise<void>
}

export const useWatchlistStore = create<WatchlistState>((set, get) => ({
  items: [],
  open: false,
  loading: false,
  toggle: () => set(s => ({ open: !s.open })),
  fetch: async () => {
    set({ loading: true })
    await reportFailure(async () => set({ items: await apiRequest<WatchlistItem[]>('/api/watchlist') }))
    set({ loading: false })
  },
  add: async (wtype, name, params) => reportFailure(async () => {
    await apiRequest('/api/watchlist', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ wtype, name, params: params ?? {} }) })
    await get().fetch()
  }),
  remove: async (id) => {
    await reportFailure(async () => { await apiRequest(`/api/watchlist/${id}`, { method: 'DELETE' }); await get().fetch() })
  },
}))
