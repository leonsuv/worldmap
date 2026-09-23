import { useEffect, useState } from 'react'
import { CircleSlash, Crosshair, Download, LocateFixed, Plus, Trash2, TriangleAlert } from 'lucide-react'
import { DrawerShell, Empty, Loading, Section, Stat } from './common'
import { useEvents, EVENT_TYPES, EVENT_COLOR, type EventDraft, type EventType, type MapEvent } from '../store/events'
import { useUi } from '../store/ui'
import { useSelection } from '../store/selection'
import { fitCircle, flyTo } from '../map/runtime'
import { notify } from '../store/notice'
import { fmtDateTime, fmtDistanceKm, fmtNumber, timeAgo } from '../lib/format'

interface Draft {
  name: string
  event_type: EventType
  lat: string
  lon: string
  radius_km: string
  description: string
}

const EMPTY: Draft = { name: '', event_type: 'storm', lat: '', lon: '', radius_km: '100', description: '' }

/** Consume a pending draft from the store as form values. */
function takeDraft(draft: EventDraft | null): Partial<Draft> | null {
  if (!draft) return null
  queueMicrotask(() => useEvents.getState().setDraft(null))
  return { lat: draft.lat.toFixed(4), lon: draft.lon.toFixed(4), name: draft.name ?? '' }
}

