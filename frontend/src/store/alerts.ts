import { create } from 'zustand'
import { api } from '../lib/api'
import { attempt } from './notice'

export interface Alert {
  id: number
  event_id: number | null
  watch_id: number | null
  title: string
  message: string
  severity: 'info' | 'warning' | 'critical'
  acknowledged: boolean
  created_at: number
  distance_km: number | null
}

interface AlertState {
  alerts: Alert[]
  count: number
  loading: boolean
  fetch: () => Promise<void>
  fetchCount: () => Promise<void>
  ack: (id: number) => Promise<boolean>
  ackAll: () => Promise<boolean>
}

export const useAlerts = create<AlertState>((set, get) => ({
  alerts: [],
  count: 0,
  loading: false,
  fetch: async () => {
    set({ loading: true })
    await attempt(async () => set({ alerts: await api<Alert[]>('/api/alerts') }))
    set({ loading: false })
    await get().fetchCount()
  },
  fetchCount: async () => {
    try {
      const { count } = await api<{ count: number }>('/api/alerts/count')
      set({ count })
    } catch {
      /* retried by the next poll */
    }
  },
  ack: id =>
    attempt(async () => {
      await api(`/api/alerts/${id}/ack`, { method: 'POST' })
      set(s => ({ alerts: s.alerts.map(a => (a.id === id ? { ...a, acknowledged: true } : a)), count: Math.max(0, s.count - 1) }))
    }),
  ackAll: () =>
    attempt(async () => {
      await api('/api/alerts/ack-all', { method: 'POST' })
      set(s => ({ alerts: s.alerts.map(a => ({ ...a, acknowledged: true })), count: 0 }))
    }),
}))
