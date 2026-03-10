import { ScatterplotLayer } from '@deck.gl/layers'
import type { HistoryPoint } from '../store/history'

export function buildHistoryShipsLayer(positions: HistoryPoint[]): ScatterplotLayer {
  return new ScatterplotLayer({
    id: 'history-ships',
    data: positions,
    getPosition: (d: HistoryPoint) => [d.lon, d.lat],
    getRadius: 4,
    radiusUnits: 'pixels' as const,
    getFillColor: [255, 180, 50, 200],
    getLineColor: [255, 220, 100, 255],
    stroked: true,
    lineWidthMinPixels: 1,
    pickable: false,
  })
}
