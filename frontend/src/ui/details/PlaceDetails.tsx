import { useEffect, useMemo, useState } from 'react'
import { Anchor, Atom, Cable, CloudSun, Factory, Lightbulb, MapPin, Plane, TriangleAlert, UtilityPole } from 'lucide-react'
import { DrawerShell, ExternalLink, KeyValues, Loading, Section, Stat } from '../common'
import { WatchButton, ZoomButton } from './shared'
import { api, isAbort } from '../../lib/api'
import { useSelection, type Selection } from '../../store/selection'
import { useUi } from '../../store/ui'
import { useEvents } from '../../store/events'
import { useStatus } from '../../store/status'
import { getShips } from '../../data/ships'
import { findFeature } from '../../data/staticData'
import { ATON_TYPES } from '../../lib/ais'
import { compass, fmtCoord, fmtNumber, fmtTime, isNum, titleCase } from '../../lib/format'
import { weatherLabel } from '../../lib/weather'

type Place = Extract<Selection, { props: Record<string, unknown> }>

const str = (v: unknown) => (v == null || v === '' ? null : String(v))
const num = (v: unknown) => (typeof v === 'number' ? v : v == null || v === '' ? null : Number(v))

function distanceKm(a: [number, number], b: [number, number]) {
  const r = Math.PI / 180
  const dLat = (b[1] - a[1]) * r
  const dLon = (b[0] - a[0]) * r
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(a[1] * r) * Math.cos(b[1] * r) * Math.sin(dLon / 2) ** 2
  return 12742 * Math.asin(Math.sqrt(h))
}

interface AirportFlight {
  callsign: string | null
  estDepartureAirport: string | null
  estArrivalAirport: string | null
  firstSeen: number
  lastSeen: number
}

