import { useEffect, useState } from 'react'
import { Anchor, Atom, Crosshair, ListChecks, MapPin, Plane, Plus, Ship, Trash2, Factory } from 'lucide-react'
import { DrawerShell, Empty, Loading } from './common'
import { useWatchlist, type WatchItem, type WatchType } from '../store/watchlist'
import { useUi } from '../store/ui'
import { useSelection } from '../store/selection'
import { flyTo } from '../map/runtime'
import { notify } from '../store/notice'
import { fmtCoord, fmtKnots, timeAgo } from '../lib/format'

const TYPES: { value: WatchType; label: string; icon: typeof Ship }[] = [
  { value: 'vessel', label: 'Vessel', icon: Ship },
  { value: 'port', label: 'Port', icon: Anchor },
  { value: 'airport', label: 'Airport', icon: Plane },
  { value: 'reactor', label: 'Nuclear plant', icon: Atom },
  { value: 'area', label: 'Area', icon: MapPin },
  { value: 'pipeline', label: 'Other asset', icon: Factory },
]

function describe(item: WatchItem): string {
  if (item.wtype === 'vessel') {
    if (!item.live) return `MMSI ${item.params.mmsi} · not in the live picture`
    return `${fmtKnots(item.live.speed)} · reported ${timeAgo(Date.now() / 1000 - item.live.age_secs)}`
  }
  const { lat, lon, radius_km } = item.params
  if (lat == null || lon == null) return ''
  return `${fmtCoord(lat, lon)}${radius_km ? ` · ${radius_km} km` : ''}`
}

function AddForm({ onDone }: { onDone: () => void }) {
  const add = useWatchlist(s => s.add)
  const startPick = useUi(s => s.startPick)
  const [wtype, setWtype] = useState<WatchType>('area')
  const [name, setName] = useState('')
  const [mmsi, setMmsi] = useState('')
  const [lat, setLat] = useState('')
  const [lon, setLon] = useState('')
  const [radius, setRadius] = useState('25')
  const [saving, setSaving] = useState(false)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    let params: WatchItem['params']
    if (wtype === 'vessel') {
      if (!/^\d{9}$/.test(mmsi.trim())) return notify('Enter the vessel’s 9-digit MMSI.')
      params = { mmsi: Number(mmsi) }
    } else {
      const la = Number(lat)
      const lo = Number(lon)
      if (!lat.trim() || !lon.trim() || !Number.isFinite(la) || !Number.isFinite(lo) || Math.abs(la) > 90 || Math.abs(lo) > 180) {
        return notify('Choose a location on the map or enter valid coordinates.')
      }
      params = { lat: la, lon: lo }
      if (wtype === 'area') {
        const r = Number(radius)
        if (!Number.isFinite(r) || r <= 0 || r > 5000) return notify('Radius must be between 0 and 5,000 km.')
        params.radius_km = r
      }
    }
    setSaving(true)
    const ok = await add(wtype, name.trim() || (wtype === 'vessel' ? `MMSI ${mmsi}` : 'Watched area'), params)
    setSaving(false)
    if (ok) onDone()
  }

  return (
    <form className="form card" style={{ boxShadow: 'none' }} onSubmit={submit}>
      <label className="field">
        <span>Type</span>
        <select className="select" value={wtype} onChange={e => setWtype(e.target.value as WatchType)}>
          {TYPES.map(t => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>Name</span>
        <input className="input" value={name} onChange={e => setName(e.target.value)} placeholder={wtype === 'vessel' ? 'e.g. Ever Given' : 'e.g. Port of Rotterdam'} maxLength={120} />
      </label>
      {wtype === 'vessel' ? (
        <label className="field">
          <span>MMSI</span>
          <input className="input" inputMode="numeric" value={mmsi} onChange={e => setMmsi(e.target.value.replace(/\D/g, '').slice(0, 9))} placeholder="9 digits" />
        </label>
      ) : (
        <>
          <div className="row">
            <label className="field grow">
              <span>Latitude</span>
              <input className="input" inputMode="decimal" value={lat} onChange={e => setLat(e.target.value)} placeholder="−90 … 90" />
            </label>
            <label className="field grow">
              <span>Longitude</span>
              <input className="input" inputMode="decimal" value={lon} onChange={e => setLon(e.target.value)} placeholder="−180 … 180" />
            </label>
          </div>
          <button
            type="button"
            className="btn"
            onClick={() =>
              startPick({
                label: 'Click the map to place the watched location',
                onPick: ([x, y]) => {
                  setLat(y.toFixed(4))
                  setLon(x.toFixed(4))
                },
              })
            }
          >
            <Crosshair size={15} /> Pick on map
          </button>
          {wtype === 'area' && (
            <label className="field">
              <span>Radius (km)</span>
              <input className="input" inputMode="decimal" value={radius} onChange={e => setRadius(e.target.value)} />
            </label>
          )}
        </>
      )}
      <div className="row" style={{ justifyContent: 'flex-end' }}>
        <button type="button" className="btn ghost" onClick={onDone}>
          Cancel
        </button>
        <button type="submit" className="btn primary" disabled={saving}>
          {saving ? 'Saving…' : 'Add'}
        </button>
      </div>
    </form>
  )
}

export default function WatchlistPanel() {
  const { items, loading, loaded, fetch, remove } = useWatchlist()
  const close = useUi(s => s.closePanel)
  const [adding, setAdding] = useState(false)

  useEffect(() => {
    void fetch()
    const id = setInterval(() => !document.hidden && void useWatchlist.getState().fetch(), 30_000)
    return () => clearInterval(id)
  }, [fetch])

  const open = (item: WatchItem) => {
    if (item.wtype === 'vessel' && item.params.mmsi) {
      if (item.live) flyTo(item.live.lon, item.live.lat, 11)
      useSelection.getState().select({ kind: 'ship', id: item.params.mmsi })
    } else if (item.params.lat != null && item.params.lon != null) {
      flyTo(item.params.lon, item.params.lat, item.wtype === 'area' ? 8 : 11)
    }
  }

  return (
    <DrawerShell
      icon={<ListChecks size={20} />}
      title="Watchlist"
      subtitle="Vessels, sites and areas that raise alerts when an event affects them"
      onClose={close}
      footer={!adding && <button className="btn primary block" onClick={() => setAdding(true)}><Plus size={15} /> Add to watchlist</button>}
    >
      {adding && <AddForm onDone={() => setAdding(false)} />}
      {loading && !loaded ? (
        <Loading />
      ) : items.length === 0 ? (
        !adding && (
          <Empty icon={<ListChecks size={26} />}>
            Nothing watched yet. Add a vessel by MMSI, or use <b>Watch</b> in any airport, port or plant panel.
          </Empty>
        )
      ) : (
        <ul className="list">
          {items.map(item => {
            const Icon = TYPES.find(t => t.value === item.wtype)?.icon ?? MapPin
            return (
              <li key={item.id} className="list-item clickable" onClick={() => open(item)}>
                <Icon size={16} color="var(--accent)" />
                <span style={{ minWidth: 0 }}>
                  <strong>{item.name}</strong>
                  <small>{describe(item)}</small>
                </span>
                <span className="list-actions">
                  <button
                    className="icon-btn"
                    aria-label={`Remove ${item.name}`}
                    title="Remove"
                    onClick={e => {
                      e.stopPropagation()
                      void remove(item.id)
                    }}
                  >
                    <Trash2 size={15} />
                  </button>
                </span>
              </li>
            )
          })}
        </ul>
      )}
    </DrawerShell>
  )
}
