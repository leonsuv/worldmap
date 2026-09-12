import { create } from 'zustand'

export interface HistoryPoint {
  mmsi: number; lat: number; lon: number; course: number | null; speed: number | null
  heading: number | null; ship_name: string; ship_type: number | null; recorded_at: number
}
interface HistoryState {
  enabled: boolean; positions: HistoryPoint[]; timestamps: number[]; currentTs: number | null
  loading: boolean; error: string | null; toggle: () => void
  fetchTimestamps: () => Promise<void>; seek: (ts: number) => Promise<void>
}
let request: AbortController | null = null
let timestampsRequest: AbortController | null = null
export const useHistoryStore = create<HistoryState>((set, get) => ({
  enabled: false, positions: [], timestamps: [], currentTs: null, loading: false, error: null,
  toggle: () => {
    request?.abort(); timestampsRequest?.abort()
    const enabled = !get().enabled
    set({ enabled, positions: [], currentTs: null, loading: false, error: null })
    if (enabled) void get().fetchTimestamps()
  },
  fetchTimestamps: async () => {
    timestampsRequest?.abort()
    const ac = new AbortController(); timestampsRequest = ac
    set({ loading: true, error: null })
    try {
      const response = await fetch('/api/history/timestamps', { signal: ac.signal })
      if (!response.ok) throw new Error('History unavailable')
      const data = await response.json()
      if (ac.signal.aborted || !get().enabled) return
      const timestamps = (data.timestamps as number[]).sort((a, b) => a - b)
      set({ timestamps, loading: false })
      if (timestamps.length) await get().seek(timestamps[timestamps.length - 1])
    } catch {
      if (!ac.signal.aborted) set({ loading: false, error: 'History could not be loaded.' })
    }
  },
  seek: async (ts) => {
    request?.abort()
    const ac = new AbortController(); request = ac
    set({ loading: true, currentTs: ts, error: null })
    try {
      const response = await fetch(`/api/history/ships?from=${ts}&to=${ts}`, { signal: ac.signal })
      if (!response.ok) throw new Error('History unavailable')
      const positions: HistoryPoint[] = await response.json()
      if (!ac.signal.aborted && get().enabled) set({ positions, loading: false })
    } catch {
      if (!ac.signal.aborted) set({ loading: false, error: 'Snapshot could not be loaded.' })
    }
  },
}))
