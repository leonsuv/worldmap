import { useEffect, useState, useSyncExternalStore } from 'react'
import { Plane } from 'lucide-react'
import { DrawerShell, ExternalLink, KeyValues, Loading, Section, Stat } from '../common'
import { ZoomButton } from './shared'
import { api, isAbort } from '../../lib/api'
import { flightsSignal, followFlight, getFlightState, extrapolate, flightsDataTime } from '../../data/flights'
import { findFeature } from '../../data/staticData'
import { setFlightTrack } from '../../map/tracks'
import { useSelection } from '../../store/selection'
import { useStatus } from '../../store/status'
import { useNow } from '../../lib/useNow'
import { AIRCRAFT_CATEGORY, EMERGENCY_SQUAWKS } from '../../lib/aircraft'
import { fmtAltitude, fmtCoord, fmtDateTime, fmtHeading, fmtNumber, fmtSpeedMs, timeAgo } from '../../lib/format'

interface FlightRecord {
  firstSeen: number
  lastSeen: number
  estDepartureAirport: string | null
  estArrivalAirport: string | null
  callsign: string | null
}

function airportLabel(icao: string | null): string {
  if (!icao) return 'Unknown'
  const name = findFeature('airports', 'ident', icao)?.properties?.name as string | undefined
  return name ? `${icao} · ${name}` : icao
}

export default function FlightDetails({ id }: { id: string }) {
  useSyncExternalStore(flightsSignal.subscribe, flightsSignal.version)
  useEffect(() => followFlight(id), [id])
  const { flight: f, live } = getFlightState(id)
  const clear = useSelection(s => s.clear)
  const now = useNow()
  const [track, setTrack] = useState<'loading' | 'none' | number>('loading')
  const authenticated = useStatus(s => s.caps?.flights_authenticated ?? false)
  const [flights, setFlights] = useState<FlightRecord[] | null | string>(null)

  useEffect(() => {
    const ac = new AbortController()
    api<{ path: [number, number, number][] }>(`/api/flights/track?icao24=${id}`, { signal: ac.signal })
      .then(data => {
        setFlightTrack(data.path.length > 1 ? { icao24: id, path: data.path } : null)
        setTrack(data.path.length > 1 ? data.path.length : 'none')
      })
      .catch(e => !isAbort(e) && setTrack('none'))
    if (authenticated) {
      api<FlightRecord[]>(`/api/flights/aircraft?icao24=${id}`, { signal: ac.signal })
        .then(data => setFlights([...data].sort((a, b) => b.firstSeen - a.firstSeen)))
        .catch(e => !isAbort(e) && setFlights(e instanceof Error ? e.message : 'Flight history is unavailable right now.'))
    }
    return () => {
      ac.abort()
      setFlightTrack(null)
    }
  }, [id, authenticated])

  if (!f) {
    return (
      <DrawerShell icon={<Plane size={20} />} title={id.toUpperCase()} subtitle="Aircraft" onClose={clear}>
        <p className="muted">This aircraft is not in the current OpenSky picture.</p>
      </DrawerShell>
    )
  }

  const [lon, lat] = extrapolate(f, now / 1000, flightsDataTime())
  const emergency = f.squawk ? EMERGENCY_SQUAWKS[f.squawk] : undefined
  const climbing = (f.vertical_rate ?? 0) > 0.5 ? '↑' : (f.vertical_rate ?? 0) < -0.5 ? '↓' : ''
  return (
    <DrawerShell
      icon={<Plane size={20} />}
      title={f.callsign || f.icao24.toUpperCase()}
      subtitle={[f.country, AIRCRAFT_CATEGORY[f.category]].filter(Boolean).join(' · ')}
      onClose={clear}
      footer={
        <>
          <ZoomButton lon={lon} lat={lat} zoom={9} />
          <div className="link-row" style={{ marginLeft: 'auto', alignSelf: 'center' }}>
            {f.callsign && <ExternalLink href={`https://www.flightradar24.com/${encodeURIComponent(f.callsign)}`}>Flightradar24</ExternalLink>}
            <ExternalLink href={`https://globe.adsbexchange.com/?icao=${f.icao24}`}>ADS-B Exchange</ExternalLink>
          </div>
        </>
      }
    >
      {!live && <p className="notice-inline">Signal lost — showing the last known state.</p>}
      {emergency && <p className="notice-inline" style={{ background: 'var(--danger-soft)', color: 'var(--danger)' }}>Squawk {f.squawk}: {emergency}</p>}
      <div className="stats">
        <Stat label="Altitude" value={f.on_ground ? 'Ground' : fmtNumber(f.altitude, 0, 'm')} />
        <Stat label="Ground speed" value={fmtNumber((f.velocity ?? NaN) * 3.6, 0, 'km/h')} />
        <Stat label="Vertical rate" value={`${climbing} ${fmtNumber(Math.abs(f.vertical_rate ?? NaN), 1, 'm/s')}`} />
      </div>
      <Section title="Flight">
        <KeyValues
          rows={[
            ['Transponder', <span className="num">{f.icao24.toUpperCase()}</span>],
            ['Callsign', f.callsign],
            ['Altitude', f.on_ground ? 'On the ground' : fmtAltitude(f.altitude)],
            ['Speed', fmtSpeedMs(f.velocity)],
            ['Heading', fmtHeading(f.track)],
            ['Squawk', f.squawk],
            ['Position', fmtCoord(lat, lon)],
            ['Last report', timeAgo(f.time_position, now)],
          ]}
        />
      </Section>
      <Section title="Current track">
        {track === 'loading' ? <Loading label="Loading track…" /> : track === 'none' ? <p className="muted">No track available for this flight.</p> : <p className="muted">{track} positions shown on the map.</p>}
      </Section>
      <Section title="Recent flights (last 2 days)">
        {!authenticated ? (
          <p className="muted">Flight history needs a free OpenSky account. Add OPENSKY_CLIENT_ID and OPENSKY_CLIENT_SECRET to backend/.env.</p>
        ) : flights === null ? (
          <Loading />
        ) : typeof flights === 'string' ? (
          <p className="muted">{flights}</p>
        ) : flights.length === 0 ? (
          <p className="muted">OpenSky has no completed flights for this aircraft yet (history is processed overnight).</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Route</th>
                <th>Departed</th>
                <th>Arrived</th>
              </tr>
            </thead>
            <tbody>
              {flights.slice(0, 12).map(r => (
                <tr key={`${r.firstSeen}-${r.lastSeen}`}>
                  <td>
                    <div title={airportLabel(r.estDepartureAirport)}>{r.estDepartureAirport ?? '?'} → {r.estArrivalAirport ?? '?'}</div>
                    {r.callsign && <small className="muted">{r.callsign.trim()}</small>}
                  </td>
                  <td>{fmtDateTime(r.firstSeen)}</td>
                  <td>{fmtDateTime(r.lastSeen)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
    </DrawerShell>
  )
}
