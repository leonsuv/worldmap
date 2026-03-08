import { memo, useCallback, useEffect, useRef } from 'react'
import { usePopupStore } from '../store/popup'
import { mapInstance } from '../map/MapContainer'

const EXTERNAL_LINKS: Record<string, (p: Record<string, unknown>) => { label: string; href: string } | null> = {
  flights: (p) => p.callsign ? { label: 'Flightradar24', href: `https://www.flightradar24.com/${String(p.callsign).trim()}` } : null,
  ships: (p) => p.mmsi ? { label: 'MarineTraffic', href: `https://www.marinetraffic.com/en/ais/details/ships/mmsi:${p.mmsi}` } : null,
}

function formatValue(v: unknown): string {
  if (v == null) return '—'
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(1)
  return String(v)
}

function InfoPopup() {
  const { lngLat, layerId, properties, close } = usePopupStore()
  const cardRef = useRef<HTMLDivElement>(null)

  // Escape key
  useEffect(() => {
    if (!lngLat) return
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') close() }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [lngLat, close])

  // Click outside
  const onPointerDown = useCallback(
    (e: PointerEvent) => {
      if (cardRef.current && !cardRef.current.contains(e.target as Node)) close()
    },
    [close],
  )
  useEffect(() => {
    if (!lngLat) return
    window.addEventListener('pointerdown', onPointerDown, true)
    return () => window.removeEventListener('pointerdown', onPointerDown, true)
  }, [lngLat, onPointerDown])

  if (!lngLat || !layerId || !properties) return null

  // Project lngLat to screen position
  const point = mapInstance?.project(lngLat)
  if (!point) return null

  const ext = EXTERNAL_LINKS[layerId]?.(properties)

  return (
    <div
      ref={cardRef}
      className="info-popup"
      style={{ left: point.x, top: point.y }}
    >
      <button className="info-popup-close" onClick={close}>×</button>
      <h3 className="info-popup-title">{layerId}</h3>
      <table className="info-popup-table">
        <tbody>
          {Object.entries(properties).map(([k, v]) => (
            <tr key={k}>
              <td className="info-popup-key">{k.replace(/_/g, ' ')}</td>
              <td className="info-popup-val">{formatValue(v)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {ext && (
        <a className="info-popup-link" href={ext.href} target="_blank" rel="noopener noreferrer">
          Open in {ext.label} ↗
        </a>
      )}
    </div>
  )
}

export default memo(InfoPopup)
