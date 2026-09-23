import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { LAYER_KEYS, type LayerKey } from '../catalog'

export type EnabledLayers = Record<LayerKey, boolean>

export const DEFAULT_LAYERS: EnabledLayers = Object.fromEntries(
  LAYER_KEYS.map(k => [k, k === 'flights' || k === 'airports']),
) as EnabledLayers

interface LayerState {
  enabled: EnabledLayers
  toggle: (key: LayerKey) => void
  setMany: (values: Partial<EnabledLayers>) => void
  only: (keys: LayerKey[]) => void
}

/** Keep only known keys from persisted state (older versions stored more). */
export function sanitize(value: unknown): EnabledLayers {
  const source = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>
  return Object.fromEntries(LAYER_KEYS.map(k => [k, typeof source[k] === 'boolean' ? source[k] : DEFAULT_LAYERS[k]])) as EnabledLayers
}

export const useLayers = create<LayerState>()(
  persist(
    set => ({
      enabled: DEFAULT_LAYERS,
      toggle: key => set(s => ({ enabled: { ...s.enabled, [key]: !s.enabled[key] } })),
      setMany: values => set(s => ({ enabled: { ...s.enabled, ...values } })),
      only: keys => set({ enabled: Object.fromEntries(LAYER_KEYS.map(k => [k, keys.includes(k)])) as EnabledLayers }),
    }),
    {
      name: 'worldmap-layers',
      version: 2,
      partialize: s => ({ enabled: s.enabled }),
      // Version 1 stored flags at the top level.
      migrate: (persisted, version) => ({ enabled: sanitize(version < 2 ? persisted : (persisted as { enabled?: unknown })?.enabled) }),
      merge: (persisted, current) => ({ ...current, enabled: sanitize((persisted as { enabled?: unknown } | undefined)?.enabled) }),
    },
  ),
)
