import { IconLayer, PathLayer } from '@deck.gl/layers'

const ICON_ATLAS = createAirplaneAtlas()
const ICON_MAPPING = { airplane: { x: 0, y: 0, width: 64, height: 64, anchorY: 32, mask: true } }

function altitudeColor(alt: number | null): [number, number, number, number] {
  if (alt == null) return [100, 100, 100, 200]
  const clamped = Math.min(Math.max(alt, 0), 13000)
  const t = clamped / 13000
  const r = Math.round(t < 0.5 ? t * 2 * 255 : 255)
  const g = Math.round(t < 0.5 ? 255 : (1 - (t - 0.5) * 2) * 255)
  return [r, g, 0, 220]
}

// Reusable typed-array buffer — avoids GC pressure on every render frame
let posBuf = new Float32Array(0)

function ensurePositions(features: GeoJSON.Feature[]): Float32Array {
  const needed = features.length * 2
  if (posBuf.length < needed) posBuf = new Float32Array(needed)
  for (let i = 0; i < features.length; i++) {
    const coords = (features[i].geometry as GeoJSON.Point).coordinates
    posBuf[i * 2] = coords[0]
    posBuf[i * 2 + 1] = coords[1]
  }
  return posBuf.subarray(0, needed)
}

export function buildFlightsLayer(fc: GeoJSON.FeatureCollection): IconLayer {
  const positions = ensurePositions(fc.features)
  return new IconLayer({
    id: 'flights',
    data: fc.features,
    getPosition: (_d: GeoJSON.Feature, { index }: { index: number }) =>
      [positions[index * 2], positions[index * 2 + 1]] as [number, number],
    getIcon: () => 'airplane',
    getSize: 20,
    getAngle: (d: GeoJSON.Feature) => -(d.properties?.true_track ?? 0),
    getColor: (d: GeoJSON.Feature) => altitudeColor(d.properties?.baro_altitude),
    iconAtlas: ICON_ATLAS,
    iconMapping: ICON_MAPPING,
    sizeScale: 1,
    pickable: true,
  })
}

/** Generates a tiny 64×64 canvas with a simple airplane shape */
function createAirplaneAtlas(): string {
  const c = document.createElement('canvas')
  c.width = 64; c.height = 64
  const ctx = c.getContext('2d')!
  ctx.fillStyle = 'white'
  ctx.beginPath()
  // fuselage
  ctx.moveTo(32, 4); ctx.lineTo(36, 28); ctx.lineTo(32, 60); ctx.lineTo(28, 28); ctx.closePath()
  ctx.fill()
  // wings
  ctx.beginPath()
  ctx.moveTo(32, 24); ctx.lineTo(58, 36); ctx.lineTo(54, 38); ctx.lineTo(32, 30)
  ctx.lineTo(10, 38); ctx.lineTo(6, 36); ctx.closePath()
  ctx.fill()
  // tail
  ctx.beginPath()
  ctx.moveTo(32, 50); ctx.lineTo(42, 58); ctx.lineTo(40, 60); ctx.lineTo(32, 54)
  ctx.lineTo(24, 60); ctx.lineTo(22, 58); ctx.closePath()
  ctx.fill()
  return c.toDataURL()
}

/** Renders a live aircraft track as a gradient path (green→yellow→red by altitude) */
export function buildTrackLayer(track: [number, number, number][]): PathLayer {
  // Build path segments colored by altitude
  const segments = track.slice(0, -1).map((pt, i) => ({
    path: [pt.slice(0, 2), track[i + 1].slice(0, 2)] as [number, number][],
    color: altitudeColor(pt[2]),
  }))

  return new PathLayer({
    id: 'flight-track',
    data: segments,
    getPath: (d: { path: [number, number][] }) => d.path,
    getColor: (d: { color: [number, number, number, number] }) => d.color,
    getWidth: 3,
    widthUnits: 'pixels',
    jointRounded: true,
    capRounded: true,
    pickable: false,
  })
}
