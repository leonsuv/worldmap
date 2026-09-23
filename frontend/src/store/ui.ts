import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type Panel = 'details' | 'watchlist' | 'events' | 'alerts'
export type Theme = 'dark' | 'light'
export type Projection = 'mercator' | 'globe'

export interface PickRequest {
  label: string
  onPick: (lngLat: [number, number]) => void
}

interface UiState {
  panel: Panel | null
  theme: Theme
  projection: Projection
  layersOpen: boolean
  legendOpen: boolean
  pick: PickRequest | null
  openPanel: (panel: Panel) => void
  togglePanel: (panel: Panel) => void
  closePanel: () => void
  setTheme: (theme: Theme) => void
  setProjection: (projection: Projection) => void
  setLayersOpen: (open: boolean) => void
  setLegendOpen: (open: boolean) => void
  startPick: (request: PickRequest) => void
  cancelPick: () => void
}

const prefersDark = () => typeof window === 'undefined' || !window.matchMedia || window.matchMedia('(prefers-color-scheme: dark)').matches
const wideScreen = () => typeof window === 'undefined' || window.innerWidth > 760

export const useUi = create<UiState>()(
  persist(
    set => ({
      panel: null,
      theme: prefersDark() ? 'dark' : 'light',
      projection: 'mercator',
      layersOpen: wideScreen(),
      legendOpen: true,
      pick: null,
      openPanel: panel => set({ panel }),
      togglePanel: panel => set(s => ({ panel: s.panel === panel ? null : panel })),
      closePanel: () => set({ panel: null }),
      setTheme: theme => set({ theme }),
      setProjection: projection => set({ projection }),
      setLayersOpen: layersOpen => set({ layersOpen }),
      setLegendOpen: legendOpen => set({ legendOpen }),
      startPick: pick => set({ pick }),
      cancelPick: () => set({ pick: null }),
    }),
    {
      name: 'worldmap-ui',
      version: 1,
      partialize: s => ({ theme: s.theme, projection: s.projection, legendOpen: s.legendOpen }),
    },
  ),
)