function AirportFlights({ icao }: { icao: string }) {
  const authenticated = useStatus(s => s.caps?.flights_authenticated ?? false)
  const [kind, setKind] = useState<'arrivals' | 'departures' | null>(null)
  const [data, setData] = useState<AirportFlight[] | 'loading' | { error: string }>('loading')
  useEffect(() => {
    if (!kind) return
    const ac = new AbortController()
    api<{ flights: AirportFlight[] }>(`/api/flights/airport?airport=${icao}&kind=${kind}`, { signal: ac.signal })
      .then(r => setData(r.flights.sort((a, b) => (kind === 'arrivals' ? b.lastSeen - a.lastSeen : b.firstSeen - a.firstSeen))))
      .catch(e => !isAbort(e) && setData({ error: e instanceof Error ? e.message : 'OpenSky history is unavailable right now.' }))
    return () => ac.abort()
  }, [icao, kind])
  if (!authenticated) {
    return (
      <Section title="Yesterday's traffic">
        <p className="muted">Arrivals and departures need a free OpenSky account (OPENSKY_CLIENT_ID and OPENSKY_CLIENT_SECRET in backend/.env).</p>
      </Section>
    )
  }
  const choose = (next: 'arrivals' | 'departures') => {
    if (next === kind) return
    setData('loading')
    setKind(next)
  }
  return (
    <Section
      title="Yesterday's traffic"
      action={
        <div className="segmented" role="group" aria-label="Traffic direction">
          <button aria-pressed={kind === 'arrivals'} onClick={() => choose('arrivals')}>Arrivals</button>
          <button aria-pressed={kind === 'departures'} onClick={() => choose('departures')}>Departures</button>
        </div>
      }
    >
      {!kind ? (
        <p className="muted">Load arrivals or departures recorded by OpenSky for the previous day.</p>
      ) : data === 'loading' ? (
        <Loading />
      ) : 'error' in data ? (
        <p className="muted">{data.error}</p>
      ) : data.length === 0 ? (
        <p className="muted">No flights recorded for yesterday.</p>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>Flight</th>
              <th>{kind === 'arrivals' ? 'From' : 'To'}</th>
              <th>{kind === 'arrivals' ? 'Landed' : 'Departed'}</th>
            </tr>
          </thead>
          <tbody>
            {data.slice(0, 40).map(f => {
              const other = kind === 'arrivals' ? f.estDepartureAirport : f.estArrivalAirport
              const otherName = other ? (findFeature('airports', 'ident', other)?.properties?.name as string | undefined) : undefined
              return (
                <tr key={`${f.callsign}-${f.firstSeen}`}>
                  <td>{f.callsign?.trim() || '—'}</td>
                  <td title={otherName}>{other ?? '?'}</td>
                  <td>{fmtTime(kind === 'arrivals' ? f.lastSeen : f.firstSeen)}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </Section>
  )
}

function AirportView({ sel }: { sel: Place }) {
  const p = sel.props
  const ident = str(p.ident)
  const elevation = num(p.elevation_ft)
  const icao = ident && /^[A-Z0-9]{4}$/.test(ident) ? ident : null
  return (
    <>
      <Section title="Airport">
        <KeyValues
          rows={[
            ['IATA', str(p.iata)],
            ['ICAO', ident],
            ['City', str(p.city)],
            ['Country', str(p.country)],
            ['Size', p.kind === 'large' ? 'Major airport' : 'Regional airport'],
            ['Elevation', isNum(elevation) ? `${fmtNumber(elevation, 0, 'ft')} (${fmtNumber(elevation * 0.3048, 0, 'm')})` : null],
            ['Position', fmtCoord(sel.lngLat[1], sel.lngLat[0])],
          ]}
        />
      </Section>
      {icao && <AirportFlights icao={icao} />}
    </>
  )
}

function SeaportView({ sel }: { sel: Place }) {
  const p = sel.props
  const nearby = useMemo(() => getShips().filter(s => distanceKm([s.lon, s.lat], sel.lngLat) <= 15), [sel.lngLat])
  const moving = nearby.filter(s => (s.speed ?? 0) >= 0.5).length
  return (
    <>
      {getShips().length > 0 && (
        <div className="stats">
          <Stat label="Vessels within 15 km" value={nearby.length} />
          <Stat label="Under way" value={moving} />
          <Stat label="In port" value={nearby.length - moving} />
        </div>
      )}
      <Section title="Seaport">
        <KeyValues
          rows={[
            ['UN/LOCODE', str(p.locode)],
            ['Country', str(p.country)],
            ['Harbour size', str(p.size) ? titleCase(String(p.size)) : null],
            ['Harbour type', str(p.harbor_type)],
            ['Position', fmtCoord(sel.lngLat[1], sel.lngLat[0])],
          ]}
        />
      </Section>
    </>
  )
}

interface Unit {
  name: string
  status: string
  capacity_mw: number | null
  reactor_type: string | null
  model: string | null
}

function ReactorView({ sel }: { sel: Place }) {
  const p = sel.props
  const units = (Array.isArray(p.units) ? p.units : []) as Unit[]
  return (
    <>
      <div className="stats">
        <Stat label="Operating capacity" value={fmtNumber(num(p.capacity_mw), 0, 'MW')} />
        <Stat label="Operating units" value={num(p.units_operational) ?? '—'} />
        <Stat label="Under construction" value={num(p.units_construction) ?? 0} />
      </div>
      <Section title="Reactor units">
        {units.length ? (
          <table className="table">
            <thead>
              <tr>
                <th>Unit</th>
                <th>Type</th>
                <th>MW</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {units.map(u => (
                <tr key={u.name}>
                  <td>{u.name}</td>
                  <td title={u.model ?? undefined}>{u.reactor_type ?? '—'}</td>
                  <td>{fmtNumber(u.capacity_mw)}</td>
                  <td>{u.status.replace(' Operation', '')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="muted">No unit details.</p>
        )}
      </Section>
      <KeyValues rows={[['Country', str(p.country)], ['Position', fmtCoord(sel.lngLat[1], sel.lngLat[0])]]} />
    </>
  )
}

function AtonView({ sel }: { sel: Place }) {
  const p = sel.props
  const off = p.off_position === true || p.off_position === 'true'
  const virtual = p.virtual === true || p.virtual === 'true'
  return (
    <>
      {off && <p className="notice-inline" style={{ background: 'var(--danger-soft)', color: 'var(--danger)' }}>Reported off its charted position.</p>}
      <KeyValues
        rows={[
          ['Type', ATON_TYPES[num(p.aton_type) ?? 0] ?? 'Aid to navigation'],
          ['Kind', virtual ? 'Virtual (broadcast only)' : 'Physical aid'],
          ['MMSI', str(p.mmsi)],
          ['Position', fmtCoord(sel.lngLat[1], sel.lngLat[0])],
        ]}
      />
    </>
  )
}

interface PointForecast {
  temperature: number | null
  apparent_temperature: number | null
  humidity: number | null
  precipitation: number | null
  weather_code: number | null
  cloud_cover: number | null
  pressure: number | null
  wind_speed: number | null
  wind_direction: number | null
  wind_gusts: number | null
  hourly: { time: string[]; temperature_2m: number[]; precipitation_probability: number[]; wind_speed_10m: number[] } | null
  timezone: string | null
}

function WeatherView({ sel }: { sel: Place }) {
  const [lon, lat] = sel.lngLat
  const [data, setData] = useState<PointForecast | 'error' | null>(null)
  useEffect(() => {
    const ac = new AbortController()
    api<PointForecast>(`/api/weather?lat=${lat.toFixed(2)}&lon=${lon.toFixed(2)}`, { signal: ac.signal })
      .then(setData)
      .catch(e => !isAbort(e) && setData('error'))
    return () => ac.abort()
  }, [lat, lon])
  if (data === null) return <Loading label="Loading forecast…" />
  if (data === 'error') return <p className="muted">The forecast is unavailable right now.</p>
  const hours = data.hourly?.time.map((t, i) => ({ t, temp: data.hourly!.temperature_2m[i], rain: data.hourly!.precipitation_probability[i], wind: data.hourly!.wind_speed_10m[i] })) ?? []
  return (
    <>
      <div className="stats">
        <Stat label={weatherLabel(data.weather_code)} value={fmtNumber(data.temperature, 0, '°C')} />
        <Stat label="Wind" value={fmtNumber(data.wind_speed, 1, 'm/s')} />
        <Stat label="Gusts" value={fmtNumber(data.wind_gusts, 1, 'm/s')} />
      </div>
      <Section title="Now">
        <KeyValues
          rows={[
            ['Feels like', fmtNumber(data.apparent_temperature, 0, '°C')],
            ['Wind from', isNum(data.wind_direction) ? `${compass(data.wind_direction)} (${Math.round(data.wind_direction)}°)` : null],
            ['Humidity', fmtNumber(data.humidity, 0, '%')],
            ['Precipitation', fmtNumber(data.precipitation, 1, 'mm')],
            ['Cloud cover', fmtNumber(data.cloud_cover, 0, '%')],
            ['Pressure', fmtNumber(data.pressure, 0, 'hPa')],
          ]}
        />
      </Section>
      {hours.length > 0 && (
        <Section title={`Next 24 hours${data.timezone ? ` (${data.timezone})` : ''}`}>
          <div className="forecast">
            {hours.map(h => (
              <div key={h.t}>
                <small>{h.t.slice(11, 16)}</small>
                <b>{Math.round(h.temp)}°</b>
                <small>{h.rain ?? 0}%</small>
                <small>{Math.round(h.wind)} m/s</small>
              </div>
            ))}
          </div>
        </Section>
      )}
    </>
  )
}

function NetworkView({ sel }: { sel: Place }) {
  const p = sel.props
  if (sel.kind === 'grid') {
    return <p className="muted">Gridfinder estimates medium- and high-voltage lines from night-time lights and road networks. Lines are modelled, not surveyed.</p>
  }
  if (sel.kind === 'hvline') {
    const voltages = str(p.voltage)
      ?.split(';')
      .map(v => (Number(v) >= 1000 ? `${Math.round(Number(v) / 1000)} kV` : v))
      .join(', ')
    return (
      <KeyValues
        rows={[
          ['Voltage', voltages ?? (str(p.voltage_kv) ? `${p.voltage_kv} kV` : null)],
          ['Current', p.hvdc === true || p.hvdc === 'true' ? 'Direct current (HVDC)' : 'Alternating current'],
          ['Type', p.kind === 'cable' ? `Cable${str(p.location) ? ` (${p.location})` : ''}` : 'Overhead line'],
          ['Circuits', str(p.circuits)],
          ['Operator', str(p.operator)],
          ['Reference', str(p.ref)],
        ]}
      />
    )
  }
  return (
    <KeyValues
      rows={[
        ['Commodity', titleCase(String(str(p.commodity) ?? str(p.COMMODITY) ?? 'Unknown').toLowerCase())],
        ['Substance', str(p.substance)],
        ['Operator', str(p.operator) ?? str(p.OPERATOR)],
        ['Diameter', num(p.diameter_mm) ? fmtNumber(num(p.diameter_mm), 0, 'mm') : num(p.PIPE_DIAMETER_MM) ? fmtNumber(num(p.PIPE_DIAMETER_MM), 0, 'mm') : null],
        ['Status', str(p.FAC_STATUS) ? titleCase(String(p.FAC_STATUS).toLowerCase()) : null],
        ['Usage', str(p.usage) ? titleCase(String(p.usage)) : null],
        ['Location', str(p.location) ? titleCase(String(p.location)) : null],
      ]}
    />
  )
}

function PlaceView({ sel }: { sel: Place }) {
  const startEvent = () => {
    useEvents.getState().select(null)
    useEvents.getState().setDraft({ lat: sel.lngLat[1], lon: sel.lngLat[0], name: str(sel.props.name) ?? undefined })
    useUi.getState().openPanel('events')
  }
  return (
    <>
      <KeyValues rows={[['Area', str(sel.props.detail)], ['Position', fmtCoord(sel.lngLat[1], sel.lngLat[0])]]} />
      <button className="btn" onClick={startEvent}>
        <TriangleAlert size={15} /> Create an event here
      </button>
    </>
  )
}

const META: Record<Place['kind'], { icon: React.ReactNode; subtitle: (p: Record<string, unknown>) => string; watch?: 'airport' | 'port' | 'reactor' | 'area' }> = {
  airport: { icon: <Plane size={20} />, subtitle: p => [str(p.iata), str(p.city), str(p.country)].filter(Boolean).join(' · '), watch: 'airport' },
  seaport: { icon: <Anchor size={20} />, subtitle: p => [str(p.locode), str(p.country)].filter(Boolean).join(' · '), watch: 'port' },
  reactor: { icon: <Atom size={20} />, subtitle: p => `Nuclear power plant · ${titleCase(String(p.status ?? 'operational'))}`, watch: 'reactor' },
  aton: { icon: <Lightbulb size={20} />, subtitle: () => 'Aid to navigation' },
  weather: { icon: <CloudSun size={20} />, subtitle: () => 'Weather · Open-Meteo' },
  hvline: { icon: <UtilityPole size={20} />, subtitle: p => [str(p.voltage_kv) ? `${p.voltage_kv} kV` : null, 'OpenStreetMap'].filter(Boolean).join(' · ') },
  pipeline: { icon: <Factory size={20} />, subtitle: () => 'Pipeline' },
  grid: { icon: <Cable size={20} />, subtitle: () => 'Estimated power line · Gridfinder' },
  place: { icon: <MapPin size={20} />, subtitle: () => 'Place · OpenStreetMap Nominatim', watch: 'area' },
}

export default function PlaceDetails({ sel }: { sel: Place }) {
  const clear = useSelection(s => s.clear)
  const meta = META[sel.kind]
  const p = sel.props
  const title =
    sel.kind === 'reactor' ? `${p.name} nuclear plant`
      : sel.kind === 'weather' ? `Weather at ${fmtCoord(sel.lngLat[1], sel.lngLat[0])}`
        : sel.kind === 'grid' ? 'Estimated power line'
          : str(p.name) ?? str(p.FAC_NAME) ?? (sel.kind === 'hvline' ? 'Power line' : sel.kind === 'pipeline' ? 'Pipeline' : 'Selected place')
  const [lon, lat] = sel.lngLat
  return (
    <DrawerShell
      icon={meta.icon}
      title={title}
      subtitle={meta.subtitle(p)}
      onClose={clear}
      footer={
        <>
          <ZoomButton lon={lon} lat={lat} zoom={sel.kind === 'airport' || sel.kind === 'seaport' ? 12 : 10} />
          {meta.watch && <WatchButton wtype={meta.watch} name={title} params={{ lat, lon, ...(meta.watch === 'area' ? { radius_km: 25 } : {}) }} />}
          {sel.kind === 'reactor' && <ExternalLink href={`https://pris.iaea.org/PRIS/CountryStatistics/CountryDetails.aspx?current=${encodeURIComponent(String(p.country ?? ''))}`}>IAEA PRIS</ExternalLink>}
        </>
      }
    >
      {sel.kind === 'airport' && <AirportView sel={sel} />}
      {sel.kind === 'seaport' && <SeaportView sel={sel} />}
      {sel.kind === 'reactor' && <ReactorView sel={sel} />}
      {sel.kind === 'aton' && <AtonView sel={sel} />}
      {sel.kind === 'weather' && <WeatherView sel={sel} />}
      {(sel.kind === 'hvline' || sel.kind === 'pipeline' || sel.kind === 'grid') && <NetworkView sel={sel} />}
      {sel.kind === 'place' && <PlaceView sel={sel} />}
      {sel.kind === 'weather' && <p className="muted">Open-Meteo forecast · values refresh every 15 minutes.</p>}
    </DrawerShell>
  )
}
