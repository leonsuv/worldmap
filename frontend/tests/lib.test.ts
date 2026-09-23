import { describe, expect, it } from 'vitest'
import { hexToRgba, rampColor, zoomBucket, zoomRamp } from '../src/lib/scale'
import { compass, fmtAltitude, fmtCount, fmtLon, fmtNumber, fmtSpeedMs, timeAgo } from '../src/lib/format'
import { isStationary, shipCategory, shipTypeLabel } from '../src/lib/ais'
import { altitudeColor } from '../src/lib/aircraft'
import { weatherLabel } from '../src/lib/weather'
import { reportMarkdown } from '../src/lib/report'
import { createRenderScheduler, memo } from '../src/map/scheduler'

describe('zoom-dependent sizing', () => {
  const stops = [[2, 4], [6, 12], [10, 20]] as const
  it('interpolates between stops and clamps outside them', () => {
    expect(zoomRamp(0, stops)).toBe(4)
    expect(zoomRamp(4, stops)).toBe(8)
    expect(zoomRamp(8, stops)).toBe(16)
    expect(zoomRamp(15, stops)).toBe(20)
    expect(zoomRamp(3, [])).toBe(0)
  })
  it('quantises zoom so layers are not rebuilt every frame', () => {
    expect(zoomBucket(5.13)).toBe(5.25)
    expect(zoomBucket(5.1)).toBe(5)
  })
  it('builds colours from hex ramps', () => {
    expect(hexToRgba('#ff8000')).toEqual([255, 128, 0, 255])
    expect(hexToRgba('#f80', 10)).toEqual([255, 136, 0, 10])
    expect(rampColor(['#000000', '#ffffff'], 0.5)).toEqual([128, 128, 128, 255])
    expect(rampColor(['#000000', '#ffffff'], 7)).toEqual([255, 255, 255, 255])
  })
})

describe('formatting', () => {
  it('writes readable relative times', () => {
    const now = 1_000_000_000_000
    expect(timeAgo(now / 1000 - 10, now)).toBe('just now')
    expect(timeAgo(now / 1000 - 600, now)).toBe('10 min ago')
    expect(timeAgo(now / 1000 - 7200, now)).toBe('2 h ago')
    expect(timeAgo(null, now)).toBe('—')
  })
  it('formats aviation and marine units', () => {
    expect(fmtAltitude(10972.8)).toBe('10,973 m · FL360')
    expect(fmtAltitude(150)).toBe('150 m')
    expect(fmtSpeedMs(250)).toBe('900 km/h · 486 kn')
    expect(fmtNumber(null)).toBe('—')
    expect(fmtCount(12_345)).toBe('12k')
    expect(fmtCount(2_500_000)).toBe('2.5M')
  })
  it('wraps longitudes and names directions', () => {
    expect(fmtLon(190)).toBe('170.00° W')
    expect(fmtLon(-10)).toBe('10.00° W')
    expect(compass(0)).toBe('N')
    expect(compass(225)).toBe('SW')
    expect(compass(359)).toBe('N')
  })
})

describe('AIS and aircraft classification', () => {
  it('groups ship types into categories', () => {
    expect(shipCategory(70)).toBe('cargo')
    expect(shipCategory(84)).toBe('tanker')
    expect(shipCategory(52)).toBe('special')
    expect(shipCategory(37)).toBe('pleasure')
    expect(shipCategory(null)).toBe('unknown')
    expect(shipCategory(null, true)).toBe('sar')
    expect(shipCategory(99)).toBe('other')
  })
  it('labels types including hazard classes', () => {
    expect(shipTypeLabel(71)).toBe('Cargo (hazard A)')
    expect(shipTypeLabel(52)).toBe('Tug')
    expect(shipTypeLabel(7)).toBe('Type 7')
    expect(shipTypeLabel(null)).toBe('Type not reported')
  })
  it('treats moored, anchored and slow vessels as stationary', () => {
    expect(isStationary(12, 5)).toBe(true)
    expect(isStationary(0.2, 0)).toBe(true)
    expect(isStationary(null, null)).toBe(true)
    expect(isStationary(8, 0)).toBe(false)
  })
  it('colours aircraft by altitude and greys out aircraft on the ground', () => {
    expect(altitudeColor(null, true)[0]).toBe(150)
    expect(altitudeColor(0, false)).not.toEqual(altitudeColor(12000, false))
  })
  it('names WMO weather codes', () => {
    expect(weatherLabel(0)).toBe('Clear sky')
    expect(weatherLabel(63)).toBe('Rain')
    expect(weatherLabel(999)).toBe('Now')
  })
})

describe('render scheduling', () => {
  it('coalesces bursts into one frame and schedules nothing while idle', () => {
    let callback: FrameRequestCallback | undefined
    let requests = 0
    let renders = 0
    const scheduler = createRenderScheduler(() => renders++, fn => ((callback = fn), ++requests), () => {})
    scheduler.start()
    for (let i = 0; i < 1000; i++) scheduler.invalidate()
    expect(requests).toBe(1)
    callback!(0)
    expect(renders).toBe(1)
    expect(requests).toBe(1)
    scheduler.stop()
    scheduler.invalidate()
    expect(requests).toBe(1)
  })
  it('memoises by argument identity', () => {
    let builds = 0
    const build = memo((data: number[], zoom: number) => ({ data, zoom, n: ++builds }))
    const data = [1]
    const first = build(data, 3)
    expect(build(data, 3)).toBe(first)
    expect(build(data, 4)).not.toBe(first)
    expect(build([1], 4).n).toBe(3)
  })
})

describe('situation report', () => {
  it('renders events and totals as Markdown', () => {
    const md = reportMarkdown({
      generated_at: 1_790_000_000,
      vessels_tracked: 12_000,
      aircraft_tracked: null,
      airports: 1,
      seaports: 1,
      nuclear_plants: 1,
      unacknowledged_alerts: 2,
      watchlist_items: 3,
      active_events: [
        { name: 'Storm', event_type: 'storm', lat: 50, lon: 8, radius_km: 100, description: 'Gale warning', started_at: 1_790_000_000, vessels: 4, aircraft: 5, airports: 6, seaports: 7, nuclear_plants: 0 },
      ],
    })
    expect(md).toContain('| Vessels tracked | 12,000 |')
    expect(md).toContain('| Aircraft tracked | — |')
    expect(md).toContain('### Storm (storm)')
    expect(md).toContain('4 vessels, 5 aircraft, 6 airports, 7 seaports')
    expect(md).toContain('- Gale warning')
  })
})
