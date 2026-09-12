import { IconLayer, TextLayer } from '@deck.gl/layers'
import type { Layer } from '@deck.gl/core'

interface WindPoint {
  lon: number; lat: number; speed: number; dir: number; gust?: number; temperature?: number
  apparent_temperature?: number; humidity?: number; precipitation?: number; weather_code?: number
  cloud_cover?: number; pressure_msl?: number; visibility?: number
}
const ARROW = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><path d="M32 54V10M17 25 32 10 47 25" fill="none" stroke="white" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/></svg>')
const MAPPING = { arrow: { x: 0, y: 0, width: 64, height: 64, anchorY: 32, mask: true } }
function speedColor(speed: number): [number, number, number, number] {
  if (speed < 4) return [117, 203, 226, 230]
  if (speed < 8) return [105, 160, 245, 240]
  if (speed < 12) return [244, 204, 104, 245]
  if (speed < 17) return [245, 160, 88, 250]
  return [243, 116, 105, 255]
}
export function buildWeatherLayer(points: WindPoint[]): Layer[] {
  const data = points.map(p => ({ ...p, properties: {
    latitude: p.lat, longitude: p.lon,
    wind_speed_ms: +p.speed.toFixed(1), wind_from_degrees: Math.round(p.dir),
    gust_ms: p.gust, temperature_c: p.temperature, feels_like_c: p.apparent_temperature,
    humidity_percent: p.humidity, precipitation_mm: p.precipitation,
    pressure_hpa: p.pressure_msl, cloud_cover_percent: p.cloud_cover,
  } }))
  return [
    new IconLayer({
      id: 'weather-wind-arrows', data, iconAtlas: ARROW, iconMapping: MAPPING,
      getPosition: (p: WindPoint) => [p.lon, p.lat], getIcon: () => 'arrow',
      getSize: (p: WindPoint) => Math.min(38, 24 + p.speed * .5),
      getAngle: (p: WindPoint) => -(p.dir + 180), getColor: (p: WindPoint) => speedColor(p.speed), pickable: true,
    }),
    new TextLayer({
      id: 'weather-temperature', data, getPosition: (p: WindPoint) => [p.lon, p.lat],
      getText: (p: WindPoint) => p.temperature === undefined ? '' : `${Math.round(p.temperature)}°`,
      getColor: [231, 240, 243, 255], getSize: 12, getPixelOffset: [0, 25],
      fontFamily: 'system-ui', fontWeight: 600, background: true,
      getBackgroundColor: [17, 31, 40, 215], backgroundPadding: [5, 3],
      pickable: false,
    }),
  ]
}
