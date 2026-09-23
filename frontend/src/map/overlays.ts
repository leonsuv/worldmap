/**
 * Map layers rendered natively by MapLibre: imported point datasets (with
 * collision-aware labels), vector tile networks, traffic and 3D buildings.
 * `syncOverlays` is idempotent and re-creates everything after a style change.
 */
import type { ExpressionSpecification, GeoJSONSource, LayerSpecification, Map as MapLibreMap, SourceSpecification } from 'maplibre-gl'
import type { EnabledLayers } from '../store/layers'
import type { Theme } from '../store/ui'
import type { Capabilities } from '../store/status'
import { getCollection, type StaticKey } from '../data/staticData'
import { firstSymbolLayer, labelFont } from './basemap'
import { HV_COLORS, PALETTES, PIPELINE_COLORS } from './colors'

/** Ids MapLibre should report for hover and click. */
export const PICKABLE_LAYERS = [
  'wm-airports-large',
  'wm-airports-medium',
  'wm-seaports-major',
  'wm-seaports-minor',
  'wm-reactors',
  'wm-aton',
  'wm-hv-line',
  'wm-hv-cable',
  'wm-pipelines',
  'wm-grid',
]

const zoomLinear = (...stops: (number | ExpressionSpecification)[]): ExpressionSpecification =>
  ['interpolate', ['linear'], ['zoom'], ...stops] as unknown as ExpressionSpecification

function hvColor(): ExpressionSpecification {
  return [
    'case',
    ['==', ['get', 'hvdc'], true], HV_COLORS.hvdc,
    ['step', ['to-number', ['get', 'voltage_kv'], 0], HV_COLORS.v110, 220, HV_COLORS.v220, 300, HV_COLORS.v300, 500, HV_COLORS.v500],
  ] as ExpressionSpecification
}

function hvWidth(): ExpressionSpecification {
  const byVoltage = (a: number, b: number, c: number) => ['step', ['to-number', ['get', 'voltage_kv'], 0], a, 300, b, 500, c] as ExpressionSpecification
  return zoomLinear(3, byVoltage(0.5, 0.8, 1.1), 7, byVoltage(0.9, 1.5, 2), 11, byVoltage(1.4, 2.4, 3.2), 15, byVoltage(2.2, 3.4, 4.5))
}

function pipelineColor(): ExpressionSpecification {
  const c: ExpressionSpecification = ['upcase', ['to-string', ['coalesce', ['get', 'commodity'], ['get', 'COMMODITY'], '']]]
  const has = (s: string): ExpressionSpecification => ['in', s, c]
  return [
    'case',
    has('HYDROGEN'), PIPELINE_COLORS.hydrogen,
    ['all', ['any', has('OIL'), has('PETROL'), has('CRUDE')], has('GAS')], PIPELINE_COLORS.mixed,
    ['any', has('GAS'), has('LNG'), has('LPG'), has('METHANE')], PIPELINE_COLORS.gas,
    ['any', has('OIL'), has('CRUDE'), has('PETROL'), has('FUEL'), has('PRODUCT'), has('CONDENSATE'), has('DIESEL')], PIPELINE_COLORS.oil,
    PIPELINE_COLORS.other,
  ] as ExpressionSpecification
}

