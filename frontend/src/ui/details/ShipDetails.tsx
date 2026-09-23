import { useEffect, useState, useSyncExternalStore } from 'react'
import { Route, Ship as ShipIcon } from 'lucide-react'
import { DrawerShell, ExternalLink, KeyValues, Section, Stat } from '../common'
import { WatchButton, ZoomButton } from './shared'
import { api, isAbort } from '../../lib/api'
import { getShip, shipsSignal } from '../../data/ships'
import { getShipTrack, setShipTrack } from '../../map/tracks'
import { useSelection } from '../../store/selection'
import { CATEGORY_COLOR, NAV_STATUS, shipCategory, shipTypeLabel } from '../../lib/ais'
import { fmtCoord, fmtHeading, fmtKnots, fmtNumber, timeAgo } from '../../lib/format'
import { notify } from '../../store/notice'

interface ShipRecord {
  mmsi: number
  lat: number
  lon: number
  course: number | null
  speed: number | null
  heading: number | null
  nav_status: number | null
  name: string
  ship_type: number | null
  imo: number | null
  callsign: string | null
  destination: string | null
  eta: string | null
  draught: number | null
  length: number | null
  beam: number | null
  kind: 'vessel' | 'sar_aircraft'
  altitude: number | null
  timestamp: number
}

export default function ShipDetails({ mmsi }: { mmsi: number }) {
  useSyncExternalStore(shipsSignal.subscribe, shipsSignal.version)
  const live = getShip(mmsi)
  const clear = useSelection(s => s.clear)
  const [record, setRecord] = useState<ShipRecord | null>(null)
  const [missing, setMissing] = useState(false)
  const [trackOn, setTrackOn] = useState(() => getShipTrack()?.mmsi === mmsi)

  useEffect(() => {
    const ac = new AbortController()
    const load = () =>
      api<ShipRecord>(`/api/ships/${mmsi}`, { signal: ac.signal })
        .then(r => {
          setRecord(r)
          setMissing(false)
        })
        .catch(e => !isAbort(e) && setMissing(true))
    void load()
    const id = setInterval(load, 30_000)
    return () => {
      ac.abort()
      clearInterval(id)
    }
  }, [mmsi])

  useEffect(() => () => setShipTrack(null), [mmsi])

  const showTrack = async () => {
    if (trackOn) {
      setShipTrack(null)
      setTrackOn(false)
      return
    }
    try {
      const data = await api<{ points: [number, number, number, number | null][] }>(`/api/history/track?mmsi=${mmsi}&hours=24`)
      if (data.points.length < 2) {
        notify('No recorded positions for this vessel in the last 24 hours yet.', 'info')
        return
      }
      setShipTrack({ mmsi, points: data.points.map(p => [p[0], p[1], p[2]]) })
      setTrackOn(true)
    } catch (e) {
      notify(e instanceof Error ? e.message : 'Track unavailable')
    }
  }

  const s = record
  const lat = live?.lat ?? s?.lat
  const lon = live?.lon ?? s?.lon
  const name = (live?.name || s?.name || '').trim() || `MMSI ${mmsi}`
  const type = s?.ship_type ?? live?.ship_type ?? null
  const sar = live?.sar || s?.kind === 'sar_aircraft'
  const category = shipCategory(type, sar)
  const speed = live?.speed ?? s?.speed ?? null
  const course = live?.course ?? s?.course ?? null
  const heading = live?.heading ?? s?.heading ?? null
  const nav = s?.nav_status ?? live?.nav_status ?? null
  const reported = live?.timestamp ?? s?.timestamp

  return (
    <DrawerShell
      icon={<ShipIcon size={20} />}
      iconColor={CATEGORY_COLOR[category]}
      title={name}
      subtitle={`${sar ? 'Search and rescue aircraft' : shipTypeLabel(type)} · MMSI ${mmsi}`}
      onClose={clear}
      footer={
        <>
          {lat != null && lon != null && <ZoomButton lon={lon} lat={lat} zoom={12} />}
          <button className="btn" aria-pressed={trackOn} onClick={showTrack}>
            <Route size={15} /> {trackOn ? 'Hide track' : '24 h track'}
          </button>
          <WatchButton wtype="vessel" name={name} params={{ mmsi }} />
        </>
      }
    >
      {missing && !live && <p className="notice-inline">This vessel has not reported recently and left the live picture.</p>}
      <div className="stats">
        <Stat label="Speed" value={fmtKnots(speed)} />
        <Stat label="Course" value={course == null ? '—' : `${Math.round(course)}°`} />
        <Stat label="Heading" value={heading == null ? '—' : `${Math.round(heading)}°`} />
      </div>
      <Section title="Voyage">
        <KeyValues
          rows={[
            ['Status', nav != null ? NAV_STATUS[nav] : null],
            ['Destination', s?.destination],
            ['ETA (UTC)', s?.eta],
            ['Draught', s?.draught != null ? fmtNumber(s.draught, 1, 'm') : null],
            ['Heading', heading != null ? fmtHeading(heading) : null],
            ['Position', lat != null && lon != null ? fmtCoord(lat, lon) : null],
            ['Last report', reported ? timeAgo(reported) : null],
            ['Altitude', sar && s?.altitude != null ? fmtNumber(s.altitude, 0, 'm') : null],
          ]}
        />
      </Section>
      <Section title="Vessel">
        <KeyValues
          rows={[
            ['MMSI', <span className="num">{mmsi}</span>],
            ['IMO', s?.imo],
            ['Call sign', s?.callsign],
            ['Type', type != null ? `${shipTypeLabel(type)} (${type})` : 'Not reported yet'],
            ['Size', s?.length ? `${s.length} m${s.beam ? ` × ${s.beam} m` : ''}` : null],
          ]}
        />
        {!s?.imo && !s?.callsign && !s?.length && <p className="muted" style={{ marginTop: 8 }}>Identity details arrive with the vessel's next static-data broadcast (every few minutes).</p>}
      </Section>
      <div className="link-row">
        <ExternalLink href={`https://www.marinetraffic.com/en/ais/details/ships/mmsi:${mmsi}`}>MarineTraffic</ExternalLink>
        <ExternalLink href={`https://www.myshiptracking.com/vessels/mmsi-${mmsi}`}>MyShipTracking</ExternalLink>
        {s?.imo && <ExternalLink href={`https://www.vesselfinder.com/vessels/details/${s.imo}`}>VesselFinder</ExternalLink>}
      </div>
    </DrawerShell>
  )
}
