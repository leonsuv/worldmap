import { api, isAbort } from '../lib/api'
import { setFeed } from '../store/status'
import { requestRedraw } from '../map/scheduler'

export interface WeatherPoint {
  lat: number
  lon: number
  temperature: number | null
  apparent_temperature: number | null
  humidity: number | null
  precipitation: number | null
  weather_code: number | null
  cloud_cover: number | null
  pressure: number | null
  wind_speed: number | null
  wind_direction: number | null
  wind_gusts: number | null
  is_day: number | null
  time: string | null
}

let points: WeatherPoint[] = []
let controller: AbortController | null = null
let timer: ReturnType<typeof setTimeout> | undefined
let lastKey = ''

export const getWeather = () => points

export function bboxParam(bbox: [number, number, number, number]): string {
  const [w, s, e, n] = bbox
  const round = (v: number) => Math.round(v * 100) / 100
  return [round(w), round(Math.max(-85, s)), round(e), round(Math.min(85, n))].join(',')
}

/** Request the weather grid for a viewport (debounced; stale requests are cancelled). */
export function requestWeather(bbox: [number, number, number, number]) {
  clearTimeout(timer)
  timer = setTimeout(async () => {
    const key = bboxParam(bbox)
    if (key === lastKey && points.length) return
    controller?.abort()
    const ac = new AbortController()
    controller = ac
    if (!points.length) setFeed('weather', { state: 'loading' })
    try {
      const data = await api<{ points: WeatherPoint[]; stale: boolean; fetched_at: number }>(`/api/weather/grid?bbox=${key}`, { signal: ac.signal })
      if (ac.signal.aborted) return
      lastKey = key
      points = data.points.filter(p => p.wind_speed != null || p.temperature != null)
      setFeed('weather', {
        state: 'ready',
        count: points.length,
        updatedAt: data.fetched_at,
        message: data.stale ? 'Some values are from the last successful update' : undefined,
      })
      requestRedraw()
    } catch (error) {
      if (isAbort(error)) return
      setFeed('weather', { state: 'error', count: points.length, message: error instanceof Error ? error.message : 'Weather unavailable' })
    }
  }, 400)
}

export function stopWeather() {
  clearTimeout(timer)
  controller?.abort()
  points = []
  lastKey = ''
  setFeed('weather', null)
  requestRedraw()
}
