import { PathLayer } from '@deck.gl/layers'
import type { Layer } from '@deck.gl/core'

interface WindPoint {
  lon: number
  lat: number
  speed: number // m/s
  dir: number   // degrees, where wind is coming FROM
}

function speedColor(speed: number): [number, number, number, number] {
  if (speed < 4) return [72, 181, 255, 170]
  if (speed < 8) return [46, 134, 255, 190]
  if (speed < 12) return [255, 202, 66, 215]
  if (speed < 17) return [255, 134, 36, 230]
  return [255, 76, 59, 245]
}

type WeatherGlyph = {
  path: [number, number][]
  speed: number
}

export function buildWeatherLayer(data: WindPoint[]): Layer[] {
  const glyphs: WeatherGlyph[] = data.map((p) => {
    // Direction conversion: meteorological direction (FROM) -> flow direction (TO)
    const rad = ((p.dir + 180) % 360) * (Math.PI / 180)

    // Keep arrows geographically compact to avoid map clutter.
    const shaft = Math.max(0.25, Math.min(1.15, 0.24 + p.speed * 0.055))
    const head = Math.max(0.08, shaft * 0.24)
    const spread = Math.PI / 6.5

    const sx = p.lon
    const sy = p.lat
    const tx = p.lon + shaft * Math.sin(rad)
    const ty = p.lat + shaft * Math.cos(rad)

    const lwx = tx - head * Math.sin(rad - spread)
    const lwy = ty - head * Math.cos(rad - spread)
    const rwx = tx - head * Math.sin(rad + spread)
    const rwy = ty - head * Math.cos(rad + spread)

    return {
      // Draw shaft and both arrowhead wings as one polyline.
      path: [
        [sx, sy],
        [tx, ty],
        [lwx, lwy],
        [tx, ty],
        [rwx, rwy],
      ],
      speed: p.speed,
    }
  })

  return [
    new PathLayer({
      id: 'weather-wind-arrows',
      data: glyphs,
      getPath: (d) => d.path,
      getColor: (d) => speedColor(d.speed),
      getWidth: (d) => Math.max(1.2, Math.min(5.2, d.speed * 0.22)),
      widthUnits: 'pixels',
      capRounded: true,
      jointRounded: true,
      pickable: false,
      opacity: 0.95,
    }),
  ]
}
