import { IconLayer } from '@deck.gl/layers'

const SEAPORT_ATLAS = createAnchorAtlas()
const SEAPORT_MAPPING = { anchor: { x: 0, y: 0, width: 64, height: 64, anchorY: 32, mask: true } }

export function buildSeaportsLayer(fc: GeoJSON.FeatureCollection): IconLayer {
  return new IconLayer({
    id: 'seaports',
    data: fc.features,
    getPosition: (d: GeoJSON.Feature) => (d.geometry as GeoJSON.Point).coordinates as [number, number],
    getIcon: () => 'anchor',
    getSize: 16,
    getColor: [40, 180, 160, 220],
    iconAtlas: SEAPORT_ATLAS,
    iconMapping: SEAPORT_MAPPING,
    sizeScale: 1,
    pickable: true,
  })
}

function createAnchorAtlas(): string {
  const c = document.createElement('canvas')
  c.width = 64; c.height = 64
  const ctx = c.getContext('2d')!
  ctx.strokeStyle = 'white'
  ctx.lineWidth = 4
  ctx.lineCap = 'round'
  // Ring at top
  ctx.beginPath(); ctx.arc(32, 14, 8, 0, Math.PI * 2); ctx.stroke()
  // Vertical shaft
  ctx.beginPath(); ctx.moveTo(32, 22); ctx.lineTo(32, 54); ctx.stroke()
  // Cross bar
  ctx.beginPath(); ctx.moveTo(18, 34); ctx.lineTo(46, 34); ctx.stroke()
  // Bottom curve (anchor flukes)
  ctx.beginPath()
  ctx.arc(32, 42, 18, 0, Math.PI)
  ctx.stroke()
  return c.toDataURL()
}
