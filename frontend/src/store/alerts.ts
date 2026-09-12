import { create } from 'zustand'
import { apiRequest, reportFailure } from './notice'

export interface AlertItem {
  id: number
  event_id: number | null
  title: string
  message: string
  severity: string
  acknowledged: boolean
  created_at: number
}

interface AlertState {
  alerts: AlertItem[]
  count: number
  open: boolean
  loading: boolean
  toggle: () => void
  fetch: () => Promise<void>
  fetchCount: () => Promise<void>
  ack: (id: number) => Promise<void>
  ackAll: () => Promise<void>
}

export const useAlertStore = create<AlertState>((set, get) => ({
  alerts: [],
  count: 0,
  open: false,
  loading: false,
  toggle: () => set(s => ({ open: !s.open })),
  fetch: async () => {
    set({ loading: true })
    await reportFailure(async () => set({ alerts: await apiRequest<AlertItem[]>('/api/alerts') }))
    set({ loading: false })
    await get().fetchCount()
  },
  fetchCount: async () => {
    if (document.hidden) return
    try { const data = await apiRequest<{ count: number }>('/api/alerts/count'); set({ count: data.count }) } catch { /* Retry with next poll. */ }
  },
  ack: async (id) => {
    await reportFailure(async () => {
      await apiRequest(`/api/alerts/${id}/ack`, { method: 'POST' })
      set(s => ({ alerts: s.alerts.map(a => a.id === id ? { ...a, acknowledged: true } : a) }))
      await get().fetchCount()
    })
  },
  ackAll: async () => {
    await reportFailure(async () => {
      await apiRequest('/api/alerts/ack-all', { method: 'POST' })
      set(s => ({ alerts: s.alerts.map(a => ({ ...a, acknowledged: true })), count: 0 }))
    })
  },
}))
