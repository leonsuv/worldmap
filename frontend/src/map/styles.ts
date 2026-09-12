export const MAP_STYLES = {
  dark: 'https://basemaps.cartocdn.com/gl/dark-matter-gl-style/style.json',
  light: 'https://basemaps.cartocdn.com/gl/positron-gl-style/style.json',
} as const


/** A quiet atlas palette: make infrastructure colors legible on land and water. */
export function styleAtlas(map: import('maplibre-gl').Map) {
  if (!map.getSource('carto') || !map.getStyle().name?.toLowerCase().includes('dark')) return
  const colors: Record<string, ['background-color' | 'fill-color' | 'line-color', string]> = {
    background: ['background-color', '#26343b'],
    water: ['fill-color', '#111f28'],
    landcover: ['fill-color', '#26343b'],
    landuse: ['fill-color', '#27383b'],
    park_national_park: ['fill-color', '#273b37'],
    park_nature_reserve: ['fill-color', '#273b37'],
    boundary_country_outline: ['line-color', '#17242c'],
    boundary_country_inner: ['line-color', '#526167'],
  }
  for (const [id, [property, color]] of Object.entries(colors)) {
    if (map.getLayer(id)) map.setPaintProperty(id, property, color)
  }
}