function layerSpecs(theme: Theme, font: string[]): { layer: LayerSpecification; flag: keyof EnabledLayers; labels?: boolean }[] {
  const p = PALETTES[theme]
  const text = (field: ExpressionSpecification, size: number, color: string) => ({
    layout: {
      'text-field': field,
      'text-font': font,
      'text-size': size,
      'text-offset': [0, 0.9] as [number, number],
      'text-anchor': 'top' as const,
      'text-optional': true,
      'text-max-width': 9,
    },
    paint: { 'text-color': color, 'text-halo-color': p.halo, 'text-halo-width': 1.4, 'text-halo-blur': 0.4 },
  })
  return [
    // ── Networks (lines under everything else) ──
    {
      flag: 'powerGrid',
      layer: {
        id: 'wm-grid', type: 'line', source: 'wm-grid', 'source-layer': 'grid',
        layout: { 'line-cap': 'round', 'line-join': 'round' },
        paint: { 'line-color': p.grid, 'line-width': zoomLinear(2, 0.35, 6, 0.8, 10, 1.4), 'line-opacity': 0.75 },
      },
    },
    {
      flag: 'pipelines',
      layer: {
        id: 'wm-pipelines', type: 'line', source: 'wm-pipelines', 'source-layer': 'pipelines',
        layout: { 'line-join': 'round' },
        paint: { 'line-color': pipelineColor(), 'line-width': zoomLinear(2, 0.7, 6, 1.3, 10, 2.2, 14, 3.2), 'line-opacity': 0.95, 'line-dasharray': [2.5, 1.2] },
      },
    },
    {
      flag: 'hvLines',
      layer: {
        id: 'wm-hv-line', type: 'line', source: 'wm-hv', 'source-layer': 'hvlines', filter: ['!=', ['get', 'kind'], 'cable'],
        layout: { 'line-cap': 'round', 'line-join': 'round' },
        paint: { 'line-color': hvColor(), 'line-width': hvWidth(), 'line-opacity': 0.92 },
      },
    },
    {
      flag: 'hvLines',
      layer: {
        id: 'wm-hv-cable', type: 'line', source: 'wm-hv', 'source-layer': 'hvlines', filter: ['==', ['get', 'kind'], 'cable'],
        layout: { 'line-join': 'round' },
        paint: { 'line-color': hvColor(), 'line-width': hvWidth(), 'line-opacity': 0.85, 'line-dasharray': [2, 1.6] },
      },
    },
    // ── 3D buildings from the basemap's own vector tiles ──
    {
      flag: 'buildings3d',
      layer: {
        id: 'wm-buildings', type: 'fill-extrusion', source: 'carto', 'source-layer': 'building', minzoom: 14,
        paint: {
          'fill-extrusion-color': p.building,
          'fill-extrusion-height': ['coalesce', ['get', 'render_height'], 8],
          'fill-extrusion-base': ['coalesce', ['get', 'render_min_height'], 0],
          'fill-extrusion-opacity': 0.82,
        },
      },
    },
    // ── Points ──
    {
      flag: 'reactors',
      layer: {
        id: 'wm-reactors', type: 'circle', source: 'wm-reactors',
        paint: {
          'circle-radius': zoomLinear(
            1, ['interpolate', ['linear'], ['sqrt', ['get', 'capacity_mw']], 0, 2.2, 90, 6],
            6, ['interpolate', ['linear'], ['sqrt', ['get', 'capacity_mw']], 0, 3.5, 90, 11],
            11, ['interpolate', ['linear'], ['sqrt', ['get', 'capacity_mw']], 0, 6, 90, 16],
          ),
          'circle-color': ['match', ['get', 'status'], 'construction', p.plantConstruction, 'suspended', p.plantSuspended, p.plantOperational],
          'circle-opacity': 0.9,
          'circle-stroke-color': p.halo,
          'circle-stroke-width': 1,
        },
      },
    },
    {
      flag: 'seaports',
      layer: {
        id: 'wm-seaports-major', type: 'circle', source: 'wm-seaports', minzoom: 2,
        filter: ['in', ['get', 'size'], ['literal', ['large', 'medium']]],
        paint: {
          'circle-radius': zoomLinear(2, ['match', ['get', 'size'], 'large', 2.6, 1.8], 7, ['match', ['get', 'size'], 'large', 4.5, 3.5], 12, 6),
          'circle-color': p.port,
          'circle-stroke-color': p.halo,
          'circle-stroke-width': 1,
        },
      },
    },
    {
      flag: 'seaports',
      layer: {
        id: 'wm-seaports-minor', type: 'circle', source: 'wm-seaports', minzoom: 6,
        filter: ['!', ['in', ['get', 'size'], ['literal', ['large', 'medium']]]],
        paint: { 'circle-radius': zoomLinear(6, 1.8, 12, 4.5), 'circle-color': p.port, 'circle-opacity': 0.85, 'circle-stroke-color': p.halo, 'circle-stroke-width': 1 },
      },
    },
    {
      flag: 'airports',
      layer: {
        id: 'wm-airports-medium', type: 'circle', source: 'wm-airports', minzoom: 5.5,
        filter: ['!=', ['get', 'kind'], 'large'],
        paint: { 'circle-radius': zoomLinear(5.5, 1.8, 9, 3.5, 13, 5), 'circle-color': p.airportMedium, 'circle-stroke-color': p.halo, 'circle-stroke-width': 1 },
      },
    },
    {
      flag: 'airports',
      layer: {
        id: 'wm-airports-large', type: 'circle', source: 'wm-airports',
        filter: ['==', ['get', 'kind'], 'large'],
        paint: {
          'circle-radius': zoomLinear(1, 1.8, 5, 3, 9, 5, 13, 7),
          'circle-color': p.airportLarge,
          'circle-stroke-color': p.halo,
          'circle-stroke-width': zoomLinear(1, 0.6, 6, 1.4),
        },
      },
    },
    {
      flag: 'aton',
      layer: {
        id: 'wm-aton', type: 'circle', source: 'wm-aton', minzoom: 6,
        paint: {
          'circle-radius': zoomLinear(6, 2, 12, 4.5),
          'circle-color': ['case', ['get', 'off_position'], p.atonOff, ['get', 'virtual'], 'rgba(0,0,0,0)', p.atonPhysical],
          'circle-stroke-color': ['case', ['get', 'off_position'], p.atonOff, ['get', 'virtual'], p.atonVirtual, p.halo],
          'circle-stroke-width': ['case', ['get', 'virtual'], 1.6, 1],
        },
      },
    },
    // ── Labels (added on top so they win placement against basemap labels) ──
    {
      flag: 'airports',
      labels: true,
      layer: {
        id: 'wm-airports-label-large', type: 'symbol', source: 'wm-airports', minzoom: 5.5, filter: ['==', ['get', 'kind'], 'large'],
        ...text(['step', ['zoom'], ['coalesce', ['get', 'iata'], ['get', 'ident']], 10, ['get', 'name']] as ExpressionSpecification, 11, p.label),
      },
    },
    {
      flag: 'airports',
      labels: true,
      layer: {
        id: 'wm-airports-label-medium', type: 'symbol', source: 'wm-airports', minzoom: 9, filter: ['!=', ['get', 'kind'], 'large'],
        ...text(['step', ['zoom'], ['coalesce', ['get', 'iata'], ['get', 'ident']], 11, ['get', 'name']] as ExpressionSpecification, 10.5, p.labelMuted),
      },
    },
    {
      flag: 'seaports',
      labels: true,
      layer: { id: 'wm-seaports-label', type: 'symbol', source: 'wm-seaports', minzoom: 7, ...text(['get', 'name'], 10.5, p.labelMuted) },
    },
    {
      flag: 'reactors',
      labels: true,
      layer: { id: 'wm-reactors-label', type: 'symbol', source: 'wm-reactors', minzoom: 5, ...text(['get', 'name'], 10.5, p.labelMuted) },
    },
  ]
}

