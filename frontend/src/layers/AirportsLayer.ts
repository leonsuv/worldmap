import { IconLayer } from '@deck.gl/layers'

const AIRPORT_ATLAS = createAirportAtlas()
const AIRPORT_MAPPING = { airport: { x: 0, y: 0, width: 64, height: 64, anchorY: 32, mask: true } }

export function buildAirportsLayer(fc: GeoJSON.FeatureCollection): IconLayer {
  return new IconLayer({
    id: 'airports',
    data: fc.features,
    getPosition: (d: GeoJSON.Feature) => (d.geometry as GeoJSON.Point).coordinates as [number, number],
    getIcon: () => 'airport',
    getSize: 18,
    getColor: [70, 130, 220, 230],
    iconAtlas: AIRPORT_ATLAS,
    iconMapping: AIRPORT_MAPPING,
    sizeScale: 1,
    pickable: true,
  })
}

function createAirportAtlas(): string {
  const c = document.createElement('canvas')
  c.width = 64; c.height = 64
  const ctx = c.getContext('2d')!
  ctx.fillStyle = 'white'
  // Control tower shape
  ctx.fillRect(28, 20, 8, 34)
  ctx.fillRect(16, 12, 32, 12)
  // dome
  ctx.beginPath()
  ctx.arc(32, 12, 16, Math.PI, 0)
  ctx.fill()
  return c.toDataURL()
}
