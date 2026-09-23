import { fmtDateTime } from './format'

export interface Report {
  generated_at: number
  vessels_tracked: number
  aircraft_tracked: number | null
  airports: number
  seaports: number
  nuclear_plants: number
  unacknowledged_alerts: number
  watchlist_items: number
  active_events: { name: string; event_type: string; lat: number; lon: number; radius_km: number; description: string; started_at: number; vessels: number; aircraft: number; airports: number; seaports: number; nuclear_plants: number }[]
}

export function reportMarkdown(r: Report): string {
  const lines = [
    `# WorldMap situation report`,
    ``,
    `Generated ${new Date(r.generated_at * 1000).toUTCString()}`,
    ``,
    `| Overview | |`,
    `|---|---:|`,
    `| Vessels tracked | ${r.vessels_tracked.toLocaleString('en-US')} |`,
    `| Aircraft tracked | ${r.aircraft_tracked == null ? '—' : r.aircraft_tracked.toLocaleString('en-US')} |`,
    `| Active events | ${r.active_events.length} |`,
    `| Unacknowledged alerts | ${r.unacknowledged_alerts} |`,
    `| Watchlist items | ${r.watchlist_items} |`,
    ``,
    `## Active events`,
    ``,
  ]
  if (!r.active_events.length) lines.push('No active events.')
  for (const e of r.active_events) {
    lines.push(`### ${e.name} (${e.event_type})`, '')
    lines.push(`- Centre ${e.lat.toFixed(4)}, ${e.lon.toFixed(4)} · radius ${e.radius_km} km · since ${fmtDateTime(e.started_at)}`)
    lines.push(`- Inside the area: ${e.vessels} vessels, ${e.aircraft} aircraft, ${e.airports} airports, ${e.seaports} seaports, ${e.nuclear_plants} nuclear plants`)
    if (e.description) lines.push(`- ${e.description}`)
    lines.push('')
  }
  return lines.join('\n')
}
