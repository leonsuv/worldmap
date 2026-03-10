import { create } from 'zustand'

export interface WatchlistItem {
  id: number
  wtype: string
  name: string
  params: string
  created_at: number
}

interface WatchlistState {
  items: WatchlistItem[]
  open: boolean
  loading: boolean
  toggle: () => void
  fetch: () => Promise<void>
  add: (wtype: string, name: string, params?: Record<string, unknown>) => Promise<void>
  remove: (id: number) => Promise<void>
}

export const useWatchlistStore = create<WatchlistState>((set, get) => ({
  items: [],
  open: false,
  loading: false,
  toggle: () => set(s => ({ open: !s.open })),
  fetch: async () => {
    set({ loading: true })
    try {
      const r = await fetch('/api/watchlist')
      if (r.ok) set({ items: await r.json() })
    } finally {
      set({ loading: false })
    }
  },
  add: async (wtype, name, params) => {
    const r = await fetch('/api/watchlist', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ wtype, name, params: params ? JSON.stringify(params) : '{}' }),
    })
    if (r.ok) get().fetch()
  },
  remove: async (id) => {
    await fetch(`/api/watchlist/${id}`, { method: 'DELETE' })
    get().fetch()
  },
}))
