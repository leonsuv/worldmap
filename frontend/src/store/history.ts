import { create } from 'zustand'

export interface HistoryPoint {
  mmsi: number
  lat: number
  lon: number
  course: number
  speed: number
  heading: number
  ship_name: string
  ship_type: number
  recorded_at: number
}

interface HistoryState {
  enabled: boolean
  positions: HistoryPoint[]
  timestamps: number[]
  currentTs: number | null
  loading: boolean
  toggle: () => void
  fetchTimestamps: () => Promise<void>
  seek: (ts: number) => Promise<void>
}

export const useHistoryStore = create<HistoryState>((set) => ({
  enabled: false,
  positions: [],
  timestamps: [],
  currentTs: null,
  loading: false,
  toggle: () => set(s => {
    if (s.enabled) return { enabled: false, positions: [], currentTs: null }
    return { enabled: true }
  }),
  fetchTimestamps: async () => {
    try {
      const r = await fetch('/api/history/timestamps')
      if (r.ok) {
        const data = await r.json()
        set({ timestamps: data.timestamps.sort((a: number, b: number) => a - b) })
      }
    } catch { /* ignore */ }
  },
  seek: async (ts) => {
    set({ loading: true, currentTs: ts })
    try {
      // Fetch a 5-minute window around the timestamp
      const r = await fetch(`/api/history/ships?from=${ts - 150}&to=${ts + 150}`)
      if (r.ok) set({ positions: await r.json() })
    } finally {
      set({ loading: false })
    }
  },
}))
