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
    getFillColor: [255, 220, 50, 200],
    getLineColor: [255, 180, 0, 255],
    lineWidthMinPixels: 1,
    stroked: true,
    radiusUnits: 'meters',
    radiusMinPixels: 4,
    radiusMaxPixels: 30,
    pickable: true,
  })
}
