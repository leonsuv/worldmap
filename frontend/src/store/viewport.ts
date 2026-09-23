import { create } from 'zustand'

export interface ViewportState {
  zoom: number
  /** [west, south, east, north]; west/east may exceed ±180 on repeated worlds. */
  bbox: [number, number, number, number]
  center: [number, number]
  /** Pointer position, or null when the pointer is off the map. */
  pointer: [number, number] | null
  setViewport: (v: Pick<ViewportState, 'zoom' | 'bbox' | 'center'>) => void
  setPointer: (p: [number, number] | null) => void
}

export const useViewport = create<ViewportState>(set => ({
  zoom: 2,
  bbox: [-180, -85, 180, 85],
  center: [10, 25],
  pointer: null,
  setViewport: v => set(v),
  setPointer: pointer => set({ pointer }),
}))
