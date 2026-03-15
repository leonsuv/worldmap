import { PathLayer } from '@deck.gl/layers'
import type { Layer } from '@deck.gl/core'

interface WindPoint {
  lon: number
  lat: number
  speed: number // m/s
  dir: number   // degrees, where wind is coming FROM
  gust?: number
  temperature?: number
  apparent_temperature?: number
  humidity?: number
  precipitation?: number
  weather_code?: number
  cloud_cover?: number
  pressure_msl?: number
  visibility?: number
  wave_height?: number
  wave_direction?: number
  wave_period?: number
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
  properties: Record<string, unknown>
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
      properties: {
        lat: +p.lat.toFixed(2),
        lon: +p.lon.toFixed(2),
        wind_speed_ms: +p.speed.toFixed(1),
        wind_direction_deg: +p.dir.toFixed(0),
        wind_gust_ms: p.gust != null ? +p.gust.toFixed(1) : undefined,
        temperature_c: p.temperature != null ? +p.temperature.toFixed(1) : undefined,
        apparent_temperature_c: p.apparent_temperature != null ? +p.apparent_temperature.toFixed(1) : undefined,
        relative_humidity_pct: p.humidity != null ? +p.humidity.toFixed(0) : undefined,
        precipitation_mm: p.precipitation != null ? +p.precipitation.toFixed(2) : undefined,
        cloud_cover_pct: p.cloud_cover != null ? +p.cloud_cover.toFixed(0) : undefined,
        pressure_msl_hpa: p.pressure_msl != null ? +p.pressure_msl.toFixed(1) : undefined,
        visibility_m: p.visibility != null ? +p.visibility.toFixed(0) : undefined,
        weather_code: p.weather_code,
        wave_height_m: p.wave_height != null ? +p.wave_height.toFixed(2) : undefined,
        wave_direction_deg: p.wave_direction != null ? +p.wave_direction.toFixed(0) : undefined,
        wave_period_s: p.wave_period != null ? +p.wave_period.toFixed(1) : undefined,
      },
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
      pickable: true,
      opacity: 0.95,
    }),
  ]
}
