import { memo, useState, useEffect } from 'react'
import { useWatchlistStore } from '../store/watchlist'
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

  useEffect(() => { if (open) fetch() }, [open, fetch])

  if (!open) return null

  const handleAdd = () => {
    if (!name.trim()) return
    add(wtype, name.trim())
    setName('')
    setAdding(false)
  }

  return (
    <div className="wl-panel">
      <button className="wl-close" onClick={toggle}><X size={16} /></button>
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
              <button className="wl-item-del" onClick={() => remove(item.id)}>
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
          <select value={wtype} onChange={e => setWtype(e.target.value)} className="wl-select">
            {TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
          </select>
          <input
            className="wl-input"
            placeholder="Name / MMSI…"
            value={name}
            onChange={e => setName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleAdd()}
            autoFocus
          />
          <button className="wl-add-btn" onClick={handleAdd}><Plus size={14} /> Add</button>
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
