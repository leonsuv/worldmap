import type { ComponentType } from 'react'
import { Anchor, Atom, Cable, Car, Factory, Lightbulb, Plane, Ship, TowerControl, UtilityPole, Wind, Building2 } from 'lucide-react'

export type LayerKey =
  | 'flights' | 'ships' | 'weather' | 'traffic' | 'airports' | 'seaports' | 'aton' | 'reactors' | 'pipelines' | 'hvLines' | 'powerGrid' | 'buildings3d'

export type Requirement =
  | { kind: 'key'; env: string; service: string; url: string }
  | { kind: 'tiles'; tileset: string; command: string }
  | { kind: 'dataset'; dataset: 'airports' | 'seaports' | 'reactors'; command: string }

export interface LayerDef {
  key: LayerKey
  label: string
  group: 'Live activity' | 'Transport' | 'Energy' | 'Map'
  icon: ComponentType<{ size?: number; strokeWidth?: number }>
  color: string
  source: string
  attribution: string
  /** Data only appears from this zoom level. */
  minZoom?: number
  requires?: Requirement
}

export const LAYERS: LayerDef[] = [
  { key: 'flights', label: 'Flights', group: 'Live activity', icon: Plane, color: '#8fc7ff', source: 'OpenSky Network', attribution: 'OpenSky Network' },
  {
    key: 'ships', label: 'Vessels', group: 'Live activity', icon: Ship, color: '#74c46a', source: 'AIS · AISstream', attribution: 'AISstream.io',
    requires: { kind: 'key', env: 'AISSTREAM_API_KEY', service: 'AISstream', url: 'https://aisstream.io' },
  },
  { key: 'weather', label: 'Wind & weather', group: 'Live activity', icon: Wind, color: '#8ecae6', source: 'Open-Meteo', attribution: 'Open-Meteo.com' },
  {
    key: 'traffic', label: 'Road traffic', group: 'Live activity', icon: Car, color: '#f3ab7b', source: 'TomTom flow', attribution: 'TomTom', minZoom: 5,
    requires: { kind: 'key', env: 'TOMTOM_API_KEY', service: 'TomTom', url: 'https://developer.tomtom.com' },
  },
  {
    key: 'airports', label: 'Airports', group: 'Transport', icon: TowerControl, color: '#9fb4ff', source: 'OurAirports', attribution: 'OurAirports',
    requires: { kind: 'dataset', dataset: 'airports', command: 'python scripts/ingest.py airports' },
  },
  {
    key: 'seaports', label: 'Seaports', group: 'Transport', icon: Anchor, color: '#5fd0c4', source: 'World Port Index', attribution: 'NGA World Port Index',
    requires: { kind: 'dataset', dataset: 'seaports', command: 'python scripts/ingest.py seaports' },
  },
  {
    key: 'aton', label: 'Navigation aids', group: 'Transport', icon: Lightbulb, color: '#f2d06b', source: 'AIS buoys & beacons', attribution: 'AISstream.io', minZoom: 6,
    requires: { kind: 'key', env: 'AISSTREAM_API_KEY', service: 'AISstream', url: 'https://aisstream.io' },
  },
  {
    key: 'reactors', label: 'Nuclear plants', group: 'Energy', icon: Atom, color: '#f2d06b', source: 'GeoNuclearData', attribution: 'GeoNuclearData',
    requires: { kind: 'dataset', dataset: 'reactors', command: 'python scripts/ingest.py reactors' },
  },
  {
    key: 'hvLines', label: 'High-voltage lines', group: 'Energy', icon: UtilityPole, color: '#f0a24b', source: 'OpenStreetMap ≥ 110 kV', attribution: 'OpenStreetMap',
    requires: { kind: 'tiles', tileset: 'hv-lines', command: 'python scripts/build_tiles.py hv-lines' },
  },
  {
    key: 'pipelines', label: 'Pipelines', group: 'Energy', icon: Factory, color: '#e07a5f', source: 'Oil, gas & hydrogen', attribution: 'OpenStreetMap',
    requires: { kind: 'tiles', tileset: 'pipelines', command: 'python scripts/build_tiles.py pipelines' },
  },
  {
    key: 'powerGrid', label: 'Estimated power grid', group: 'Energy', icon: Cable, color: '#d8c06a', source: 'Gridfinder model', attribution: 'Gridfinder',
    requires: { kind: 'tiles', tileset: 'power-grid', command: 'python scripts/build_tiles.py power-grid' },
  },
  { key: 'buildings3d', label: '3D buildings', group: 'Map', icon: Building2, color: '#aabbd0', source: 'OpenStreetMap', attribution: 'OpenStreetMap', minZoom: 14 },
]

export const LAYER_BY_KEY = Object.fromEntries(LAYERS.map(l => [l.key, l])) as Record<LayerKey, LayerDef>
export const LAYER_KEYS = LAYERS.map(l => l.key)
export const GROUPS = [...new Set(LAYERS.map(l => l.group))]

export const PRESETS: { name: string; keys: LayerKey[] }[] = [
  { name: 'Aviation', keys: ['flights', 'airports', 'weather'] },
  { name: 'Maritime', keys: ['ships', 'seaports', 'aton'] },
  { name: 'Energy', keys: ['reactors', 'hvLines', 'pipelines', 'powerGrid'] },
]

export const REGIONS: { name: string; center: [number, number]; zoom: number }[] = [
  { name: 'World', center: [10, 25], zoom: 1.6 },
  { name: 'Europe', center: [12, 50], zoom: 3.6 },
  { name: 'North America', center: [-98, 40], zoom: 3 },
  { name: 'South America', center: [-60, -18], zoom: 2.8 },
  { name: 'Africa', center: [20, 3], zoom: 2.8 },
  { name: 'Middle East', center: [48, 27], zoom: 3.8 },
  { name: 'Asia', center: [100, 30], zoom: 2.8 },
  { name: 'Oceania', center: [140, -25], zoom: 3 },
]