function CreateForm({ initial, onDone }: { initial: Partial<Draft>; onDone: () => void }) {
  const create = useEvents(s => s.create)
  const startPick = useUi(s => s.startPick)
  const [d, setD] = useState<Draft>({ ...EMPTY, ...initial })
  const [saving, setSaving] = useState(false)
  const set = (patch: Partial<Draft>) => setD(prev => ({ ...prev, ...patch }))

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    const lat = Number(d.lat)
    const lon = Number(d.lon)
    const radius = Number(d.radius_km)
    if (!d.name.trim()) return notify('Give the event a name.')
    if (!d.lat.trim() || !d.lon.trim() || !Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180) {
      return notify('Choose the event centre on the map or enter valid coordinates.')
    }
    if (!Number.isFinite(radius) || radius <= 0 || radius > 5000) return notify('Radius must be between 0 and 5,000 km.')
    setSaving(true)
    const ok = await create({ name: d.name.trim(), event_type: d.event_type, lat, lon, radius_km: radius, description: d.description.trim() })
    setSaving(false)
    if (ok) {
      fitCircle(lat, lon, radius)
      onDone()
    }
  }

  return (
    <form className="form card" style={{ boxShadow: 'none' }} onSubmit={submit}>
      <label className="field">
        <span>Name</span>
        <input className="input" value={d.name} onChange={e => set({ name: e.target.value })} placeholder="e.g. Storm Ciarán" maxLength={120} autoFocus />
      </label>
      <label className="field">
        <span>Type</span>
        <select className="select" value={d.event_type} onChange={e => set({ event_type: e.target.value as EventType })}>
          {EVENT_TYPES.map(t => (
            <option key={t.key} value={t.key}>
              {t.label}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        <label className="field grow">
          <span>Latitude</span>
          <input className="input" inputMode="decimal" value={d.lat} onChange={e => set({ lat: e.target.value })} />
        </label>
        <label className="field grow">
          <span>Longitude</span>
          <input className="input" inputMode="decimal" value={d.lon} onChange={e => set({ lon: e.target.value })} />
        </label>
      </div>
      <button
        type="button"
        className="btn"
        onClick={() => startPick({ label: 'Click the map to set the event centre', onPick: ([x, y]) => set({ lat: y.toFixed(4), lon: x.toFixed(4) }) })}
      >
        <Crosshair size={15} /> Pick centre on map
      </button>
      <label className="field">
        <span>Radius: {fmtNumber(Number(d.radius_km) || 0, 0, 'km')}</span>
        <input type="range" min={5} max={1500} step={5} value={Number(d.radius_km) || 100} onChange={e => set({ radius_km: e.target.value })} style={{ accentColor: 'var(--accent)' }} />
      </label>
      <label className="field">
        <span>Description (optional)</span>
        <textarea className="textarea" value={d.description} onChange={e => set({ description: e.target.value })} maxLength={2000} />
      </label>
      <div className="row" style={{ justifyContent: 'flex-end' }}>
        <button type="button" className="btn ghost" onClick={onDone}>
          Cancel
        </button>
        <button type="submit" className="btn primary" disabled={saving}>
          {saving ? 'Creating…' : 'Create event'}
        </button>
      </div>
    </form>
  )
}

function Affected({ event }: { event: MapEvent }) {
  const affected = useEvents(s => s.affected)
  const loading = useEvents(s => s.affectedLoading)
  const select = useSelection(s => s.select)
  if (loading) return <Loading label="Finding assets in the area…" />
  if (!affected) return null
  const groups = [
    { key: 'ships', label: 'Vessels', items: affected.ships.map(s => ({ id: String(s.mmsi), name: s.name || `MMSI ${s.mmsi}`, d: s.distance_km, go: () => { flyTo(s.lon, s.lat, 11); select({ kind: 'ship', id: s.mmsi }) } })) },
    { key: 'flights', label: 'Aircraft', items: affected.flights.map(f => ({ id: f.icao24, name: f.callsign || f.icao24, d: f.distance_km, go: () => { flyTo(f.lon, f.lat, 9); select({ kind: 'flight', id: f.icao24 }) } })) },
    { key: 'airports', label: 'Airports', items: affected.airports.map(a => ({ id: a.ident, name: a.name, d: a.distance_km, go: () => { flyTo(a.lon, a.lat, 11); select({ kind: 'airport', id: a.ident, lngLat: [a.lon, a.lat], props: a as unknown as Record<string, unknown> }) } })) },
    { key: 'seaports', label: 'Seaports', items: affected.seaports.map(p => ({ id: p.locode ?? p.name, name: p.name, d: p.distance_km, go: () => { flyTo(p.lon, p.lat, 11); select({ kind: 'seaport', id: p.locode ?? p.name, lngLat: [p.lon, p.lat], props: p as unknown as Record<string, unknown> }) } })) },
    { key: 'reactors', label: 'Nuclear plants', items: affected.reactors.map(r => ({ id: r.name, name: r.name, d: r.distance_km, go: () => flyTo(r.lon, r.lat, 10) })) },
  ]
  return (
    <Section
      title="Inside this area"
      action={
        <a className="btn small ghost" href={`/api/export/csv?type=affected&event_id=${event.id}`} download>
          <Download size={13} /> CSV
        </a>
      }
    >
      <div className="stats" style={{ marginBottom: 10 }}>
        <Stat label="Vessels" value={affected.ships.length} />
        <Stat label="Aircraft" value={affected.flights.length} />
        <Stat label="Sites" value={affected.airports.length + affected.seaports.length + affected.reactors.length} />
      </div>
      {affected.total === 0 && <p className="muted">No tracked assets are inside the area right now.</p>}
      {groups
        .filter(g => g.items.length)
        .map(g => (
          <details key={g.key} open={g.items.length <= 5} style={{ marginBottom: 8 }}>
            <summary style={{ cursor: 'pointer', fontWeight: 600, padding: '4px 0' }}>
              {g.label} ({g.items.length})
            </summary>
            <ul className="list" style={{ marginTop: 6 }}>
              {g.items.slice(0, 25).map(item => (
                <li key={item.id} className="list-item clickable" style={{ gridTemplateColumns: 'minmax(0,1fr) auto' }} onClick={item.go}>
                  <strong>{item.name}</strong>
                  <small>{fmtDistanceKm(item.d)}</small>
                </li>
              ))}
              {g.items.length > 25 && <li className="muted" style={{ padding: 6 }}>and {g.items.length - 25} more (see CSV)</li>}
            </ul>
          </details>
        ))}
    </Section>
  )
}

export default function EventsPanel() {
  const { events, loading, selectedId, fetch, select, close, remove } = useEvents()
  const closePanel = useUi(s => s.closePanel)
  const [creating, setCreating] = useState<Partial<Draft> | null>(() => takeDraft(useEvents.getState().draft))

  useEffect(() => {
    void fetch()
  }, [fetch])

  // Drafts requested while the panel is already open.
  useEffect(
    () =>
      useEvents.subscribe(s => {
        const draft = takeDraft(s.draft)
        if (draft) setCreating(draft)
      }),
    [],
  )

  const selected = events.find(e => e.id === selectedId) ?? null
  const active = events.filter(e => e.active)
  const past = events.filter(e => !e.active)

  const item = (e: MapEvent) => (
    <li
      key={e.id}
      className={`list-item clickable ${e.id === selectedId ? 'selected' : ''} ${e.active ? '' : 'dim'}`}
      onClick={() => {
        select(e.id === selectedId ? null : e.id)
        if (e.id !== selectedId) fitCircle(e.lat, e.lon, e.radius_km)
      }}
    >
      <span className="dot" style={{ '--dot': EVENT_COLOR[e.event_type] } as React.CSSProperties} />
      <span style={{ minWidth: 0 }}>
        <strong>{e.name}</strong>
        <small>
          {EVENT_TYPES.find(t => t.key === e.event_type)?.label ?? e.event_type} · {fmtNumber(e.radius_km, 0, 'km')} ·{' '}
          {e.active ? `since ${timeAgo(e.started_at)}` : `ended ${fmtDateTime(e.ended_at)}`}
        </small>
      </span>
      <span className="list-actions" onClick={ev => ev.stopPropagation()}>
        <button className="icon-btn" title="Zoom to area" aria-label="Zoom to area" onClick={() => fitCircle(e.lat, e.lon, e.radius_km)}>
          <LocateFixed size={15} />
        </button>
        {e.active && (
          <button className="icon-btn" title="End event" aria-label="End event" onClick={() => void close(e.id)}>
            <CircleSlash size={15} />
          </button>
        )}
        <button
          className="icon-btn"
          title="Delete event and its alerts"
          aria-label="Delete event"
          onClick={() => window.confirm(`Delete “${e.name}” and its alerts?`) && void remove(e.id)}
        >
          <Trash2 size={15} />
        </button>
      </span>
    </li>
  )

  return (
    <DrawerShell
      icon={<TriangleAlert size={20} />}
      title="Events"
      subtitle="Storms, outages and closures — see what is inside and get alerts"
      onClose={closePanel}
      footer={!creating && <button className="btn primary block" onClick={() => setCreating({})}><Plus size={15} /> New event</button>}
    >
      {creating && <CreateForm initial={creating} onDone={() => setCreating(null)} />}
      {loading && !events.length ? (
        <Loading />
      ) : events.length === 0 ? (
        !creating && <Empty icon={<TriangleAlert size={26} />}>No events yet. Create one to mark an affected area on the map.</Empty>
      ) : (
        <>
          {active.length > 0 && (
            <Section title={`Active (${active.length})`}>
              <ul className="list">{active.map(item)}</ul>
            </Section>
          )}
          {selected && <Affected event={selected} />}
          {past.length > 0 && (
            <Section title={`Ended (${past.length})`}>
              <ul className="list">{past.map(item)}</ul>
            </Section>
          )}
        </>
      )}
    </DrawerShell>
  )
}
