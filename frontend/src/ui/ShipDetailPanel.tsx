import { memo, useCallback, useEffect } from 'react'
import { useShipDetailStore } from '../store/shipDetail'
import type { ShipDetail } from '../store/shipDetail'
import { Ship, X, ExternalLink } from 'lucide-react'

const NAV_STATUS_LABELS: Record<number, string> = {
  0: 'Under way using engine',
  1: 'At anchor',
  2: 'Not under command',
  3: 'Restricted manoeuvrability',
  4: 'Constrained by draught',
  5: 'Moored',
  6: 'Aground',
  7: 'Engaged in fishing',
  8: 'Under way sailing',
  9: 'Reserved (HSC)',
  10: 'Reserved (WIG)',
  11: 'Power-driven towing astern',
  12: 'Power-driven pushing/towing',
  14: 'AIS-SART active',
  15: 'Not defined',
}

const SHIP_TYPE_LABELS: Record<number, string> = {
  20: 'Wing in ground', 30: 'Fishing', 31: 'Towing', 32: 'Towing (large)',
  33: 'Dredging', 34: 'Diving ops', 35: 'Military ops', 36: 'Sailing',
  37: 'Pleasure craft', 40: 'HSC', 50: 'Pilot vessel', 51: 'SAR vessel',
  52: 'Tug', 53: 'Port tender', 54: 'Anti-pollution', 55: 'Law enforcement',
  60: 'Passenger', 70: 'Cargo', 80: 'Tanker', 90: 'Other',
}

function shipTypeLabel(type_: number | null): string {
  if (type_ == null) return 'Unknown'
  // AIS ship types: first digit is category (6x=passenger, 7x=cargo, 8x=tanker)
  const decade = Math.floor(type_ / 10) * 10
  return SHIP_TYPE_LABELS[type_] ?? SHIP_TYPE_LABELS[decade] ?? `Type ${type_}`
}

function Row({ label, value }: { label: string; value: string | number | null | undefined }) {
  if (value == null || value === '') return null
  return (
    <div className="spd-row">
      <span className="spd-label">{label}</span>
      <span className="spd-value">{value}</span>
    </div>
  )
}

function ShipDetailPanel() {
  const detail = useShipDetailStore((s) => s.detail)
  const close = useShipDetailStore((s) => s.close)

  const onKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') close()
  }, [close])

  useEffect(() => {
    if (!detail) return
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [detail, onKeyDown])

  if (!detail) return null

  const d: ShipDetail = detail
  const name = d.ship_name?.trim() || `MMSI ${d.mmsi}`

  return (
    <div className="spd-panel">
      <button className="spd-close" onClick={close}><X size={16} /></button>

      <div className="spd-header">
        <Ship size={18} />
        <div>
          <h3 className="spd-title">{name}</h3>
          <span className="spd-subtitle">
            {shipTypeLabel(d.ship_type)} · MMSI {d.mmsi}
          </span>
        </div>
      </div>

      <div className="spd-section">
        <h4 className="spd-section-title">Identification</h4>
        <Row label="MMSI" value={d.mmsi} />
        <Row label="IMO" value={d.imo} />
        <Row label="Callsign" value={d.callsign} />
        <Row label="Ship Type" value={d.ship_type != null ? `${d.ship_type} — ${shipTypeLabel(d.ship_type)}` : null} />
      </div>

      <div className="spd-section">
        <h4 className="spd-section-title">Voyage</h4>
        <Row label="Destination" value={d.destination} />
        <Row label="ETA" value={d.eta} />
        <Row label="Draught" value={d.draught != null ? `${d.draught}m` : null} />
        <Row label="Nav Status" value={d.nav_status != null ? NAV_STATUS_LABELS[d.nav_status] ?? `${d.nav_status}` : null} />
      </div>

      <div className="spd-section">
        <h4 className="spd-section-title">Dynamics</h4>
        <Row label="Speed" value={d.speed != null ? `${d.speed.toFixed(1)} kn` : null} />
        <Row label="Course" value={d.course != null ? `${d.course.toFixed(1)}°` : null} />
        <Row label="Heading" value={d.heading != null ? `${d.heading.toFixed(1)}°` : null} />
      </div>

      <div className="spd-section">
        <h4 className="spd-section-title">Dimensions</h4>
        <Row label="Length" value={d.length != null ? `${d.length}m` : null} />
        <Row label="Beam" value={d.beam != null ? `${d.beam}m` : null} />
      </div>

      <div className="spd-links">
        <a
          href={`https://www.marinetraffic.com/en/ais/details/ships/mmsi:${d.mmsi}`}
          target="_blank"
          rel="noopener noreferrer"
          className="spd-link"
        >
          MarineTraffic <ExternalLink size={11} />
        </a>
        <a
          href={`https://www.vesselfinder.com/vessels?name=${d.mmsi}`}
          target="_blank"
          rel="noopener noreferrer"
          className="spd-link"
        >
          VesselFinder <ExternalLink size={11} />
        </a>
        {d.imo && (
          <a
            href={`https://www.marinetraffic.com/en/ais/details/ships/imo:${d.imo}`}
            target="_blank"
            rel="noopener noreferrer"
            className="spd-link"
          >
            IMO {d.imo} <ExternalLink size={11} />
          </a>
        )}
      </div>
    </div>
  )
}

export default memo(ShipDetailPanel)
