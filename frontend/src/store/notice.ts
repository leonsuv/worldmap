import { create } from 'zustand'
export const useNoticeStore = create<{ message: string | null; show: (message: string) => void; dismiss: () => void }>(set => ({
  message: null, show: message => set({ message }), dismiss: () => set({ message: null }),
}))
export async function apiRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, init)
  if (!response.ok) throw new Error(`Request failed (${response.status}). Please try again.`)
  if (response.status === 204) return undefined as T
  return response.json() as Promise<T>
}
export async function reportFailure(action: () => Promise<void>): Promise<boolean> {
  try { await action(); return true } catch (error) {
    useNoticeStore.getState().show(error instanceof Error ? error.message : 'Connection failed. Please try again.')
    return false
  }
}
