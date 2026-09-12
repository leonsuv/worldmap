export function weatherSamplePoints(bbox: [number, number, number, number], zoom: number) {
  const [west, south, east, north] = bbox
  const span = Math.min(360, east - west)
  const s = Math.max(-85, south)
  const n = Math.min(85, north)
  if (![west, south, east, north, zoom].every(Number.isFinite) || span <= 0 || n <= s) return []
  const cols = zoom < 4 ? 4 : 6
  const rows = 4
  return Array.from({ length: cols * rows }, (_, i) => ({
    lon: +(((((west + (i % cols + 0.5) * span / cols) + 180) % 360 + 360) % 360) - 180).toFixed(2),
    lat: +(s + (Math.floor(i / cols) + 0.5) * (n - s) / rows).toFixed(2),
  }))
}
