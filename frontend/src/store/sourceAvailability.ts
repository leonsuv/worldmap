import { create } from 'zustand'

export const SOURCE_REQUIREMENTS: Record<string, { label: string; instruction: string }> = {
  traffic: { label: 'TomTom API key required', instruction: 'Set TOMTOM_API_KEY in backend/.env and restart the backend.' },
  ships: { label: 'AISstream API key required', instruction: 'Set AISSTREAM_API_KEY in backend/.env and restart the backend.' },
  aton: { label: 'AISstream API key required', instruction: 'Set AISSTREAM_API_KEY in backend/.env and restart the backend.' },
  pipelines: { label: 'Pipeline tiles required', instruction: 'Run make pipeline-tiles from the project directory, then restart the backend.' },
  powerGrid: { label: 'Power-grid tiles required', instruction: 'Run make grid-tiles from the project directory, then restart the backend.' },
  hvLines: { label: 'High-voltage tiles required', instruction: 'Run make grid-tiles from the project directory, then restart the backend.' },
}
export interface SourceCapabilities { tiles: string[]; traffic_configured: boolean; ships_configured: boolean }
export function parseCapabilities(data: unknown): SourceCapabilities {
  const value = data as Partial<SourceCapabilities> | null
  if (!value || !Array.isArray(value.tiles) || !value.tiles.every(t => typeof t === 'string') || typeof value.traffic_configured !== 'boolean' || typeof value.ships_configured !== 'boolean') throw new Error('Invalid source status')
  return value as SourceCapabilities
}
export function sourceAvailable(key: string, data: SourceCapabilities | null): boolean {
  if (!(key in SOURCE_REQUIREMENTS)) return true
  if (!data) return false
  if (key === 'traffic') return data.traffic_configured
  if (key === 'ships' || key === 'aton') return data.ships_configured
  const tile = ({ pipelines: 'pipelines', powerGrid: 'power-grid', hvLines: 'hv-lines' } as Record<string, string>)[key]
  return data.tiles.includes(tile)
}
interface AvailabilityState {
  data: SourceCapabilities | null
  checking: boolean
  error: boolean
  refresh: () => Promise<void>
}
export const useSourceAvailability = create<AvailabilityState>((set, get) => ({
  data: null, checking: false, error: false,
  refresh: async () => {
    if (get().checking) return
    set({ checking: true, error: false })
    try {
      const response = await fetch('/api/status', { signal: AbortSignal.timeout(10_000), cache: 'no-store' })
      if (!response.ok) throw new Error('Backend unavailable')
      set({ data: parseCapabilities(await response.json()), checking: false })
    } catch { set({ checking: false, error: true }) }
  },
}))
