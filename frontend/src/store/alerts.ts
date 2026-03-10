import { create } from 'zustand'

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
    try {
      const r = await fetch('/api/alerts')
      if (r.ok) {
        const data = await r.json()
        set({ alerts: data })
      }
    } finally {
      set({ loading: false })
    }
    get().fetchCount()
  },
  fetchCount: async () => {
    try {
      const r = await fetch('/api/alerts/count')
      if (r.ok) {
        const data = await r.json()
        set({ count: data.count })
      }
    } catch { /* ignore */ }
  },
  ack: async (id) => {
    await fetch(`/api/alerts/${id}/ack`, { method: 'POST' })
    set(s => ({
      alerts: s.alerts.map(a => a.id === id ? { ...a, acknowledged: true } : a),
      count: Math.max(0, s.count - 1),
    }))
  },
  ackAll: async () => {
    await fetch('/api/alerts/ack-all', { method: 'POST' })
    set(s => ({
      alerts: s.alerts.map(a => ({ ...a, acknowledged: true })),
      count: 0,
    }))
  },
}))
