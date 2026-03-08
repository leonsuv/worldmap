import { memo, useCallback, useEffect } from 'react'
import { useFlightDetailStore } from '../store/flightDetail'
import type { FlightDetail, FlightRecord } from '../store/flightDetail'
import { Plane, X, ExternalLink, Loader2 } from 'lucide-react'

function formatTime(unix: number): string {
  return new Date(unix * 1000).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

function formatAlt(meters: number | null | undefined): string {
  if (meters == null) return '—'
  return `${Math.round(meters)}m (FL${Math.round(meters * 3.28084 / 100)})`
}

function FlightRow({ f }: { f: FlightRecord }) {
  return (
    <tr>
      <td className="fpd-cell">{f.estDepartureAirport ?? '?'}</td>
      <td className="fpd-cell">{f.estArrivalAirport ?? '?'}</td>
      <td className="fpd-cell">{formatTime(f.firstSeen)}</td>
      <td className="fpd-cell">{formatTime(f.lastSeen)}</td>
    </tr>
  )
}

function TrackInfo({ detail }: { detail: FlightDetail }) {
  if (!detail.track) return null
  const pts = detail.track
  if (pts.length === 0) return <p className="fpd-empty">No live track available</p>
  const minAlt = Math.min(...pts.map(p => p[2]).filter(a => a != null))
  const maxAlt = Math.max(...pts.map(p => p[2]).filter(a => a != null))
  return (
    <div className="fpd-track-info">
      <span>{pts.length} waypoints</span>
      <span>Alt: {formatAlt(minAlt)} → {formatAlt(maxAlt)}</span>
    </div>
  )
}

function FlightDetailPanel() {
  const detail = useFlightDetailStore((s) => s.detail)
  const close = useFlightDetailStore((s) => s.close)

  const onKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') close()
  }, [close])

  useEffect(() => {
    if (!detail) return
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [detail, onKeyDown])

  if (!detail) return null

  const callsign = detail.callsign.trim()

  return (
    <div className="fpd-panel">
      <button className="fpd-close" onClick={close}><X size={16} /></button>

      <div className="fpd-header">
        <Plane size={18} />
        <div>
          <h3 className="fpd-title">{callsign || detail.icao24}</h3>
          <span className="fpd-subtitle">
            {detail.icao24} · {detail.origin_country}
          </span>
        </div>
      </div>

      {detail.loading && (
        <div className="fpd-loading">
          <Loader2 size={16} className="fpd-spin" />
          Loading flight data…
        </div>
      )}

      <TrackInfo detail={detail} />

      {detail.flights && detail.flights.length > 0 && (
        <div className="fpd-section">
          <h4 className="fpd-section-title">Recent Flights (last 48h)</h4>
          <div className="fpd-table-wrap">
            <table className="fpd-table">
              <thead>
                <tr>
                  <th>From</th><th>To</th><th>Departed</th><th>Arrived</th>
                </tr>
              </thead>
              <tbody>
                {detail.flights.map((f, i) => <FlightRow key={i} f={f} />)}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {detail.flights && detail.flights.length === 0 && !detail.loading && (
        <p className="fpd-empty">No recent flights found</p>
      )}

      <div className="fpd-links">
        {callsign && (
          <a
            href={`https://www.flightradar24.com/${callsign}`}
            target="_blank"
            rel="noopener noreferrer"
            className="fpd-link"
          >
            Flightradar24 <ExternalLink size={11} />
          </a>
        )}
        <a
          href={`https://opensky-network.org/aircraft-profile?icao24=${detail.icao24}`}
          target="_blank"
          rel="noopener noreferrer"
          className="fpd-link"
        >
          OpenSky <ExternalLink size={11} />
        </a>
      </div>
    </div>
  )
}

export default memo(FlightDetailPanel)
