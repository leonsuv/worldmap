import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export interface LayerState {
  flights: boolean
  ships: boolean
  aton: boolean
  weather: boolean
  reactors: boolean
  pipelines: boolean
  powerGrid: boolean
  hvLines: boolean
  solar: boolean
  windTurbines: boolean
  traffic: boolean
  airports: boolean
  seaports: boolean
  buildings3d: boolean
  toggle: (layer: keyof Omit<LayerState, 'toggle'>) => void
}

export const useLayerStore = create<LayerState>()(
  persist(
    (set) => ({
      flights: false,
      ships: false,
      aton: false,
      weather: false,
      reactors: false,
      pipelines: false,
      powerGrid: false,
      hvLines: false,
      solar: false,
      windTurbines: false,
      traffic: false,
      airports: false,
      seaports: false,
      buildings3d: false,
      toggle: (layer) => set((s) => ({ [layer]: !s[layer] })),
    }),
    { name: 'worldmap-layers' },
  ),
)
