import { memo, useState, useEffect } from 'react'
import { useEventStore } from '../store/events'
import { AlertTriangle, Plus, X, Eye, Trash2, XCircle } from 'lucide-react'

const EVENT_TYPES = ['storm', 'outage', 'closure', 'geopolitical', 'custom']
const EVENT_COLORS: Record<string, string> = {
  storm: '#f44336',
  outage: '#ff9800',
  closure: '#9c27b0',
  geopolitical: '#e91e63',
  custom: '#2196f3',
}

function EventPanel() {
  const { events, open, selectedId, affected, toggle, fetch, create, close, remove, select } = useEventStore()
  const [adding, setAdding] = useState(false)
  const [form, setForm] = useState({ name: '', event_type: 'storm', lat: '', lon: '', radius_km: '50', description: '' })

  useEffect(() => { if (open) fetch() }, [open, fetch])

  if (!open) return null

  const handleCreate = () => {
    if (!form.name || !form.lat || !form.lon) return
    create({
      name: form.name,
      event_type: form.event_type,
      lat: parseFloat(form.lat),
      lon: parseFloat(form.lon),
      radius_km: parseFloat(form.radius_km) || 50,
      description: form.description,
    })
    setForm({ name: '', event_type: 'storm', lat: '', lon: '', radius_km: '50', description: '' })
    setAdding(false)
  }

  const selectedEvent = events.find(e => e.id === selectedId)

  return (
    <div className="ev-panel">
      <button className="ev-close" onClick={toggle}><X size={16} /></button>
      <div className="ev-header">
        <AlertTriangle size={18} />
        <h2 className="ev-title">Events</h2>
        <span className="ev-count">{events.filter(e => e.active).length} active</span>
      </div>

      <div className="ev-list">
        {events.map(ev => (
          <div
            key={ev.id}
            className={`ev-item ${ev.id === selectedId ? 'selected' : ''} ${ev.active ? '' : 'closed'}`}
          >
            <span className="ev-dot" style={{ background: EVENT_COLORS[ev.event_type] || '#888' }} />
            <div className="ev-item-info" onClick={() => select(ev.id === selectedId ? null : ev.id)}>
              <span className="ev-item-name">{ev.name}</span>
              <span className="ev-item-type">{ev.event_type} · {ev.radius_km} km</span>
            </div>
            <div className="ev-item-actions">
              <button onClick={() => select(ev.id)} title="View affected"><Eye size={12} /></button>
              {ev.active && <button onClick={() => close(ev.id)} title="Close event"><XCircle size={12} /></button>}
              <button onClick={() => remove(ev.id)} title="Delete"><Trash2 size={12} /></button>
            </div>
          </div>
        ))}
        {events.length === 0 && <div className="ev-empty">No events created yet</div>}
      </div>

      {/* Affected assets for selected event */}
      {selectedEvent && affected && (
        <div className="ev-affected">
          <h3 className="ev-affected-title">Affected Assets — {selectedEvent.name}</h3>
          <div className="ev-affected-summary">
            <span>{affected.ships.length} ships</span>
            <span>{affected.airports.length} airports</span>
            <span>{affected.seaports.length} seaports</span>
            <span>{affected.reactors.length} reactors</span>
            <strong>{affected.total} total</strong>
          </div>
          {affected.ships.length > 0 && (
            <div className="ev-affected-section">
              <h4>Ships</h4>
              {affected.ships.slice(0, 10).map((s, i) => (
                <div key={i} className="ev-affected-row">
                  {String(s.ship_name || s.mmsi)} — {Number(s.speed || 0).toFixed(1)} kn
                </div>
              ))}
              {affected.ships.length > 10 && <div className="ev-affected-more">+{affected.ships.length - 10} more</div>}
            </div>
          )}
        </div>
      )}

      {/* Create form */}
      {adding ? (
        <div className="ev-form">
          <input className="ev-input" placeholder="Event name" value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} autoFocus />
          <select className="ev-select" value={form.event_type} onChange={e => setForm(f => ({ ...f, event_type: e.target.value }))}>
            {EVENT_TYPES.map(t => <option key={t} value={t}>{t}</option>)}
          </select>
          <div className="ev-form-row">
            <input className="ev-input ev-input-half" placeholder="Lat" value={form.lat} onChange={e => setForm(f => ({ ...f, lat: e.target.value }))} />
            <input className="ev-input ev-input-half" placeholder="Lon" value={form.lon} onChange={e => setForm(f => ({ ...f, lon: e.target.value }))} />
          </div>
          <input className="ev-input" placeholder="Radius (km)" value={form.radius_km} onChange={e => setForm(f => ({ ...f, radius_km: e.target.value }))} />
          <input className="ev-input" placeholder="Description (optional)" value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))} />
          <button className="ev-create-btn" onClick={handleCreate}>Create Event</button>
        </div>
      ) : (
        <button className="ev-add-trigger" onClick={() => setAdding(true)}>
          <Plus size={14} /> New Event
        </button>
      )}
    </div>
  )
}

export default memo(EventPanel)
