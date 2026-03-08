import { IconLayer } from '@deck.gl/layers'

const ATON_ATLAS = createAtoNAtlas()
const ATON_MAPPING = {
  buoy: { x: 0, y: 0, width: 64, height: 64, anchorY: 32, mask: true },
  virtual: { x: 64, y: 0, width: 64, height: 64, anchorY: 32, mask: true },
}

export function buildAtoNLayer(fc: GeoJSON.FeatureCollection): IconLayer {
  return new IconLayer({
    id: 'aton',
    data: fc.features,
    getPosition: (d: GeoJSON.Feature) => (d.geometry as GeoJSON.Point).coordinates as [number, number],
    getIcon: (d: GeoJSON.Feature) => {
      const p = d.properties ?? {}
      return p.virtual ? 'virtual' : 'buoy'
    },
    getSize: 14,
    getColor: (d: GeoJSON.Feature) => {
      const p = d.properties ?? {}
      if (p.off_position) return [255, 80, 80, 230]   // red if off position
      if (p.virtual) return [180, 120, 255, 200]       // purple for virtual
      return [255, 200, 50, 220]                       // yellow for physical
    },
    iconAtlas: ATON_ATLAS,
    iconMapping: ATON_MAPPING,
    sizeScale: 1,
    pickable: true,
  })
}

function createAtoNAtlas(): string {
  const c = document.createElement('canvas')
  c.width = 128; c.height = 64
  const ctx = c.getContext('2d')!

  // Buoy icon (physical)
  ctx.strokeStyle = 'white'
  ctx.fillStyle = 'white'
  ctx.lineWidth = 3
  ctx.lineCap = 'round'
  // Diamond shape
  ctx.beginPath()
  ctx.moveTo(32, 10); ctx.lineTo(48, 32); ctx.lineTo(32, 54); ctx.lineTo(16, 32)
  ctx.closePath()
  ctx.stroke()
  // Center dot
  ctx.beginPath(); ctx.arc(32, 32, 4, 0, Math.PI * 2); ctx.fill()

  // Virtual AtoN icon (dashed diamond)
  ctx.setLineDash([4, 4])
  ctx.beginPath()
  ctx.moveTo(96, 10); ctx.lineTo(112, 32); ctx.lineTo(96, 54); ctx.lineTo(80, 32)
  ctx.closePath()
  ctx.stroke()
  ctx.setLineDash([])
  // Center ring
  ctx.beginPath(); ctx.arc(96, 32, 4, 0, Math.PI * 2); ctx.stroke()

  return c.toDataURL()
}