const STATIC_SOURCES: Record<string, StaticKey> = { 'wm-airports': 'airports', 'wm-seaports': 'seaports', 'wm-reactors': 'reactors', 'wm-aton': 'aton' }
const TILE_SOURCES: Record<string, string> = { 'wm-grid': 'power-grid', 'wm-pipelines': 'pipelines', 'wm-hv': 'hv-lines' }
const EMPTY: GeoJSON.FeatureCollection = { type: 'FeatureCollection', features: [] }

function ensureSource(map: MapLibreMap, id: string, spec: SourceSpecification) {
  if (!map.getSource(id)) map.addSource(id, spec)
}

let appliedTheme: Theme | null = null

export interface OverlayState {
  enabled: EnabledLayers
  caps: Capabilities | null
  theme: Theme
  trafficStyle: 'dark' | 'light'
}

/** Create missing sources/layers, push data and set visibility. Safe to call often. */
export function syncOverlays(map: MapLibreMap, state: OverlayState) {
  if (!map.isStyleLoaded() && !map.getStyle()?.layers?.length) return
  const before = firstSymbolLayer(map)
  const font = labelFont(map)

  // Theme change: drop our layers so they are recreated with the new palette.
  if (appliedTheme !== state.theme) {
    for (const spec of layerSpecs(state.theme, font)) if (map.getLayer(spec.layer.id)) map.removeLayer(spec.layer.id)
    appliedTheme = state.theme
  }

  for (const [id, key] of Object.entries(STATIC_SOURCES)) {
    ensureSource(map, id, { type: 'geojson', data: EMPTY, attribution: undefined })
    const source = map.getSource(id) as GeoJSONSource | undefined
    const data = getCollection(key)
    const tagged = source as unknown as { _wmData?: unknown }
    if (source && data && tagged._wmData !== data) {
      source.setData(data)
      tagged._wmData = data
    }
  }
  for (const [id, tileset] of Object.entries(TILE_SOURCES)) {
    const meta = state.caps?.tile_sources.find(t => t.id === tileset)
    if (!meta) continue
    // OpenStreetMap is already credited by the basemap; other sources are added.
    const attribution = meta.attribution && !/openstreetmap/i.test(meta.attribution) ? meta.attribution : undefined
    ensureSource(map, id, { type: 'vector', tiles: [`/tiles/${tileset}/{z}/{x}/{y}`], minzoom: meta.minzoom, maxzoom: meta.maxzoom, attribution })
  }
  const trafficSource = `wm-traffic-${state.trafficStyle}`
  if (state.caps?.traffic_configured) {
    ensureSource(map, trafficSource, {
      type: 'raster',
      tiles: [`/api/traffic/{z}/{x}/{y}?style=${state.trafficStyle}`],
      tileSize: 256,
      minzoom: 3,
      maxzoom: 18,
      attribution: '© TomTom',
    })
  }

  // Traffic raster sits lowest, right above the basemap roads.
  const trafficLayer = 'wm-traffic'
  const trafficOn = state.enabled.traffic && !!state.caps?.traffic_configured
  if (map.getLayer(trafficLayer) && (map.getLayer(trafficLayer) as unknown as { source: string }).source !== trafficSource) map.removeLayer(trafficLayer)
  if (trafficOn && !map.getLayer(trafficLayer)) {
    map.addLayer({ id: trafficLayer, type: 'raster', source: trafficSource, minzoom: 5, paint: { 'raster-opacity': 0.9, 'raster-fade-duration': 200 } }, before)
  }
  if (map.getLayer(trafficLayer)) map.setLayoutProperty(trafficLayer, 'visibility', trafficOn ? 'visible' : 'none')

  for (const { layer, flag, labels } of layerSpecs(state.theme, font)) {
    const source = 'source' in layer ? String(layer.source) : ''
    const tileset = TILE_SOURCES[source]
    const available = !tileset || !!state.caps?.tiles.includes(tileset)
    const visible = state.enabled[flag] && available
    if (!map.getLayer(layer.id)) {
      if (!visible || !map.getSource(source)) continue
      try {
        map.addLayer(layer, labels ? undefined : before)
      } catch (error) {
        console.warn(`Could not add layer ${layer.id}`, error)
        continue
      }
    }
    map.setLayoutProperty(layer.id, 'visibility', visible ? 'visible' : 'none')
  }
}

/** After a style reload every overlay must be recreated. */
export function resetOverlays() {
  appliedTheme = null
}
