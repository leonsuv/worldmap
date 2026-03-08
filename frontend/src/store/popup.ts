import { create } from 'zustand'

export interface PopupState {
  lngLat: [number, number] | null
  layerId: string | null
  properties: Record<string, unknown> | null
  open: (lngLat: [number, number], layerId: string, properties: Record<string, unknown>) => void
  close: () => void
}

export const usePopupStore = create<PopupState>((set) => ({
  lngLat: null,
  layerId: null,
  properties: null,
  open: (lngLat, layerId, properties) => set({ lngLat, layerId, properties }),
  close: () => set({ lngLat: null, layerId: null, properties: null }),
}))
