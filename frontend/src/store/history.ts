import { create } from 'zustand'
import { api, isAbort, rowsToObjects } from '../lib/api'

export interface HistoryPoint {
  mmsi: number
  lon: number
  lat: number
  course: number | null
  speed: number | null
  heading: number | null
  ship_type: number | null
  name: string
}

interface HistoryState {
  enabled: boolean
  timestamps: number[]
  index: number
  positions: HistoryPoint[]
  loading: boolean
  playing: boolean
  error: string | null
  open: () => Promise<void>
  close: () => void
  seek: (index: number) => Promise<void>
  setPlaying: (playing: boolean) => void
}

let request: AbortController | null = null
let player: ReturnType<typeof setInterval> | null = null

function stopPlayer() {
  if (player) clearInterval(player)
  player = null
}

export const useHistory = create<HistoryState>((set, get) => ({
  enabled: false,
  timestamps: [],
  index: -1,
  positions: [],
  loading: false,
  playing: false,
  error: null,
  open: async () => {
    request?.abort()
    const ac = new AbortController()
    request = ac
    set({ enabled: true, loading: true, error: null, positions: [], timestamps: [], index: -1 })
    try {
      const { timestamps } = await api<{ timestamps: number[] }>('/api/history/timestamps', { signal: ac.signal })
      if (ac.signal.aborted || !get().enabled) return
      const sorted = [...timestamps].sort((a, b) => a - b)
      set({ timestamps: sorted, loading: false })
      if (sorted.length) await get().seek(sorted.length - 1)
    } catch (error) {
      if (!isAbort(error) && get().enabled) set({ loading: false, error: 'Recorded positions could not be loaded.' })
    }
  },
  close: () => {
    request?.abort()
    stopPlayer()
    set({ enabled: false, positions: [], timestamps: [], index: -1, loading: false, playing: false, error: null })
  },
  seek: async index => {
    const ts = get().timestamps[index]
    if (ts === undefined) return
    request?.abort()
    const ac = new AbortController()
    request = ac
    set({ index, loading: true, error: null })
    try {
      const data = await api<{ fields: string[]; rows: unknown[][] }>(`/api/history/ships?at=${ts}`, { signal: ac.signal })
      if (ac.signal.aborted || !get().enabled) return
      set({ positions: rowsToObjects<HistoryPoint>(data.fields, data.rows), loading: false })
    } catch (error) {
      if (!isAbort(error) && get().enabled) set({ loading: false, error: 'This snapshot could not be loaded.' })
    }
  },
  setPlaying: playing => {
    stopPlayer()
    set({ playing })
    if (!playing) return
    player = setInterval(() => {
      const { index, timestamps, loading, enabled } = get()
      if (!enabled) return stopPlayer()
      if (loading) return
      if (index >= timestamps.length - 1) {
        stopPlayer()
        set({ playing: false })
        return
      }
      void get().seek(index + 1)
    }, 1200)
  },
}))
