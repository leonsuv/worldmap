const cache = new Map<number, Intl.NumberFormat>()
function nf(digits: number): Intl.NumberFormat {
  let f = cache.get(digits)
  if (!f) {
    f = new Intl.NumberFormat('en-US', { maximumFractionDigits: digits, minimumFractionDigits: 0 })
    cache.set(digits, f)
  }
  return f
}

export const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)

export function fmtNumber(value: number | null | undefined, digits = 0, unit = ''): string {
  if (!isNum(value)) return '—'
  return `${nf(digits).format(value)}${unit ? ` ${unit}` : ''}`
}

export function fmtCount(n: number): string {
  if (n >= 1_000_000) return `${nf(1).format(n / 1_000_000)}M`
  if (n >= 10_000) return `${nf(0).format(n / 1000)}k`
  return nf(0).format(n)
}

/** "just now", "4 min ago", "3 h ago", "2 d ago". */
export function timeAgo(unixSeconds: number | null | undefined, now = Date.now()): string {
  if (!isNum(unixSeconds)) return '—'
  const s = Math.max(0, Math.round(now / 1000 - unixSeconds))
  if (s < 45) return 'just now'
  if (s < 3600) return `${Math.round(s / 60)} min ago`
  if (s < 86400) return `${Math.round(s / 3600)} h ago`
  return `${Math.round(s / 86400)} d ago`
}

export function fmtDateTime(unixSeconds: number | null | undefined, opts: Intl.DateTimeFormatOptions = {}): string {
  if (!isNum(unixSeconds)) return '—'
  return new Date(unixSeconds * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', ...opts })
}

export function fmtTime(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

export function fmtLat(lat: number): string {
  return `${Math.abs(lat).toFixed(2)}° ${lat < 0 ? 'S' : 'N'}`
}

export function fmtLon(lon: number): string {
  const wrapped = ((((lon + 180) % 360) + 360) % 360) - 180
  return `${Math.abs(wrapped).toFixed(2)}° ${wrapped < 0 ? 'W' : 'E'}`
}

export function fmtCoord(lat: number, lon: number): string {
  return `${fmtLat(lat)}, ${fmtLon(lon)}`
}

const COMPASS = ['N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE', 'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW']
export function compass(deg: number): string {
  return COMPASS[Math.round((((deg % 360) + 360) % 360) / 22.5) % 16]
}

export function fmtHeading(deg: number | null | undefined): string {
  return isNum(deg) ? `${Math.round(deg)}° ${compass(deg)}` : '—'
}

/** Metres → "10,973 m · FL360". */
export function fmtAltitude(m: number | null | undefined): string {
  if (!isNum(m)) return '—'
  const fl = Math.round((m * 3.28084) / 100)
  return fl >= 10 ? `${fmtNumber(m, 0, 'm')} · FL${String(fl).padStart(3, '0')}` : fmtNumber(m, 0, 'm')
}

/** Metres per second → "850 km/h · 459 kn". */
export function fmtSpeedMs(ms: number | null | undefined): string {
  if (!isNum(ms)) return '—'
  return `${fmtNumber(ms * 3.6, 0, 'km/h')} · ${fmtNumber(ms * 1.943844, 0, 'kn')}`
}

export function fmtKnots(kn: number | null | undefined): string {
  return isNum(kn) ? fmtNumber(kn, 1, 'kn') : '—'
}

export function fmtDistanceKm(km: number | null | undefined): string {
  if (!isNum(km)) return '—'
  return km < 10 ? fmtNumber(km, 1, 'km') : fmtNumber(km, 0, 'km')
}

export function titleCase(s: string): string {
  return s.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}
