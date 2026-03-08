import { PathLayer } from '@deck.gl/layers'

interface TrafficSegment {
  coordinates: [number, number][]
  color: [number, number, number]
}

export function buildTrafficLayer(data: TrafficSegment[]): PathLayer {
  return new PathLayer({
    id: 'traffic',
    data,
    getPath: (d) => d.coordinates,
    getColor: (d) => [...d.color, 220] as [number, number, number, number],
    getWidth: 4,
    widthUnits: 'pixels',
    widthMinPixels: 2,
    pickable: false,
    jointRounded: true,
    capRounded: true,
  })
}
