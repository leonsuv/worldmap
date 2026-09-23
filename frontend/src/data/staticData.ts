import { api } from '../lib/api'
import { setFeed } from '../store/status'
import { createSignal } from '../map/scheduler'
import type { LayerKey } from '../catalog'

export type Collection = GeoJSON.FeatureCollection<GeoJSON.Point>

export type StaticKey = 'airports' | 'seaports' | 'reactors' | 'aton'

const PATHS: Record<StaticKey, string> = {
  airports: '/api/airports',
  seaports: '/api/seaports',
  reactors: '/api/reactors',
  aton: '/api/ships/aton',
}

const data: Partial<Record<StaticKey, Collection>> = {}
const pending = new Map<StaticKey, Promise<void>>()
export const staticSignal = createSignal()

export const getCollection = (key: StaticKey): Collection | undefined => data[key]

/** Load a dataset once (AtoN is refreshed on demand). */
export function loadCollection(key: StaticKey, force = false): Promise<void> {
  if (data[key] && !force) return Promise.resolve()
  const existing = pending.get(key)
  if (existing) return existing
  if (!data[key]) setFeed(key as LayerKey, { state: 'loading' })
  const run = api<Collection>(PATHS[key])
    .then(fc => {
      data[key] = fc
      setFeed(key as LayerKey, {
        state: 'ready',
        count: fc.features.length,
        message: key === 'aton' && !fc.features.length ? 'Waiting for AIS navigation-aid reports' : undefined,
      })
      staticSignal.emit()
    })
    .catch(error => setFeed(key as LayerKey, { state: 'error', message: error instanceof Error ? error.message : 'Unavailable' }))
    .finally(() => pending.delete(key))
  pending.set(key, run)
  return run
}

/** Forget a dataset so it reloads (after re-import). */
export function invalidateCollection(key: StaticKey) {
  delete data[key]
}

/** Find a feature by a property value, e.g. an airport by ICAO code. */
export function findFeature(key: StaticKey, prop: string, value: unknown): GeoJSON.Feature<GeoJSON.Point> | undefined {
  return data[key]?.features.find(f => f.properties?.[prop] === value)
}
