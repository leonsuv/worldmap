import { describe, it, expect } from 'vitest'
import { weatherSamplePoints } from '../src/layers/weatherSampling'

describe('weather sampling', () => {
  it('covers every latitude band rather than truncating the north', () => {
    const points = weatherSamplePoints([-180, -80, 180, 80], 2)
    expect(points).toHaveLength(16)
    expect(new Set(points.map(p => p.lat))).toEqual(new Set([-60, -20, 20, 60]))
  })
  it('wraps the antimeridian while sampling the visible area', () => {
    const points = weatherSamplePoints([170, -20, 210, 20], 6)
    expect(points).toHaveLength(24)
    expect(points.some(p => p.lon < -150)).toBe(true)
    expect(points.some(p => p.lon > 170)).toBe(true)
    expect(points.every(p => Math.abs(p.lon) <= 180)).toBe(true)
  })
  it('bounds requests and rejects invalid viewports', () => {
    expect(weatherSamplePoints([-720, -90, 720, 90], 9)).toHaveLength(24)
    expect(weatherSamplePoints([0, 0, 0, 0], 3)).toEqual([])
    expect(weatherSamplePoints([NaN, -20, 40, 20], 3)).toEqual([])
  })
})
