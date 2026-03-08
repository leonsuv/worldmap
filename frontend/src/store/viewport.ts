import { create } from 'zustand'

export interface ViewportState {
  zoom: number
  bbox: [number, number, number, number] // [west, south, east, north]
  center: [number, number] // [lon, lat]
  setViewport: (v: Pick<ViewportState, 'zoom' | 'bbox' | 'center'>) => void
}

export const useViewportStore = create<ViewportState>((set) => ({
  zoom: 2,
  bbox: [-180, -90, 180, 90],
  center: [0, 20],
  setViewport: (v) => set(v),
}))
