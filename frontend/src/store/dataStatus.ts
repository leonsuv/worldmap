import { create } from 'zustand'

export type DataStatus = { state: 'loading' | 'ready' | 'error'; count?: number; message?: string }
export const useDataStatus = create<{ sources: Record<string, DataStatus>; set: (key: string, status: DataStatus) => void }>((set) => ({
  sources: {},
  set: (key, status) => set(s => ({ sources: { ...s.sources, [key]: status } })),
}))
