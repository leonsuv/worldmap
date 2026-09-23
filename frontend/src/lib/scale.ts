/**
 * Piecewise-linear value by zoom, clamped at both ends — the same idea as
 * MapLibre's `["interpolate", ["linear"], ["zoom"], ...]` for deck.gl sizes.
 */
export function zoomRamp(zoom: number, stops: readonly (readonly [number, number])[]): number {
  if (!stops.length) return 0
  if (zoom <= stops[0][0]) return stops[0][1]
  for (let i = 1; i < stops.length; i++) {
    const [z1, v1] = stops[i]
    if (zoom <= z1) {
      const [z0, v0] = stops[i - 1]
      return v0 + ((zoom - z0) / (z1 - z0)) * (v1 - v0)
    }
  }
  return stops[stops.length - 1][1]
}

/** Quantise zoom so derived layers rebuild a few times per zoom level, not every frame. */
export function zoomBucket(zoom: number, steps = 4): number {
  return Math.round(zoom * steps) / steps
}

export const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v))

/** Mix two hex colours (0..1) into an RGBA tuple. */
export function hexToRgba(hex: string, alpha = 255): [number, number, number, number] {
  const h = hex.replace('#', '')
  const n = parseInt(h.length === 3 ? h.split('').map(c => c + c).join('') : h, 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha]
}

/** Colour on a multi-stop ramp for t in 0..1. */
export function rampColor(stops: readonly string[], t: number, alpha = 255): [number, number, number, number] {
  const x = clamp(t, 0, 1) * (stops.length - 1)
  const i = Math.min(Math.floor(x), stops.length - 2)
  const f = x - i
  const a = hexToRgba(stops[i])
  const b = hexToRgba(stops[i + 1])
  return [Math.round(a[0] + (b[0] - a[0]) * f), Math.round(a[1] + (b[1] - a[1]) * f), Math.round(a[2] + (b[2] - a[2]) * f), alpha]
}
