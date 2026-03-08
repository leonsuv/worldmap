import { LineLayer } from '@deck.gl/layers'

interface WindPoint {
  lon: number
  lat: number
  speed: number // m/s
  dir: number   // degrees, where wind is coming FROM
}

export function buildWeatherLayer(data: WindPoint[]): LineLayer {
  // Convert wind points into line segments: origin → tip in the direction wind blows TO
  const lines = data.map((p) => {
    const len = Math.min(p.speed * 0.15, 3) // scale to degrees
    const rad = ((p.dir + 180) % 360) * (Math.PI / 180) // direction wind blows TO
    return {
      from: [p.lon, p.lat],
      to: [p.lon + len * Math.sin(rad), p.lat + len * Math.cos(rad)],
      speed: p.speed,
    }
  })

  return new LineLayer({
    id: 'weather-wind',
    data: lines,
    getSourcePosition: (d) => d.from as [number, number],
    getTargetPosition: (d) => d.to as [number, number],
    getColor: (d) => {
      const s = d.speed as number
      if (s < 5) return [100, 200, 255, 180]
      if (s < 10) return [50, 150, 255, 200]
      if (s < 15) return [255, 200, 50, 220]
      return [255, 80, 50, 240]
    },
    getWidth: (d) => Math.max(1, Math.min((d.speed as number) / 3, 5)),
    widthUnits: 'pixels',
    pickable: false,
  })
}
