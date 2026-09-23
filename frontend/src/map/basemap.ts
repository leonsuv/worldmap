import type { Map as MapLibreMap } from 'maplibre-gl'
import type { Theme } from '../store/ui'

export const BASEMAPS: Record<Theme, string> = {
  dark: 'https://basemaps.cartocdn.com/gl/dark-matter-gl-style/style.json',
  light: 'https://basemaps.cartocdn.com/gl/positron-gl-style/style.json',
}

/** Id of the first label layer: overlays are inserted below it so place names stay readable. */
export function firstSymbolLayer(map: MapLibreMap): string | undefined {
  return map.getStyle()?.layers?.find(l => l.type === 'symbol')?.id
}

/** A font stack the basemap's glyph server actually has. */
export function labelFont(map: MapLibreMap): string[] {
  const stacks: string[][] = []
  for (const layer of map.getStyle()?.layers ?? []) {
    if (layer.type !== 'symbol') continue
    const font = (layer.layout as Record<string, unknown> | undefined)?.['text-font']
    if (Array.isArray(font) && font.length && font.every(f => typeof f === 'string')) stacks.push(font as string[])
  }
  // Upright text reads better for points than the italic water-label fonts.
  const upright = stacks.filter(s => !s.some(f => /italic/i.test(f)))
  return upright.find(s => s.some(f => /regular|medium/i.test(f))) ?? upright[0] ?? stacks[0] ?? ['Open Sans Regular']
}

/** Calmer land/water contrast for the dark basemap so overlay colours read clearly. */
export function tuneBasemap(map: MapLibreMap, theme: Theme) {
  if (theme !== 'dark') return
  const colors: Record<string, [string, string]> = {
    background: ['background-color', '#1f2c33'],
    water: ['fill-color', '#0e1a21'],
    landcover: ['fill-color', '#22313a'],
    landuse: ['fill-color', '#233238'],
    park_national_park: ['fill-color', '#23352f'],
    park_nature_reserve: ['fill-color', '#23352f'],
    boundary_country_outline: ['line-color', '#12202a'],
    boundary_country_inner: ['line-color', '#4b5b63'],
  }
  for (const [id, [property, color]] of Object.entries(colors)) {
    if (!map.getLayer(id)) continue
    try {
      map.setPaintProperty(id, property as 'fill-color', color)
    } catch {
      /* layer type differs in a future basemap version */
    }
  }
}
