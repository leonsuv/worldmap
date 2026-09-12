import { ScatterplotLayer } from '@deck.gl/layers'

export function buildReactorsLayer(fc: GeoJSON.FeatureCollection): ScatterplotLayer {
  return new ScatterplotLayer({
    id: 'reactors',
    data: fc.features,
    getPosition: (d: GeoJSON.Feature) => (d.geometry as GeoJSON.Point).coordinates as [number, number],
    getRadius: (d: GeoJSON.Feature) => {
      const mw = d.properties?.capacity_mw ?? 500
      return Math.max(3000, Math.sqrt(mw) * 500)
    },
    getFillColor: [222, 214, 139, 205],
    getLineColor: [255, 242, 193, 255],
    lineWidthMinPixels: 1,
    stroked: true,
    radiusUnits: 'meters',
    radiusMinPixels: 3,
    radiusMaxPixels: 12,
    pickable: true,
  })
}
