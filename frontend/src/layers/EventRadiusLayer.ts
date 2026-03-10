import { ScatterplotLayer } from '@deck.gl/layers'
import type { EventItem } from '../store/events'

const EVENT_COLORS: Record<string, [number, number, number, number]> = {
  storm:        [244, 67, 54, 60],
  outage:       [255, 152, 0, 60],
  closure:      [156, 39, 176, 60],
  geopolitical: [233, 30, 99, 60],
  custom:       [33, 150, 243, 60],
}

const BORDER_COLORS: Record<string, [number, number, number, number]> = {
  storm:        [244, 67, 54, 180],
  outage:       [255, 152, 0, 180],
  closure:      [156, 39, 176, 180],
  geopolitical: [233, 30, 99, 180],
  custom:       [33, 150, 243, 180],
}

export function buildEventRadiusLayers(events: EventItem[]): ScatterplotLayer[] {
  const active = events.filter(e => e.active)
  if (active.length === 0) return []

  // Fill layer
  const fill = new ScatterplotLayer({
    id: 'event-radius-fill',
    data: active,
    getPosition: (d: EventItem) => [d.lon, d.lat],
    getRadius: (d: EventItem) => d.radius_km * 1000, // meters
    getFillColor: (d: EventItem) => EVENT_COLORS[d.event_type] ?? [100, 100, 100, 60],
    radiusUnits: 'meters' as const,
    filled: true,
    stroked: false,
    pickable: false,
  })

  // Border layer
  const border = new ScatterplotLayer({
    id: 'event-radius-border',
    data: active,
    getPosition: (d: EventItem) => [d.lon, d.lat],
    getRadius: (d: EventItem) => d.radius_km * 1000,
    getLineColor: (d: EventItem) => BORDER_COLORS[d.event_type] ?? [100, 100, 100, 180],
    radiusUnits: 'meters' as const,
    filled: false,
    stroked: true,
    lineWidthMinPixels: 2,
    pickable: false,
  })

  return [fill, border]
}
