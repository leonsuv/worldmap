import { memo, useState, useEffect } from 'react'
import { useWatchlistStore } from '../store/watchlist'
import { useNoticeStore } from '../store/notice'
import { List, Plus, Trash2, Ship, Anchor, MapPin, Atom, Factory, X } from 'lucide-react'

const TYPES = [
  { value: 'vessel', label: 'Vessel', icon: Ship },
  { value: 'port', label: 'Port', icon: Anchor },
  { value: 'area', label: 'Area', icon: MapPin },
  { value: 'reactor', label: 'Reactor', icon: Atom },
  { value: 'pipeline', label: 'Pipeline', icon: Factory },
]

function WatchlistPanel() {
  const { items, open, loading, toggle, fetch, add, remove } = useWatchlistStore()
  const [adding, setAdding] = useState(false)
  const [wtype, setWtype] = useState('vessel')
  const [name, setName] = useState('')
  const [mmsi, setMmsi] = useState('')
  const [lat, setLat] = useState('')
  const [lon, setLon] = useState('')
  const [saving, setSaving] = useState(false)

  useEffect(() => { if (open) fetch() }, [open, fetch])

  if (!open) return null

  const handleAdd = async () => {
    if (!name.trim() || saving) return
    let params: Record<string, unknown>
    if (wtype === 'vessel') {
      if (!/^\d{9}$/.test(mmsi.trim())) {
        useNoticeStore.getState().show('Enter a 9-digit MMSI to track this vessel.')
        return
      }
      params = { mmsi: Number(mmsi) }
    } else {
      if (!lat.trim() || !lon.trim() || !Number.isFinite(Number(lat)) || !Number.isFinite(Number(lon)) || Math.abs(Number(lat)) > 90 || Math.abs(Number(lon)) > 180) {
        useNoticeStore.getState().show('Enter valid latitude and longitude for this location.')
        return
      }
      params = { lat: Number(lat), lon: Number(lon) }
    }
    setSaving(true)
    const saved = await add(wtype, name.trim(), params)
    setSaving(false)
    if (!saved) return
    setName('')
    setMmsi('')
    setLat('')
    setLon('')
    setAdding(false)
  }

  return (
    <div className="wl-panel">
      <button aria-label="Close panel" className="wl-close" onClick={toggle}><X size={16} /></button>
      <div className="wl-header">
        <List size={18} />
        <h2 className="wl-title">Watchlist</h2>
        <span className="wl-count">{items.length}</span>
      </div>

      {loading && <div className="wl-loading">Loading…</div>}

      <div className="wl-items">
        {items.map(item => {
          const TypeDef = TYPES.find(t => t.value === item.wtype)
          const Icon = TypeDef?.icon ?? MapPin
          return (
            <div key={item.id} className="wl-item">
              <Icon size={14} />
              <span className="wl-item-name">{item.name}</span>
              <span className="wl-item-type">{item.wtype}</span>
              <button aria-label={`Remove ${item.name}`} className="wl-item-del" onClick={() => remove(item.id)}>
                <Trash2 size={12} />
              </button>
            </div>
          )
        })}
        {!loading && items.length === 0 && (
          <div className="wl-empty">No items on your watchlist</div>
        )}
      </div>

      {adding ? (
        <div className="wl-add-form">
          <select aria-label="Watchlist type" value={wtype} onChange={e => setWtype(e.target.value)} className="wl-select">
            {TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
          </select>
          <input
            className="wl-input"
            aria-label="Watchlist name"
            placeholder="Name…"
            value={name}
            onChange={e => setName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleAdd()}
            autoFocus
          />
          {wtype === 'vessel' ? (
            <input className="wl-input" aria-label="MMSI" placeholder="9-digit MMSI" inputMode="numeric" value={mmsi} onChange={e => setMmsi(e.target.value)} />
          ) : <>
            <input className="wl-input" aria-label="Latitude" placeholder="Latitude (−90 to 90)" type="number" min="-90" max="90" step="any" value={lat} onChange={e => setLat(e.target.value)} />
            <input className="wl-input" aria-label="Longitude" placeholder="Longitude (−180 to 180)" type="number" min="-180" max="180" step="any" value={lon} onChange={e => setLon(e.target.value)} />
          </>}
          <button className="wl-add-btn" disabled={saving} onClick={handleAdd}><Plus size={14} /> {saving ? 'Saving…' : 'Add'}</button>
        </div>
      ) : (
        <button className="wl-add-trigger" onClick={() => setAdding(true)}>
          <Plus size={14} /> Add to Watchlist
        </button>
      )}
    </div>
  )
}

export default memo(WatchlistPanel)
